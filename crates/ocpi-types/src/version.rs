//! OCPI version negotiation primitives.
//!
//! Two parties discover each other's supported versions via the `/versions`
//! endpoint, then agree on a shared one. This module models the version
//! identifiers, the `/versions` list entry, and the per-version endpoint
//! details returned by `GET /versions/{version}`.
//!
//! Spec: `specs/ocpi/2.2.1/version_information_endpoint.asciidoc`

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::common::Url;
use crate::OcpiError;

// ── VersionNumber ─────────────────────────────────────────────────────────────

/// An OCPI protocol version, serialized as its canonical dotted string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum VersionNumber {
    /// OCPI 2.0.
    #[serde(rename = "2.0")]
    V2_0,
    /// OCPI 2.1.1.
    #[serde(rename = "2.1.1")]
    V2_1_1,
    /// OCPI 2.2.
    #[serde(rename = "2.2")]
    V2_2,
    /// OCPI 2.2.1.
    #[serde(rename = "2.2.1")]
    V2_2_1,
    /// OCPI 2.3.0.
    #[serde(rename = "2.3.0")]
    V2_3_0,
    /// OCPI 3.0 — **recognition-only** (logic deferred, `blocked-upstream`).
    ///
    /// 3.0 is developed in a separate, access-restricted repository and is not
    /// vendored here, so the SDK **recognises** the identifier (parses, orders
    /// it highest, and round-trips `"3.0"`) but ships **no `v3_0` type surface**.
    /// Because no `supported` set the crate ships includes `V3_0`, version
    /// negotiation can never *select* it: a 3.0-only partner degrades to *no
    /// common version*, which the caller maps to an explicit `UnsupportedVersion`
    /// `status_code` — the top-of-range mirror of the recognition-only 2.0 slice
    /// (`V2_0`). See [`specs/ocpi/3.0/README.md`] — *"defer logic, not schema."*
    ///
    /// [`specs/ocpi/3.0/README.md`]: https://github.com/evlinked/ocpi-rs/blob/main/specs/ocpi/3.0/README.md
    #[serde(rename = "3.0")]
    V3_0,
}

impl VersionNumber {
    /// The canonical dotted version string (e.g. `"2.2.1"`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::V2_0 => "2.0",
            Self::V2_1_1 => "2.1.1",
            Self::V2_2 => "2.2",
            Self::V2_2_1 => "2.2.1",
            Self::V2_3_0 => "2.3.0",
            Self::V3_0 => "3.0",
        }
    }
}

impl FromStr for VersionNumber {
    type Err = OcpiError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "2.0" => Ok(Self::V2_0),
            "2.1.1" => Ok(Self::V2_1_1),
            "2.2" => Ok(Self::V2_2),
            "2.2.1" => Ok(Self::V2_2_1),
            "2.3.0" => Ok(Self::V2_3_0),
            "3.0" => Ok(Self::V3_0),
            _ => Err(OcpiError::Invalid(format!("unknown OCPI version: {s}"))),
        }
    }
}

impl fmt::Display for VersionNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── Version ───────────────────────────────────────────────────────────────────

/// An entry in the `/versions` list: a supported version and where to fetch
/// its details.
///
/// Spec: `specs/ocpi/2.2.1/version_information_endpoint.asciidoc` — Version class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version {
    /// The supported OCPI version.
    pub version: VersionNumber,
    /// URL of the endpoint listing this version's module details.
    pub url: Url,
}

// ── ModuleID ──────────────────────────────────────────────────────────────────

/// Module identifier as it appears on the wire in an `Endpoint` object.
///
/// Each standard module has a fixed lowercase ASCII identifier. The OCPI spec
/// also lets parties advertise **custom or customized modules** whose
/// identifier is any other string (conventionally prefixed with country-code +
/// party-id, e.g. `"nltnm-tokens"`). Such an id is preserved verbatim in
/// [`ModuleID::Custom`] and round-trips byte-for-byte, so a single custom entry
/// in a partner's `GET /versions/{version}` catalogue no longer fails the whole
/// payload — mirroring the raw-preserving
/// [`OcpiStatusCode::Unknown`](crate::OcpiStatusCode::Unknown) precedent
/// (tolerant where the spec is open, strict where it is closed).
///
/// A `Custom` id is opaque data: it never compares equal to a standard variant,
/// so routing / endpoint-selection that matches against the known modules
/// ignores it (per the spec's "only send to parties you have an agreement
/// with").
///
/// Spec: `specs/ocpi/2.2.1/version_information_endpoint.asciidoc` — ModuleID
/// enum, `===== Custom Modules`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ModuleID {
    /// Charge Detail Records module.
    Cdrs,
    /// Charging Profiles module (`"chargingprofiles"`).
    ChargingProfiles,
    /// Commands module.
    Commands,
    /// Credentials & Registration module (required for all implementations).
    Credentials,
    /// Hub Client Info module (`"hubclientinfo"`).
    HubClientInfo,
    /// Locations module.
    Locations,
    /// Payments module (`"payments"`) — introduced in OCPI 2.3.0.
    Payments,
    /// Sessions module.
    Sessions,
    /// Tariffs module.
    Tariffs,
    /// Tokens module.
    Tokens,
    /// A custom or customized module ID the spec permits (`===== Custom
    /// Modules`), preserved verbatim. Opaque to routing — never matches a
    /// standard module.
    Custom(String),
}

impl ModuleID {
    /// The wire identifier string for this module.
    ///
    /// Standard variants return their fixed lowercase identifier; a
    /// [`ModuleID::Custom`] returns its preserved raw string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Cdrs => "cdrs",
            Self::ChargingProfiles => "chargingprofiles",
            Self::Commands => "commands",
            Self::Credentials => "credentials",
            Self::HubClientInfo => "hubclientinfo",
            Self::Locations => "locations",
            Self::Payments => "payments",
            Self::Sessions => "sessions",
            Self::Tariffs => "tariffs",
            Self::Tokens => "tokens",
            Self::Custom(s) => s.as_str(),
        }
    }

    /// Whether this is a custom (non-standard) module ID.
    #[must_use]
    pub fn is_custom(&self) -> bool {
        matches!(self, Self::Custom(_))
    }
}

impl From<&str> for ModuleID {
    /// Maps a wire identifier to its standard variant, or preserves an unknown
    /// id verbatim as [`ModuleID::Custom`]. A standard id never lands in
    /// `Custom`.
    fn from(s: &str) -> Self {
        match s {
            "cdrs" => Self::Cdrs,
            "chargingprofiles" => Self::ChargingProfiles,
            "commands" => Self::Commands,
            "credentials" => Self::Credentials,
            "hubclientinfo" => Self::HubClientInfo,
            "locations" => Self::Locations,
            "payments" => Self::Payments,
            "sessions" => Self::Sessions,
            "tariffs" => Self::Tariffs,
            "tokens" => Self::Tokens,
            other => Self::Custom(other.to_owned()),
        }
    }
}

impl Serialize for ModuleID {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ModuleID {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Deserialize as a raw string, then fold known ids onto their fixed
        // variant and preserve anything else as `Custom`. Unlike a closed
        // `#[derive(Deserialize)]` enum, an unrecognised id is data, not an
        // error — so one custom endpoint never fails the whole VersionDetails.
        let raw = String::deserialize(deserializer)?;
        Ok(Self::from(raw.as_str()))
    }
}

// ── InterfaceRole ─────────────────────────────────────────────────────────────

/// Which side of the OCPI data-flow interface an endpoint implements.
///
/// Spec: `specs/ocpi/2.2.1/version_information_endpoint.asciidoc` — InterfaceRole enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InterfaceRole {
    /// Data owner; other parties pull data from this endpoint.
    Sender,
    /// Data consumer; the data owner pushes to this endpoint.
    Receiver,
}

// ── Endpoint ──────────────────────────────────────────────────────────────────

/// A single module endpoint entry within a [`VersionDetails`] response.
///
/// Spec: `specs/ocpi/2.2.1/version_information_endpoint.asciidoc` — Endpoint class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    /// Which module this endpoint belongs to.
    pub identifier: ModuleID,
    /// Whether this is the sender (owner) or receiver (consumer) side.
    ///
    /// Note: for `credentials`, the role field has no functional significance;
    /// by convention, send `SENDER` for your own credentials endpoint.
    pub role: InterfaceRole,
    /// URL to call for this module endpoint.
    pub url: Url,
}

// ── VersionDetails ────────────────────────────────────────────────────────────

/// Response body for `GET /versions/{version}`.
///
/// Lists all supported module endpoints for a specific OCPI version.
///
/// Spec: `specs/ocpi/2.2.1/version_information_endpoint.asciidoc` — VersionDetails.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionDetails {
    /// The OCPI version these endpoints implement.
    pub version: VersionNumber,
    /// All supported module endpoints for this version.
    pub endpoints: Vec<Endpoint>,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── VersionNumber ──

    #[test]
    fn version_number_ord_ascending_order() {
        assert!(VersionNumber::V2_0 < VersionNumber::V2_1_1);
        assert!(VersionNumber::V2_1_1 < VersionNumber::V2_2);
        assert!(VersionNumber::V2_2 < VersionNumber::V2_2_1);
        assert!(VersionNumber::V2_2_1 < VersionNumber::V2_3_0);
        // 3.0 is recognised and orders highest (recognition-only, #219).
        assert!(VersionNumber::V2_3_0 < VersionNumber::V3_0);
    }

    #[test]
    fn version_number_serializes_as_dotted_string() {
        assert_eq!(
            serde_json::to_string(&VersionNumber::V2_2_1).unwrap(),
            "\"2.2.1\""
        );
    }

    #[test]
    fn version_number_from_str_roundtrip() {
        for (s, v) in [
            ("2.0", VersionNumber::V2_0),
            ("2.1.1", VersionNumber::V2_1_1),
            ("2.2", VersionNumber::V2_2),
            ("2.2.1", VersionNumber::V2_2_1),
            ("2.3.0", VersionNumber::V2_3_0),
            // 3.0 is recognised (recognition-only forward-scaffold, #219).
            ("3.0", VersionNumber::V3_0),
        ] {
            assert_eq!(s.parse::<VersionNumber>().unwrap(), v, "parse {s}");
            assert_eq!(v.as_str(), s, "as_str for {s}");
            assert_eq!(v.to_string(), s, "Display for {s}");
        }
    }

    #[test]
    fn version_number_v3_0_serde_round_trips() {
        // Recognition-only: `"3.0"` parses and round-trips through serde so a
        // partner catalogue that merely *lists* 3.0 deserializes cleanly rather
        // than failing the whole payload (#219).
        let json = serde_json::to_string(&VersionNumber::V3_0).unwrap();
        assert_eq!(json, "\"3.0\"");
        let back: VersionNumber = serde_json::from_str(&json).unwrap();
        assert_eq!(back, VersionNumber::V3_0);
    }

    #[test]
    fn version_number_from_str_unknown() {
        // A genuinely-unknown version is still a hard error — recognition of
        // 3.0 must not loosen rejection of versions the SDK does not know.
        assert!("9.9".parse::<VersionNumber>().is_err());
        assert!("2.4".parse::<VersionNumber>().is_err());
        assert!("".parse::<VersionNumber>().is_err());
    }

    #[test]
    fn version_entry_round_trips() {
        let v = Version {
            version: VersionNumber::V2_3_0,
            url: Url::try_from("https://example.com/2.3.0").unwrap(),
        };
        let json = serde_json::to_string(&v).unwrap();
        let back: Version = serde_json::from_str(&json).unwrap();
        assert_eq!(back, v);
        assert_eq!(back.version.as_str(), "2.3.0");
    }

    // ── ModuleID ──

    #[test]
    fn module_id_serde_roundtrip() {
        let cases: &[(ModuleID, &str)] = &[
            (ModuleID::Cdrs, "\"cdrs\""),
            (ModuleID::ChargingProfiles, "\"chargingprofiles\""),
            (ModuleID::Commands, "\"commands\""),
            (ModuleID::Credentials, "\"credentials\""),
            (ModuleID::HubClientInfo, "\"hubclientinfo\""),
            (ModuleID::Locations, "\"locations\""),
            (ModuleID::Sessions, "\"sessions\""),
            (ModuleID::Tariffs, "\"tariffs\""),
            (ModuleID::Tokens, "\"tokens\""),
        ];
        for (id, expected_json) in cases {
            let json = serde_json::to_string(id).unwrap();
            assert_eq!(json, *expected_json, "serialize {id:?}");
            let back: ModuleID = serde_json::from_str(&json).unwrap();
            assert_eq!(back, *id, "deserialize {id:?}");
        }
    }

    #[test]
    fn module_id_payments_serde_roundtrip() {
        // `payments` (2.3.0) is a standard variant, not a custom id.
        assert_eq!(
            serde_json::to_string(&ModuleID::Payments).unwrap(),
            "\"payments\""
        );
        let back: ModuleID = serde_json::from_str("\"payments\"").unwrap();
        assert_eq!(back, ModuleID::Payments);
        assert!(!back.is_custom());
    }

    #[test]
    fn module_id_custom_round_trips_byte_for_byte() {
        // A spec-allowed custom module id (`===== Custom Modules`,
        // `version_information_endpoint.asciidoc:189-194`) deserializes into a
        // raw-preserving `Custom` and re-serializes verbatim.
        let json = "\"nltnm-tokens\"";
        let id: ModuleID = serde_json::from_str(json).unwrap();
        assert_eq!(id, ModuleID::Custom("nltnm-tokens".to_owned()));
        assert!(id.is_custom());
        assert_eq!(id.as_str(), "nltnm-tokens");
        assert_eq!(serde_json::to_string(&id).unwrap(), json);
    }

    #[test]
    fn module_id_standard_never_lands_in_custom() {
        // Every standard wire id folds onto its fixed variant, not `Custom`.
        for s in [
            "cdrs",
            "chargingprofiles",
            "commands",
            "credentials",
            "hubclientinfo",
            "locations",
            "payments",
            "sessions",
            "tariffs",
            "tokens",
        ] {
            let id: ModuleID = serde_json::from_str(&format!("\"{s}\"")).unwrap();
            assert!(!id.is_custom(), "{s} must not be Custom");
            assert_eq!(id.as_str(), s);
        }
    }

    #[test]
    fn module_id_custom_is_opaque_to_routing() {
        // A custom id never compares equal to any standard module, so
        // endpoint-selection logic that matches known modules ignores it.
        let custom = ModuleID::Custom("nltnm-tokens".to_owned());
        for standard in [
            ModuleID::Cdrs,
            ModuleID::ChargingProfiles,
            ModuleID::Commands,
            ModuleID::Credentials,
            ModuleID::HubClientInfo,
            ModuleID::Locations,
            ModuleID::Payments,
            ModuleID::Sessions,
            ModuleID::Tariffs,
            ModuleID::Tokens,
        ] {
            assert_ne!(custom, standard);
        }
    }

    #[test]
    fn version_details_mixes_standard_and_custom_endpoints() {
        // The load-bearing handshake fence: a `VersionDetails` that lists a
        // custom endpoint alongside standard ones deserializes successfully,
        // with the standard endpoints intact and the custom one preserved —
        // instead of the whole catalogue failing on the unknown id.
        let json = r#"{
            "version": "2.2.1",
            "endpoints": [
                {
                    "identifier": "credentials",
                    "role": "SENDER",
                    "url": "https://example.com/ocpi/cpo/2.2.1/credentials"
                },
                {
                    "identifier": "locations",
                    "role": "SENDER",
                    "url": "https://example.com/ocpi/cpo/2.2.1/locations"
                },
                {
                    "identifier": "nltnm-tokens",
                    "role": "SENDER",
                    "url": "https://example.com/ocpi/cpo/2.2.1/nltnm-tokens"
                }
            ]
        }"#;
        let details: VersionDetails = serde_json::from_str(json).unwrap();
        assert_eq!(details.endpoints.len(), 3);
        assert_eq!(details.endpoints[0].identifier, ModuleID::Credentials);
        assert_eq!(details.endpoints[1].identifier, ModuleID::Locations);
        assert_eq!(
            details.endpoints[2].identifier,
            ModuleID::Custom("nltnm-tokens".to_owned())
        );
        // Full VersionDetails round-trips, custom id preserved verbatim.
        let back: VersionDetails =
            serde_json::from_str(&serde_json::to_string(&details).unwrap()).unwrap();
        assert_eq!(back, details);
    }

    // ── InterfaceRole ──

    #[test]
    fn interface_role_serde_roundtrip() {
        assert_eq!(
            serde_json::to_string(&InterfaceRole::Sender).unwrap(),
            "\"SENDER\""
        );
        assert_eq!(
            serde_json::to_string(&InterfaceRole::Receiver).unwrap(),
            "\"RECEIVER\""
        );
        let s: InterfaceRole = serde_json::from_str("\"SENDER\"").unwrap();
        assert_eq!(s, InterfaceRole::Sender);
        let r: InterfaceRole = serde_json::from_str("\"RECEIVER\"").unwrap();
        assert_eq!(r, InterfaceRole::Receiver);
    }

    // ── VersionDetails ──

    #[test]
    fn version_details_serde_spec_example_1() {
        // Mirrors the spec: CPO with credentials + locations.
        // Spec ref: version_information_endpoint.asciidoc, GET /versions/{version} example 1.
        let json = r#"{
            "version": "2.2.1",
            "endpoints": [
                {
                    "identifier": "credentials",
                    "role": "SENDER",
                    "url": "https://example.com/ocpi/cpo/2.2.1/credentials"
                },
                {
                    "identifier": "locations",
                    "role": "SENDER",
                    "url": "https://example.com/ocpi/cpo/2.2.1/locations"
                }
            ]
        }"#;
        let details: VersionDetails = serde_json::from_str(json).unwrap();
        assert_eq!(details.version, VersionNumber::V2_2_1);
        assert_eq!(details.endpoints.len(), 2);
        assert_eq!(details.endpoints[0].identifier, ModuleID::Credentials);
        assert_eq!(details.endpoints[0].role, InterfaceRole::Sender);
        assert_eq!(
            details.endpoints[0].url.as_str(),
            "https://example.com/ocpi/cpo/2.2.1/credentials"
        );
        assert_eq!(details.endpoints[1].identifier, ModuleID::Locations);
        // round-trip
        let back: VersionDetails =
            serde_json::from_str(&serde_json::to_string(&details).unwrap()).unwrap();
        assert_eq!(back, details);
    }

    #[test]
    fn version_details_serde_spec_example_2() {
        // Mirrors the spec: party acting as both CPO and eMSP.
        // Spec ref: version_information_endpoint.asciidoc, GET /versions/{version} example 2.
        let json = r#"{
            "version": "2.2.1",
            "endpoints": [
                {
                    "identifier": "credentials",
                    "role": "SENDER",
                    "url": "https://example.com/ocpi/2.2.1/credentials"
                },
                {
                    "identifier": "locations",
                    "role": "SENDER",
                    "url": "https://example.com/ocpi/cpo/2.2.1/locations"
                },
                {
                    "identifier": "tokens",
                    "role": "RECEIVER",
                    "url": "https://example.com/ocpi/cpo/2.2.1/tokens"
                },
                {
                    "identifier": "tokens",
                    "role": "SENDER",
                    "url": "https://example.com/ocpi/emsp/2.2.1/tokens"
                },
                {
                    "identifier": "locations",
                    "role": "RECEIVER",
                    "url": "https://example.com/ocpi/emsp/2.2.1/locations"
                }
            ]
        }"#;
        let details: VersionDetails = serde_json::from_str(json).unwrap();
        assert_eq!(details.version, VersionNumber::V2_2_1);
        assert_eq!(details.endpoints.len(), 5);
        // credentials only once (no role distinction per spec note)
        assert_eq!(details.endpoints[0].identifier, ModuleID::Credentials);
    }

    #[test]
    fn versions_list_serde_roundtrip() {
        // Wire format for GET /versions response body (the data array).
        let json = r#"[
            {"version": "2.2.1", "url": "https://example.com/ocpi/cpo/2.2.1"},
            {"version": "2.1.1", "url": "https://example.com/ocpi/cpo/2.1.1"}
        ]"#;
        let versions: Vec<Version> = serde_json::from_str(json).unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].version, VersionNumber::V2_2_1);
        assert_eq!(
            versions[0].url.as_str(),
            "https://example.com/ocpi/cpo/2.2.1"
        );
    }
}

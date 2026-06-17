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

use serde::{Deserialize, Serialize};

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
/// Each module has a fixed lowercase ASCII identifier.
///
/// Custom module IDs (`"nltnm-tokens"` style) are spec-allowed but not yet
/// modelled; deserializing one will return an error. A follow-up issue will
/// add an `Other(String)` catch-all.
///
/// Spec: `specs/ocpi/2.2.1/version_information_endpoint.asciidoc` — ModuleID enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModuleID {
    /// Charge Detail Records module.
    Cdrs,
    /// Charging Profiles module (`"chargingprofiles"`).
    #[serde(rename = "chargingprofiles")]
    ChargingProfiles,
    /// Commands module.
    Commands,
    /// Credentials & Registration module (required for all implementations).
    Credentials,
    /// Hub Client Info module (`"hubclientinfo"`).
    #[serde(rename = "hubclientinfo")]
    HubClientInfo,
    /// Locations module.
    Locations,
    /// Sessions module.
    Sessions,
    /// Tariffs module.
    Tariffs,
    /// Tokens module.
    Tokens,
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
        ] {
            assert_eq!(s.parse::<VersionNumber>().unwrap(), v, "parse {s}");
            assert_eq!(v.as_str(), s, "as_str for {s}");
            assert_eq!(v.to_string(), s, "Display for {s}");
        }
    }

    #[test]
    fn version_number_from_str_unknown() {
        assert!("3.0".parse::<VersionNumber>().is_err());
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

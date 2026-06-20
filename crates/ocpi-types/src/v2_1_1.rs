//! OCPI **2.1.1** typed models.
//!
//! Modules: Versions, Credentials, Locations, Sessions, CDRs, Tariffs, Tokens,
//! Commands.
//!
//! Populated incrementally — see milestone **M7** in the roadmap. Shared
//! primitives live in [`crate::common`], [`crate::envelope`], and
//! [`crate::version`].
//!
//! ## Version-negotiation delta from 2.2.1
//!
//! The Sender/Receiver `InterfaceRole` split was introduced in OCPI **2.2**.
//! In 2.1.1 a version-details endpoint entry is therefore just
//! `{ identifier, url }` with **no `role` field**. The shared
//! [`crate::version::Endpoint`] carries a required `role`, so it cannot
//! represent a 2.1.1 endpoint faithfully. Rather than weaken the 2.2.1 type
//! by making `role` optional, this module defines a distinct 2.1.1-shaped
//! [`Endpoint`] and [`VersionDetails`]. [`VersionNumber`] and [`ModuleID`]
//! are version-agnostic and reused as-is.
//!
//! The 2.1.1 module set is **Locations, Sessions, CDRs, Tariffs, Tokens,
//! Commands, Credentials** — there is **no** `HubClientInfo` and **no**
//! `ChargingProfiles` (both arrived in 2.2). The reused [`ModuleID`] enum is a
//! superset; constructing a 2.1.1 [`Endpoint`] with one of the 2.2-only
//! identifiers is possible at the type level but out of spec — see
//! [`Endpoint::is_valid_2_1_1`].

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::common::{CiString36, CiString64, Url};
use crate::version::{ModuleID, VersionNumber};

// ── Endpoint ──────────────────────────────────────────────────────────────────

/// A single module endpoint entry within a 2.1.1 [`VersionDetails`] response.
///
/// Unlike the 2.2.1 [`crate::version::Endpoint`], there is **no `role`** —
/// the Sender/Receiver split did not exist before OCPI 2.2.
///
/// Spec: OCPI 2.1.1 — *Version details endpoint* (`specs/ocpi/2.1.1/OCPI_2.1.1.pdf`,
/// "Versions" chapter, Endpoint class).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    /// Which module this endpoint belongs to.
    pub identifier: ModuleID,
    /// URL to call for this module endpoint.
    pub url: Url,
}

impl Endpoint {
    /// Whether `identifier` is part of the OCPI 2.1.1 module set.
    ///
    /// `HubClientInfo` and `ChargingProfiles` were introduced in 2.2 and are
    /// therefore not valid in a 2.1.1 version-details response, even though
    /// the shared [`ModuleID`] enum can represent them.
    #[must_use]
    pub const fn is_valid_2_1_1(&self) -> bool {
        !matches!(
            self.identifier,
            ModuleID::HubClientInfo | ModuleID::ChargingProfiles
        )
    }
}

// ── VersionDetails ────────────────────────────────────────────────────────────

/// Response body for `GET /versions/{version}` in OCPI 2.1.1.
///
/// Lists all supported module endpoints for the 2.1.1 API. Mirrors the 2.2.1
/// [`crate::version::VersionDetails`] but uses the role-less 2.1.1 [`Endpoint`].
///
/// Spec: OCPI 2.1.1 — *Version details endpoint* (`specs/ocpi/2.1.1/OCPI_2.1.1.pdf`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionDetails {
    /// The OCPI version these endpoints implement (expected `2.1.1`).
    pub version: VersionNumber,
    /// All supported module endpoints for this version.
    pub endpoints: Vec<Endpoint>,
}

impl VersionDetails {
    /// Whether every endpoint uses a module identifier valid in OCPI 2.1.1.
    ///
    /// See [`Endpoint::is_valid_2_1_1`].
    #[must_use]
    pub fn endpoints_valid_2_1_1(&self) -> bool {
        self.endpoints.iter().all(Endpoint::is_valid_2_1_1)
    }
}

// ── TokenType ─────────────────────────────────────────────────────────────────

/// Type of a 2.1.1 [`Token`].
///
/// ## Delta from 2.2.1
///
/// OCPI 2.1.1 defines only `OTHER` and `RFID`. The `AD_HOC_USER` and
/// `APP_USER` variants were introduced in 2.2.1, so they are deliberately
/// absent here — a 2.1.1 peer never emits them.
///
/// Spec: OCPI 2.1.1 — *Tokens* module, `TokenType` enum
/// (`specs/ocpi/2.1.1/OCPI_2.1.1.pdf`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TokenType {
    /// Other token type.
    Other,
    /// RFID token read by the charge point reader.
    Rfid,
}

// ── Token ─────────────────────────────────────────────────────────────────────

/// An OCPI **2.1.1** Token — the credential an eMSP issues to a driver, which a
/// CPO authorizes (locally via whitelist or in real time).
///
/// ## Deltas from the 2.2.1 [`crate::v2_2_1::Token`]
///
/// - Uses **`auth_id`** (the eMSP contract identifier) — *not* `contract_id`.
/// - **No** `country_code` / `party_id` on the object itself (added in 2.2).
/// - **No** `group_id`, `default_profile_type`, or `energy_contract` (all 2.2+).
/// - [`TokenType`] covers only `OTHER` / `RFID`.
///
/// `whitelist` reuses [`crate::v2_2_1::WhitelistType`]: the enum
/// (`ALWAYS` / `ALLOWED` / `ALLOWED_OFFLINE` / `NEVER`) is byte-identical across
/// 2.1.1 and 2.2.1, so it is shared rather than duplicated.
///
/// Spec: OCPI 2.1.1 — *Tokens* module, *Object description*
/// (`specs/ocpi/2.1.1/OCPI_2.1.1.pdf`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    /// Unique ID by which this token (combined with `type`) can be identified.
    pub uid: CiString36,
    /// Type of the token. Wire name is `type` (Rust keyword conflict).
    #[serde(rename = "type")]
    pub token_type: TokenType,
    /// eMSP contract identifier (the eMA ID within the eMSP's platform).
    ///
    /// Renamed to `contract_id` in 2.2.1; in 2.1.1 the wire field is `auth_id`.
    pub auth_id: CiString36,
    /// Visual number printed on the token (e.g. RFID card), max 64 chars.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub visual_number: Option<CiString64>,
    /// Issuing company name (max 64 chars).
    pub issuer: CiString64,
    /// Whether this token is currently valid.
    pub valid: bool,
    /// Whitelist policy — when the CPO may authorize without contacting the eMSP.
    pub whitelist: crate::v2_2_1::WhitelistType,
    /// ISO 639-1 preferred language of the token owner (max 2 chars).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub language: Option<String>,
    /// Timestamp when this token was last updated or created.
    pub last_updated: DateTime<Utc>,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v2_2_1::WhitelistType;

    #[test]
    fn token_type_serde_roundtrip() {
        for (ty, wire) in [
            (TokenType::Other, "\"OTHER\""),
            (TokenType::Rfid, "\"RFID\""),
        ] {
            let json = serde_json::to_string(&ty).unwrap();
            assert_eq!(json, wire);
            let back: TokenType = serde_json::from_str(&json).unwrap();
            assert_eq!(back, ty);
        }
    }

    #[test]
    fn token_type_rejects_2_2_1_only_variants() {
        // AD_HOC_USER / APP_USER are 2.2.1 additions and must not parse as 2.1.1.
        assert!(serde_json::from_str::<TokenType>("\"AD_HOC_USER\"").is_err());
        assert!(serde_json::from_str::<TokenType>("\"APP_USER\"").is_err());
    }

    #[test]
    fn token_serde_spec_example() {
        // Ported from the OCPI 2.1.1 spec "Tokens" object example: an RFID
        // token. Note: auth_id (not contract_id), and no country_code/party_id.
        // Spec ref: specs/ocpi/2.1.1/OCPI_2.1.1.pdf, "Tokens" module.
        let json = r#"{
            "uid": "12345678905880",
            "type": "RFID",
            "auth_id": "DE8ACC12E46L89",
            "visual_number": "DF000-2001-8999",
            "issuer": "TheNewMotion",
            "valid": true,
            "whitelist": "ALLOWED",
            "language": "it",
            "last_updated": "2018-12-10T17:25:10Z"
        }"#;
        let token: Token = serde_json::from_str(json).unwrap();
        assert_eq!(token.uid.as_str(), "12345678905880");
        assert_eq!(token.token_type, TokenType::Rfid);
        assert_eq!(token.auth_id.as_str(), "DE8ACC12E46L89");
        assert_eq!(token.whitelist, WhitelistType::Allowed);
        assert!(token.valid);

        let back: Token = serde_json::from_str(&serde_json::to_string(&token).unwrap()).unwrap();
        assert_eq!(back, token);
    }

    #[test]
    fn token_wire_form_has_auth_id_not_2_2_1_fields() {
        let token = Token {
            uid: CiString36::try_from("12345678905880").unwrap(),
            token_type: TokenType::Rfid,
            auth_id: CiString36::try_from("DE8ACC12E46L89").unwrap(),
            visual_number: None,
            issuer: CiString64::try_from("TheNewMotion").unwrap(),
            valid: true,
            whitelist: WhitelistType::Always,
            language: None,
            last_updated: "2018-12-10T17:25:10Z".parse().unwrap(),
        };
        let json = serde_json::to_string(&token).unwrap();
        assert!(json.contains("\"auth_id\":\"DE8ACC12E46L89\""));
        // None of the 2.2+ fields may appear on the 2.1.1 wire form.
        for absent in [
            "contract_id",
            "country_code",
            "party_id",
            "group_id",
            "default_profile_type",
            "energy_contract",
        ] {
            assert!(
                !json.contains(absent),
                "2.1.1 Token must not carry `{absent}`: {json}"
            );
        }
    }

    #[test]
    fn endpoint_has_no_role_on_the_wire() {
        let ep = Endpoint {
            identifier: ModuleID::Locations,
            url: Url::try_from("https://example.com/ocpi/cpo/2.1.1/locations/").unwrap(),
        };
        let json = serde_json::to_string(&ep).unwrap();
        assert!(
            !json.contains("role"),
            "2.1.1 endpoint must omit role: {json}"
        );
        assert!(json.contains("identifier"));
        assert!(json.contains("url"));
        let back: Endpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ep);
    }

    #[test]
    fn endpoint_deserializes_without_role() {
        // A faithful 2.1.1 payload carries no role key at all.
        let json = r#"{
            "identifier": "tokens",
            "url": "https://example.com/ocpi/cpo/2.1.1/tokens/"
        }"#;
        let ep: Endpoint = serde_json::from_str(json).unwrap();
        assert_eq!(ep.identifier, ModuleID::Tokens);
        assert_eq!(
            ep.url.as_str(),
            "https://example.com/ocpi/cpo/2.1.1/tokens/"
        );
    }

    #[test]
    fn version_details_serde_spec_example() {
        // Ported from the OCPI 2.1.1 spec "Version details endpoint" example:
        // a CPO advertising its 2.1.1 module endpoints. Note: no `role` field.
        // Spec ref: specs/ocpi/2.1.1/OCPI_2.1.1.pdf, "Versions" chapter.
        let json = r#"{
            "version": "2.1.1",
            "endpoints": [
                {
                    "identifier": "credentials",
                    "url": "https://example.com/ocpi/cpo/2.1.1/credentials/"
                },
                {
                    "identifier": "locations",
                    "url": "https://example.com/ocpi/cpo/2.1.1/locations/"
                },
                {
                    "identifier": "tariffs",
                    "url": "https://example.com/ocpi/cpo/2.1.1/tariffs/"
                },
                {
                    "identifier": "tokens",
                    "url": "https://example.com/ocpi/cpo/2.1.1/tokens/"
                }
            ]
        }"#;
        let details: VersionDetails = serde_json::from_str(json).unwrap();
        assert_eq!(details.version, VersionNumber::V2_1_1);
        assert_eq!(details.endpoints.len(), 4);
        assert_eq!(details.endpoints[0].identifier, ModuleID::Credentials);
        assert_eq!(details.endpoints[1].identifier, ModuleID::Locations);
        assert_eq!(
            details.endpoints[1].url.as_str(),
            "https://example.com/ocpi/cpo/2.1.1/locations/"
        );
        assert!(details.endpoints_valid_2_1_1());

        // Round-trip and prove no `role` leaks into the serialized form.
        let out = serde_json::to_string(&details).unwrap();
        assert!(
            !out.contains("role"),
            "serialized 2.1.1 details must omit role"
        );
        let back: VersionDetails = serde_json::from_str(&out).unwrap();
        assert_eq!(back, details);
    }

    #[test]
    fn detects_out_of_spec_2_2_modules() {
        // HubClientInfo / ChargingProfiles are 2.2+ and not valid in 2.1.1.
        let hub = Endpoint {
            identifier: ModuleID::HubClientInfo,
            url: Url::try_from("https://example.com/ocpi/cpo/2.1.1/hubclientinfo/").unwrap(),
        };
        assert!(!hub.is_valid_2_1_1());

        let profiles = Endpoint {
            identifier: ModuleID::ChargingProfiles,
            url: Url::try_from("https://example.com/ocpi/cpo/2.1.1/chargingprofiles/").unwrap(),
        };
        assert!(!profiles.is_valid_2_1_1());

        let details = VersionDetails {
            version: VersionNumber::V2_1_1,
            endpoints: vec![
                Endpoint {
                    identifier: ModuleID::Sessions,
                    url: Url::try_from("https://example.com/ocpi/cpo/2.1.1/sessions/").unwrap(),
                },
                hub,
            ],
        };
        assert!(!details.endpoints_valid_2_1_1());
    }
}

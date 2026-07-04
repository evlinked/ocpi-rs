//! OCPI 2.1.1 — Tokens module types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::common::{CiString36, CiString64, DisplayText};

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

// ── LocationReferences ────────────────────────────────────────────────────────

/// References a location and, optionally, specific EVSEs/connectors within it.
///
/// Sent as the body of a real-time authorize request and echoed (filtered to the
/// allowed subset) in the [`AuthorizationInfo`] response.
///
/// ## Delta from 2.2.1
///
/// OCPI **2.1.1** carries a third field, **`connector_ids`**, which was removed
/// in 2.2.1 (where a `LocationReferences` narrows to `location_id` + `evse_uids`
/// only). A 2.1.1 peer may therefore scope authorization down to individual
/// connectors, so the field is modelled here.
///
/// Spec: OCPI 2.1.1 — *Tokens* module, *LocationReferences* class
/// (`specs/ocpi/2.1.1/OCPI_2.1.1.pdf`, §12.4.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocationReferences {
    /// Unique identifier for the location (`Location.id`).
    pub location_id: CiString36,
    /// Unique identifiers of the EVSEs within the location (`Evse.uid`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evse_uids: Vec<CiString36>,
    /// Identifiers of the connectors within the given EVSEs (`Connector.id`).
    ///
    /// Removed in 2.2.1; kept here for 2.1.1 fidelity.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connector_ids: Vec<CiString36>,
}

// ── AuthorizationInfo ─────────────────────────────────────────────────────────

/// Response to a real-time authorization request
/// (`POST /tokens/{token_uid}/authorize`).
///
/// ## Deltas from the 2.2.1 [`crate::v2_2_1::AuthorizationInfo`]
///
/// - **No `token`** field — 2.1.1 does not echo the full [`Token`] in the
///   response (added in 2.2).
/// - **No `authorization_reference`** — that identifier arrived in 2.2.
/// - `location` is the 2.1.1 [`LocationReferences`] (carries `connector_ids`).
///
/// The `allowed` enum ([`crate::v2_2_1::AllowedType`]:
/// `ALLOWED` / `BLOCKED` / `EXPIRED` / `NO_CREDIT` / `NOT_ALLOWED`) is
/// byte-identical across 2.1.1 and 2.2.1, so it is shared rather than duplicated.
///
/// Spec: OCPI 2.1.1 — *Tokens* module, *AuthorizationInfo* object
/// (`specs/ocpi/2.1.1/OCPI_2.1.1.pdf`, §12.3.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizationInfo {
    /// Status of the token, and whether charging is allowed at the optionally
    /// given location.
    pub allowed: crate::v2_2_1::AllowedType,
    /// Optional reference to the location supplied in the request, filtered to
    /// the EVSEs/connectors the driver is allowed to charge at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<LocationReferences>,
    /// Optional display text with additional information for the EV driver.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub info: Option<DisplayText>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v2_2_1::{AllowedType, WhitelistType};

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
    fn location_references_keeps_connector_ids() {
        // 2.1.1 LocationReferences carries connector_ids (dropped in 2.2.1).
        let json = r#"{
            "location_id": "LOC1",
            "evse_uids": ["3256"],
            "connector_ids": ["1", "2"]
        }"#;
        let lr: LocationReferences = serde_json::from_str(json).unwrap();
        assert_eq!(lr.location_id.as_str(), "LOC1");
        assert_eq!(lr.evse_uids.len(), 1);
        assert_eq!(lr.connector_ids.len(), 2);

        // Empty vecs are omitted on the wire.
        let bare = LocationReferences {
            location_id: CiString36::try_from("LOC1").unwrap(),
            evse_uids: vec![],
            connector_ids: vec![],
        };
        let wire = serde_json::to_string(&bare).unwrap();
        assert_eq!(wire, r#"{"location_id":"LOC1"}"#);
    }

    #[test]
    fn authorization_info_omits_2_2_1_fields() {
        // Ported from the 2.1.1 spec AuthorizationInfo example: allowed + no
        // token / authorization_reference (both 2.2 additions).
        let json = r#"{ "allowed": "ALLOWED" }"#;
        let info: AuthorizationInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.allowed, AllowedType::Allowed);
        assert!(info.location.is_none());
        assert!(info.info.is_none());

        let wire = serde_json::to_string(&info).unwrap();
        assert_eq!(wire, r#"{"allowed":"ALLOWED"}"#);
        for absent in ["token", "authorization_reference"] {
            assert!(
                !wire.contains(absent),
                "2.1.1 AuthorizationInfo must not carry `{absent}`: {wire}"
            );
        }
    }
}

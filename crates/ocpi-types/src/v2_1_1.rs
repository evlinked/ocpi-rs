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

use crate::common::{BusinessDetails, CiString2, CiString3, CiString36, CiString64, Url};
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

// ── Credentials ───────────────────────────────────────────────────────────────

/// The OCPI **2.1.1** credentials object exchanged during registration (POST),
/// updates (PUT), and returned on GET.
///
/// ## Delta from 2.2.1
///
/// The 2.1.1 object is **flat**: a party advertises exactly one
/// `party_id` / `country_code` / `business_details` at the top level. There is
/// **no `roles: [...]` array** — the multi-role array (and the Hub role) were
/// introduced in OCPI 2.2. Modelling 2.1.1 with the 2.2.1
/// [`crate::v2_2_1::Credentials`] (which requires `roles`) would not round-trip
/// against a real 2.1.1 peer, so this is a distinct type.
///
/// The registration flow is the classic *Token A → B → C* exchange via
/// `GET`/`POST`/`PUT`/`DELETE /credentials`, but with **no `OCPI-to/from-*`
/// routing headers** (those are 2.2+ only).
///
/// Spec: OCPI 2.1.1 — *Credentials* module / *Registration* use-case
/// (`specs/ocpi/2.1.1/OCPI_2.1.1.pdf`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credentials {
    /// Bearer token the remote party must use in subsequent requests.
    ///
    /// OCPI 2.1.1 spec: case-sensitive ASCII, max 64 characters. Not validated
    /// here — callers are responsible.
    pub token: String,
    /// URL of this party's `/versions` endpoint.
    pub url: Url,
    /// Human-readable business details for this party.
    pub business_details: BusinessDetails,
    /// eMI3 party identifier (3-char, e.g. `"EXA"`).
    pub party_id: CiString3,
    /// ISO 3166-1 alpha-2 country code (e.g. `"NL"`).
    pub country_code: CiString2,
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// The type of remote command an eMSP (Sender) sends to a CPO (Receiver).
///
/// Used as a URL path segment in the Commands receiver interface:
/// `POST {commands_endpoint_url}{command}`.
///
/// ## Delta from 2.2.1
///
/// 2.1.1 defines **four** commands — `RESERVE_NOW`, `START_SESSION`,
/// `STOP_SESSION`, `UNLOCK_CONNECTOR`. The `CANCEL_RESERVATION` variant (and its
/// `CancelReservation` body) were introduced in OCPI 2.2, so they are absent
/// here — a 2.1.1 peer never emits them.
///
/// Spec: OCPI 2.1.1 — *Commands* module, `CommandType` enum (§13.4.2,
/// `specs/ocpi/2.1.1/OCPI_2.1.1.pdf`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CommandType {
    /// Request the Charge Point to reserve a (specific) EVSE for a Token, now.
    ReserveNow,
    /// Request the Charge Point to start a transaction on the given EVSE/Connector.
    StartSession,
    /// Request the Charge Point to stop an ongoing session.
    StopSession,
    /// Request the Charge Point to unlock a connector (help-desk operators only).
    UnlockConnector,
}

/// Result of a command request, as reported back to the eMSP.
///
/// ## Delta from 2.2.1
///
/// In 2.1.1 this **single** enum is used in two places: the synchronous body of
/// the CPO's response to the eMSP's `POST {command}`, **and** the asynchronous
/// `CommandResponse` the CPO later POSTs to the Sender's `response_url` once the
/// Charge Point replies (§13.2.1.2.1, §13.2.2.1). OCPI 2.2 split this into a
/// separate `CommandResponseType` (sync ack) and `CommandResultType` (async
/// result) and moved `TIMEOUT` out into a numeric `timeout` field — so this
/// 2.1.1 enum is a distinct, faithful type rather than a reuse of
/// [`crate::v2_2_1::CommandResponseType`].
///
/// Spec: OCPI 2.1.1 — *Commands* module, `CommandResponseType` enum (§13.4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CommandResponseType {
    /// The requested command is not supported by this CPO, Charge Point, or EVSE.
    NotSupported,
    /// Command request rejected by the CPO or Charge Point.
    Rejected,
    /// Command request accepted by the CPO or Charge Point.
    Accepted,
    /// No response received from the Charge Point within a reasonable time.
    Timeout,
    /// The session referenced in the command is not known by this CPO.
    UnknownSession,
}

/// Response to a command request.
///
/// ## Delta from 2.2.1
///
/// The 2.1.1 object carries **only** `result`. The numeric `timeout` field and
/// the `message: DisplayText[]` list were both introduced in OCPI 2.2, so they
/// are absent here. The same object shape is used for the CPO's synchronous ack
/// and for the asynchronous Charge-Point response POSTed to the `response_url`.
///
/// Spec: OCPI 2.1.1 — *Commands* module, `CommandResponse` object (§13.3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandResponse {
    /// Result of the command request.
    pub result: CommandResponseType,
}

/// Request the CPO to reserve an EVSE at a Location for a Token.
///
/// A reservation can be replaced/updated by sending a `RESERVE_NOW` with the
/// same Location and the same `reservation_id`.
///
/// ## Delta from 2.2.1
///
/// - `reservation_id` is an **integer** in 2.1.1 (it became a `CiString36` in
///   2.2). Models as [`i32`].
/// - `location_id` / `evse_uid` are plain `string` (max 39), not `CiString`.
/// - No `authorization_reference` (2.2+).
///
/// Spec: OCPI 2.1.1 — *Commands* module, `ReserveNow` object (§13.3.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReserveNow {
    /// URL the `CommandResponse` POST should be sent to (Sender-chosen, ideally
    /// unique per request to disambiguate simultaneous commands).
    pub response_url: Url,
    /// Token the Charge Point must use to reserve (pre-authorized by the eMSP).
    pub token: Token,
    /// Date/Time when this reservation ends.
    pub expiry_date: DateTime<Utc>,
    /// Reservation ID, unique for this reservation. A matching ID replaces the
    /// existing reservation. Integer in 2.1.1 (`CiString36` from 2.2 onward).
    pub reservation_id: i32,
    /// `Location.id` of the Location (at the CPO) to reserve an EVSE on.
    pub location_id: String,
    /// Optional `EVSE.uid` to reserve a specific EVSE; if absent the CPO keeps
    /// one free.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evse_uid: Option<String>,
}

/// Request the CPO to start a charging session at a Location/EVSE.
///
/// The Token is pre-authorized by the eMSP.
///
/// ## Delta from 2.2.1
///
/// 2.1.1 carries no `connector_id` and no `authorization_reference` (both 2.2+);
/// `location_id` / `evse_uid` are plain `string`.
///
/// Spec: OCPI 2.1.1 — *Commands* module, `StartSession` object (§13.3.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartSession {
    /// URL the `CommandResponse` POST should be sent to.
    pub response_url: Url,
    /// Token the Charge Point must use to start a new session.
    pub token: Token,
    /// `Location.id` of the Location (at the CPO) on which to start a session.
    pub location_id: String,
    /// Optional `EVSE.uid`; if absent the Charge Point may choose the EVSE.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evse_uid: Option<String>,
}

/// Request the CPO to stop an ongoing charging session.
///
/// Spec: OCPI 2.1.1 — *Commands* module, `StopSession` object (§13.3.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopSession {
    /// URL the `CommandResponse` POST should be sent to.
    pub response_url: Url,
    /// `Session.id` of the session that is requested to be stopped.
    pub session_id: String,
}

/// Request the CPO to unlock a specific connector (help-desk/operator use only).
///
/// **Warning:** this command must never be sent directly by an EV Driver — use
/// only when a connector fails to unlock after a transaction ends.
///
/// Spec: OCPI 2.1.1 — *Commands* module, `UnlockConnector` object (§13.3.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnlockConnector {
    /// URL the `CommandResponse` POST should be sent to.
    pub response_url: Url,
    /// `Location.id` of the Location.
    pub location_id: String,
    /// `EVSE.uid` of the EVSE.
    pub evse_uid: String,
    /// `Connector.id` to unlock.
    pub connector_id: String,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::Image;
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
    fn credentials_serde_spec_example() {
        // Ported from the OCPI 2.1.1 spec "Credentials object" example — note
        // the flat shape: party_id/country_code/business_details at top level,
        // and NO `roles` array.
        // Spec ref: specs/ocpi/2.1.1/OCPI_2.1.1.pdf, "Credentials" module.
        let json = r#"{
            "token": "ebf3b399-779f-4497-9b9d-ac6ad3cc44d2",
            "url": "https://example.com/ocpi/cpo/versions",
            "business_details": {
                "name": "Example Operator",
                "website": "https://example.com"
            },
            "party_id": "EXA",
            "country_code": "NL"
        }"#;
        let creds: Credentials = serde_json::from_str(json).unwrap();
        assert_eq!(creds.token, "ebf3b399-779f-4497-9b9d-ac6ad3cc44d2");
        assert_eq!(creds.party_id.as_str(), "EXA");
        assert_eq!(creds.country_code.as_str(), "NL");
        assert_eq!(creds.business_details.name, "Example Operator");

        let back: Credentials =
            serde_json::from_str(&serde_json::to_string(&creds).unwrap()).unwrap();
        assert_eq!(back, creds);
    }

    #[test]
    fn credentials_wire_form_is_flat_no_roles_array() {
        let creds = Credentials {
            token: "TOKEN_B".into(),
            url: Url::try_from("https://emsp.example.com/ocpi/versions").unwrap(),
            business_details: BusinessDetails {
                name: "Example Provider".into(),
                website: None,
                logo: None,
            },
            party_id: CiString3::try_from("MSP").unwrap(),
            country_code: CiString2::try_from("DE").unwrap(),
        };
        let json = serde_json::to_string(&creds).unwrap();
        // The 2.1.1 wire form must NOT carry a 2.2+ `roles` array, and must
        // carry party_id/country_code at the top level.
        assert!(
            !json.contains("\"roles\""),
            "2.1.1 credentials must be flat (no roles array): {json}"
        );
        assert!(json.contains("\"party_id\":\"MSP\""));
        assert!(json.contains("\"country_code\":\"DE\""));
    }

    #[test]
    fn business_details_logo_roundtrips() {
        let creds = Credentials {
            token: "TOKEN_C".into(),
            url: Url::try_from("https://cpo.example.com/ocpi/versions").unwrap(),
            business_details: BusinessDetails {
                name: "Example Operator".into(),
                website: Some("https://example.com".into()),
                logo: Some(Image {
                    url: "https://example.com/logo.png".into(),
                    thumbnail: Some("https://example.com/logo-thumb.png".into()),
                    category: "OPERATOR".into(),
                    image_type: "png".into(),
                    width: Some(512),
                    height: Some(512),
                }),
            },
            party_id: CiString3::try_from("EXA").unwrap(),
            country_code: CiString2::try_from("NL").unwrap(),
        };
        let back: Credentials =
            serde_json::from_str(&serde_json::to_string(&creds).unwrap()).unwrap();
        assert_eq!(back, creds);
        assert_eq!(back.business_details.logo.unwrap().category, "OPERATOR");
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

    // ── Commands ────────────────────────────────────────────────────────────

    #[test]
    fn command_type_serde_roundtrip() {
        for (ty, wire) in [
            (CommandType::ReserveNow, "\"RESERVE_NOW\""),
            (CommandType::StartSession, "\"START_SESSION\""),
            (CommandType::StopSession, "\"STOP_SESSION\""),
            (CommandType::UnlockConnector, "\"UNLOCK_CONNECTOR\""),
        ] {
            assert_eq!(serde_json::to_string(&ty).unwrap(), wire);
            let back: CommandType = serde_json::from_str(wire).unwrap();
            assert_eq!(back, ty);
        }
    }

    #[test]
    fn command_type_rejects_2_2_only_cancel_reservation() {
        // CANCEL_RESERVATION is a 2.2 addition and must not parse as 2.1.1.
        assert!(serde_json::from_str::<CommandType>("\"CANCEL_RESERVATION\"").is_err());
    }

    #[test]
    fn command_response_type_serde_roundtrip() {
        // 2.1.1 has all five values, incl. TIMEOUT and UNKNOWN_SESSION (§13.4.1).
        for (ty, wire) in [
            (CommandResponseType::NotSupported, "\"NOT_SUPPORTED\""),
            (CommandResponseType::Rejected, "\"REJECTED\""),
            (CommandResponseType::Accepted, "\"ACCEPTED\""),
            (CommandResponseType::Timeout, "\"TIMEOUT\""),
            (CommandResponseType::UnknownSession, "\"UNKNOWN_SESSION\""),
        ] {
            assert_eq!(serde_json::to_string(&ty).unwrap(), wire);
            let back: CommandResponseType = serde_json::from_str(wire).unwrap();
            assert_eq!(back, ty);
        }
    }

    #[test]
    fn command_response_is_just_result_no_2_2_fields() {
        // 2.1.1 CommandResponse carries ONLY `result` — no `timeout`, no
        // `message` list (both 2.2 additions). Spec ref: §13.3.1.
        let resp = CommandResponse {
            result: CommandResponseType::Accepted,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, r#"{"result":"ACCEPTED"}"#);
        for absent in ["timeout", "message"] {
            assert!(
                !json.contains(absent),
                "2.1.1 CommandResponse must not carry `{absent}`: {json}"
            );
        }
        let back: CommandResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back, resp);
    }

    #[test]
    fn reserve_now_serde_spec_example() {
        // Constructed from the OCPI 2.1.1 ReserveNow object table (§13.3.2);
        // the embedded token mirrors the spec's Tokens example. Note the
        // integer reservation_id and the plain-string location/evse ids.
        let json = r#"{
            "response_url": "https://example.com/ocpi/emsp/2.0/commands/RESERVE_NOW/1234",
            "token": {
                "uid": "12345678905880",
                "type": "RFID",
                "auth_id": "DE8ACC12E46L89",
                "issuer": "TheNewMotion",
                "valid": true,
                "whitelist": "ALLOWED",
                "last_updated": "2018-12-10T17:25:10Z"
            },
            "expiry_date": "2018-12-10T18:00:00Z",
            "reservation_id": 1234,
            "location_id": "LOC1",
            "evse_uid": "3256"
        }"#;
        let rn: ReserveNow = serde_json::from_str(json).unwrap();
        assert_eq!(rn.reservation_id, 1234);
        assert_eq!(rn.location_id, "LOC1");
        assert_eq!(rn.evse_uid.as_deref(), Some("3256"));
        assert_eq!(rn.token.auth_id.as_str(), "DE8ACC12E46L89");

        let back: ReserveNow = serde_json::from_str(&serde_json::to_string(&rn).unwrap()).unwrap();
        assert_eq!(back, rn);
        // reservation_id must serialize as a JSON number, not a string.
        assert!(serde_json::to_string(&rn)
            .unwrap()
            .contains("\"reservation_id\":1234"));
    }

    #[test]
    fn start_session_omits_2_2_fields_and_optional_evse() {
        // StartSession with no evse_uid (Charge Point chooses the EVSE).
        let json = r#"{
            "response_url": "https://example.com/ocpi/emsp/2.0/commands/START_SESSION/4567",
            "token": {
                "uid": "12345678905880",
                "type": "RFID",
                "auth_id": "DE8ACC12E46L89",
                "issuer": "TheNewMotion",
                "valid": true,
                "whitelist": "ALWAYS",
                "last_updated": "2018-12-10T17:25:10Z"
            },
            "location_id": "LOC1"
        }"#;
        let ss: StartSession = serde_json::from_str(json).unwrap();
        assert_eq!(ss.location_id, "LOC1");
        assert!(ss.evse_uid.is_none());

        let out = serde_json::to_string(&ss).unwrap();
        // Optional evse_uid omitted when None; no 2.2-only fields present.
        for absent in ["evse_uid", "connector_id", "authorization_reference"] {
            assert!(
                !out.contains(absent),
                "2.1.1 StartSession (no evse) must not carry `{absent}`: {out}"
            );
        }
        let back: StartSession = serde_json::from_str(&out).unwrap();
        assert_eq!(back, ss);
    }

    #[test]
    fn stop_and_unlock_serde_roundtrip() {
        let stop = StopSession {
            response_url: Url::try_from(
                "https://example.com/ocpi/emsp/2.0/commands/STOP_SESSION/8",
            )
            .unwrap(),
            session_id: "1234".to_owned(),
        };
        let back: StopSession =
            serde_json::from_str(&serde_json::to_string(&stop).unwrap()).unwrap();
        assert_eq!(back, stop);

        let unlock = UnlockConnector {
            response_url: Url::try_from(
                "https://example.com/ocpi/emsp/2.0/commands/UNLOCK_CONNECTOR/2",
            )
            .unwrap(),
            location_id: "LOC1".to_owned(),
            evse_uid: "3256".to_owned(),
            connector_id: "1".to_owned(),
        };
        let json = serde_json::to_string(&unlock).unwrap();
        assert!(json.contains("\"connector_id\":\"1\""));
        let back: UnlockConnector = serde_json::from_str(&json).unwrap();
        assert_eq!(back, unlock);
    }
}

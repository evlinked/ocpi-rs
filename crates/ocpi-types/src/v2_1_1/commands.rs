//! OCPI 2.1.1 — Commands module types.
//!
//! The Commands module lets an eMSP (Sender) ask a CPO (Receiver) to act on a
//! Charge Point remotely: reserve an EVSE, start or stop a session, or unlock a
//! connector. The CPO answers synchronously with a [`CommandResponse`] and then
//! asynchronously POSTs a second [`CommandResponse`] to the Sender-supplied
//! `response_url` once the Charge Point replies.
//!
//! ## Deltas from the 2.2.1 Commands types
//!
//! The 2.1.1 wire shape predates several 2.2 additions, so these types are
//! version-pinned rather than reused from [`crate::v2_2_1`]:
//!
//! - **No `CANCEL_RESERVATION`** command (and no `CancelReservation` body) —
//!   both arrived in OCPI 2.2. A 2.1.1 peer never emits them.
//! - **One [`CommandResponseType`] enum** serves both the synchronous ack and
//!   the asynchronous result. 2.2 split these into `CommandResponseType`
//!   (sync) + `CommandResultType` (async) and moved `TIMEOUT` into a numeric
//!   field.
//! - **[`CommandResponse`] carries only `result`** — the 2.2 `timeout` and
//!   `message: DisplayText[]` fields are absent.
//! - **`ReserveNow.reservation_id` is an integer** (it became a `CiString36` in
//!   2.2); `location_id` / `evse_uid` are plain strings, and there is no
//!   `authorization_reference` / `connector_id` (all 2.2+).
//!
//! Spec: OCPI 2.1.1 — *Commands* module (`specs/ocpi/2.1.1/OCPI_2.1.1.pdf`,
//! chapter 13).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::common::Url;

use super::Token;

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

#[cfg(test)]
mod tests {
    use super::*;

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

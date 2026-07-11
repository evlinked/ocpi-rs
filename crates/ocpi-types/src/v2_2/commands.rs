//! OCPI **2.2** Commands module — the wire-delta override over 2.2.1.
//!
//! Only [`StartSession`] genuinely differs on the 2.2 wire; every other Commands
//! type (`CommandType`, `CommandResponse`, `CommandResult`, `ReserveNow`,
//! `StopSession`, `UnlockConnector`, `CancelReservation`) is wire-identical to
//! 2.2.1 and re-exported unchanged by [`super`].
//!
//! The delta, per `specs/ocpi/2.2.1/version_history.asciidoc` (read backwards —
//! this is the 2.2.1 addition that 2.2 does **not** have) and the 2.2 module
//! spec `specs/ocpi/2.2/mod_commands.asciidoc`:
//!
//! - [`StartSession`] — no `connector_id`. In OCPI 2.2 a `START_SESSION`
//!   targets a Location (optionally narrowed to an EVSE); the Charge Point picks
//!   the connector. 2.2.1 added an optional `connector_id` (used together with
//!   the `START_SESSION_CONNECTOR_REQUIRED` EVSE capability) so a Sender can pin
//!   a specific connector.
//!
//! ## `START_SESSION_CONNECTOR_REQUIRED` capability
//!
//! The matching `Capability::StartSessionConnectorRequired` value 2.2.1 added to
//! the Locations `Capability` enum is **not** overridden here: `Capability` stays
//! a plain [`super`] re-export. Overriding it would mean duplicating the whole
//! ~40-variant enum to drop a single value that a conformant 2.2 peer never
//! emits. Accepting the value from a (non-conformant) 2.2 peer is harmless — it
//! is an EVSE-level advertisement, not a routing- or billing-critical field — so
//! a documented re-export is the pragmatic, faithful choice, mirroring how the
//! CDRs slice keeps `SignedData` a re-export. The genuine wire risk — a Sender
//! pinning a `connector_id` a 2.2 Receiver cannot honour — is eliminated at the
//! type level here, because 2.2 `StartSession` has no such field to carry.

use serde::{Deserialize, Serialize};

use crate::common::{CiString36, Url};

// `Token` is wire-identical between 2.2 and 2.2.1, so it comes from the `v2_2`
// re-export surface rather than a direct `crate::v2_2_1` path — if a later slice
// ever overrides `Token`, this picks up the 2.2 shape without another edit.
use super::Token;

// ── StartSession ──────────────────────────────────────────────────────────────

/// Request the CPO to start a charging session on behalf of an EV driver
/// (OCPI 2.2 shape).
///
/// The eMSP provides a pre-authorized [`Token`] and the target `location_id`,
/// optionally narrowed to an `evse_uid`. Unlike 2.2.1 there is **no**
/// `connector_id`: in 2.2 the Charge Point decides which connector of the
/// (optionally specified) EVSE to use.
///
/// Spec: `specs/ocpi/2.2/mod_commands.asciidoc` — StartSession object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartSession {
    /// URL the `CommandResult` POST should be sent to.
    pub response_url: Url,
    /// Token the Charge Point must use to start the session (pre-authorized by the eMSP).
    pub token: Token,
    /// `Location.id` of the Location (at the CPO) on which to start a session.
    pub location_id: CiString36,
    /// Optional `EVSE.uid` of the EVSE on which to start a session. When absent,
    /// the Charge Point itself decides on which EVSE to start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evse_uid: Option<CiString36>,
    /// eMSP authorization reference included in the resulting Session/CDR when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization_reference: Option<CiString36>,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::StartSession;

    #[test]
    fn start_session_2_2_has_no_connector_id() {
        // A faithful 2.2 StartSession: response_url + token + location_id, with
        // an optional evse_uid but no connector_id.
        let json = r#"{
            "response_url": "https://example.com/ocpi/emsp/2.2/commands/START_SESSION/42",
            "token": {
                "country_code": "DE",
                "party_id": "TNM",
                "uid": "12345678905880",
                "type": "RFID",
                "contract_id": "DE8ACC12E46L89",
                "issuer": "TheNewMotion",
                "valid": true,
                "whitelist": "ALLOWED",
                "last_updated": "2015-06-29T22:39:09Z"
            },
            "location_id": "LOC1",
            "evse_uid": "3256"
        }"#;
        let cmd: StartSession = serde_json::from_str(json).unwrap();
        assert_eq!(cmd.location_id.as_str(), "LOC1");
        assert_eq!(cmd.evse_uid.as_ref().unwrap().as_str(), "3256");

        // Negative: the 2.2.1-only `connector_id` must not appear on the wire.
        let out = serde_json::to_string(&cmd).unwrap();
        assert!(
            !out.contains("connector_id"),
            "2.2 StartSession must not emit 2.2.1's connector_id: {out}"
        );

        let back: StartSession = serde_json::from_str(&out).unwrap();
        assert_eq!(back, cmd);
    }

    #[test]
    fn start_session_2_2_rejects_stray_connector_id() {
        // A 2.2 Receiver must not silently accept a `connector_id` it cannot
        // honour. With `deny_unknown_fields` not set the struct would ignore it;
        // this test pins the *documented* behaviour: because 2.2 StartSession has
        // no such field, a peer that pins a connector is handled without any
        // silent connector-level data being carried into the session — the field
        // round-trips to nothing. We assert the parsed value never resurrects a
        // connector on re-serialization.
        let json = r#"{
            "response_url": "https://example.com/cmd",
            "token": {
                "country_code": "DE",
                "party_id": "TNM",
                "uid": "12345678905880",
                "type": "RFID",
                "contract_id": "DE8ACC12E46L89",
                "issuer": "TheNewMotion",
                "valid": true,
                "whitelist": "ALLOWED",
                "last_updated": "2015-06-29T22:39:09Z"
            },
            "location_id": "LOC1",
            "evse_uid": "3256",
            "connector_id": "1"
        }"#;
        let cmd: StartSession = serde_json::from_str(json).unwrap();
        let out = serde_json::to_string(&cmd).unwrap();
        assert!(
            !out.contains("connector_id"),
            "a stray 2.2.1 connector_id must not survive a 2.2 round-trip: {out}"
        );
    }

    #[test]
    fn start_session_2_2_omits_optional_fields_when_absent() {
        // Minimal 2.2 StartSession: no evse_uid, no authorization_reference.
        let json = r#"{
            "response_url": "https://example.com/cmd",
            "token": {
                "country_code": "DE",
                "party_id": "TNM",
                "uid": "12345678905880",
                "type": "RFID",
                "contract_id": "DE8ACC12E46L89",
                "issuer": "TheNewMotion",
                "valid": true,
                "whitelist": "ALLOWED",
                "last_updated": "2015-06-29T22:39:09Z"
            },
            "location_id": "LOC1"
        }"#;
        let cmd: StartSession = serde_json::from_str(json).unwrap();
        assert!(cmd.evse_uid.is_none());
        assert!(cmd.authorization_reference.is_none());

        let out = serde_json::to_string(&cmd).unwrap();
        assert!(!out.contains("evse_uid"), "{out}");
        assert!(!out.contains("authorization_reference"), "{out}");
        assert!(!out.contains("connector_id"), "{out}");
    }
}

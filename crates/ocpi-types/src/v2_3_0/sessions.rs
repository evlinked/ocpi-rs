//! OCPI **2.3.0** Sessions — the North-American tax delta over 2.2.1 (slice 3 of #188).
//!
//! A [`Session`]'s single cost field, [`total_cost`](Session::total_cost), is the
//! VAT-only [`crate::common::Price`] through 2.2.1. 2.3.0 reworks that value type
//! into the tax-itemised [`Price`](super::Price) (`before_taxes` + an itemised
//! [`TaxAmount`](super::TaxAmount) list, slice 1 / #190). Per
//! `specs/ocpi/2.3.0/mod_sessions.asciidoc` the Session object is otherwise
//! structurally the 2.2.1 shape, so this fork changes exactly one field relative
//! to [`crate::v2_2_1::Session`]: `total_cost` carries the reworked
//! [`Price`](super::Price).
//!
//! Every other field is wire-identical to 2.2.1, so the sub-types
//! ([`CdrToken`](crate::v2_2_1::CdrToken), [`AuthMethod`](crate::v2_2_1::AuthMethod),
//! [`ChargingPeriod`](crate::v2_2_1::ChargingPeriod) — which carries no cost, only
//! metered dimensions — and [`SessionStatus`](crate::v2_2_1::SessionStatus)) stay
//! plain re-uses of the 2.2.1 types.
//!
//! ### The trust boundary
//!
//! `total_cost` is optional (spec card. `?`), but when present its embedded
//! [`Price`](super::Price) routes through serde's derived `Deserialize`: a
//! `total_cost` that omits the required `before_taxes` is **rejected on
//! deserialize** rather than silently defaulted — faithful to the crate's core
//! promise (*the unsupported case is rejected explicitly, never silently
//! mangled*).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::common::{CiString2, CiString3, CiString36};
use crate::v2_2_1::{AuthMethod, CdrToken, ChargingPeriod, SessionStatus};

use super::Price;

/// A charging session object owned by the CPO (OCPI **2.3.0**).
///
/// The 2.2.1 [`Session`](crate::v2_2_1::Session) shape with its
/// [`total_cost`](Session::total_cost) reworked onto the tax-itemised 2.3.0
/// [`Price`](super::Price). Describes one charging session from start to finish,
/// including token, location references, energy consumed, and cost. Transmitted
/// from CPO to eMSP via push (PUT/PATCH) or pull (GET list).
///
/// Spec: `specs/ocpi/2.3.0/mod_sessions.asciidoc` — Session object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    /// ISO 3166-1 alpha-2 country code of the CPO that owns this session.
    pub country_code: CiString2,
    /// Party ID of the CPO that owns this session.
    pub party_id: CiString3,
    /// Unique session ID within the CPO's platform.
    pub id: CiString36,
    /// Timestamp when the session became ACTIVE (or was created if PENDING).
    pub start_date_time: DateTime<Utc>,
    /// Timestamp when the session ended. `None` if still active.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub end_date_time: Option<DateTime<Utc>>,
    /// Total energy charged so far, in kWh.
    pub kwh: f64,
    /// Token used to start the session.
    pub cdr_token: CdrToken,
    /// Authentication method used (may change during the session).
    pub auth_method: AuthMethod,
    /// Authorization reference from the eMSP (e.g. from StartSession).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub authorization_reference: Option<CiString36>,
    /// `Location.id` of the CPO location where this session is happening.
    pub location_id: CiString36,
    /// `EVSE.uid` at the location (`#NA` for reservation without EVSE assignment).
    pub evse_uid: CiString36,
    /// `Connector.id` at the EVSE (`#NA` for reservation without connector assignment).
    pub connector_id: CiString36,
    /// Optional meter identifier inside the charge point.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub meter_id: Option<String>,
    /// ISO 4217 currency code (e.g. `"EUR"`).
    pub currency: String,
    /// Charging periods recorded so far (may be empty if no periods yet).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub charging_periods: Vec<ChargingPeriod>,
    /// Total cost of the session so far in `currency`. `None` if not yet known.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub total_cost: Option<Price>,
    /// Current status of the session.
    pub status: SessionStatus,
    /// Timestamp of the last update (or creation) of this session object.
    pub last_updated: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::Session;

    /// A North-American session: `total_cost` carries `before_taxes` plus an
    /// itemised `taxes` list, and the whole object round-trips unmangled.
    #[test]
    fn north_american_taxed_session_round_trips() {
        let json = r#"{
            "country_code": "CA",
            "party_id": "EXA",
            "id": "session-2-3-0-na",
            "start_date_time": "2026-07-11T09:00:00Z",
            "kwh": 12.5,
            "cdr_token": {
                "country_code": "CA",
                "party_id": "EXA",
                "uid": "12345",
                "type": "RFID",
                "contract_id": "CA-EXA-C12345"
            },
            "auth_method": "WHITELIST",
            "location_id": "LOC1",
            "evse_uid": "EVSE1",
            "connector_id": "1",
            "currency": "CAD",
            "total_cost": {
                "before_taxes": 10.00,
                "taxes": [
                    { "name": "GST", "percentage": 5.0, "amount": 0.50 },
                    { "name": "QST", "percentage": 9.975, "amount": 0.9975 }
                ]
            },
            "status": "ACTIVE",
            "last_updated": "2026-07-11T09:05:00Z"
        }"#;
        let session: Session = serde_json::from_str(json).unwrap();
        assert_eq!(session.total_cost.as_ref().unwrap().before_taxes, 10.00);
        assert_eq!(session.total_cost.as_ref().unwrap().taxes.len(), 2);
        assert_eq!(session.total_cost.as_ref().unwrap().taxes[1].name, "QST");

        let out = serde_json::to_string(&session).unwrap();
        let back: Session = serde_json::from_str(&out).unwrap();
        assert_eq!(back, session);
    }

    /// A session whose cost is not yet known omits `total_cost` on the wire.
    #[test]
    fn session_without_total_cost_omits_it_on_the_wire() {
        let json = r#"{
            "country_code": "NL",
            "party_id": "ABC",
            "id": "session-pending",
            "start_date_time": "2026-07-11T09:00:00Z",
            "kwh": 0.0,
            "cdr_token": {
                "country_code": "NL", "party_id": "ABC", "uid": "1",
                "type": "RFID", "contract_id": "NL-ABC-C1"
            },
            "auth_method": "WHITELIST",
            "location_id": "LOC1",
            "evse_uid": "EVSE1",
            "connector_id": "1",
            "currency": "EUR",
            "status": "PENDING",
            "last_updated": "2026-07-11T09:00:00Z"
        }"#;
        let session: Session = serde_json::from_str(json).unwrap();
        assert!(session.total_cost.is_none());
        let out = serde_json::to_string(&session).unwrap();
        assert!(!out.contains("total_cost"), "absent cost omitted: {out}");
    }

    /// A `total_cost` present but missing the required `before_taxes` is rejected
    /// on deserialize rather than defaulted to zero.
    #[test]
    fn session_total_cost_without_before_taxes_is_rejected() {
        let json = r#"{
            "country_code": "NL",
            "party_id": "ABC",
            "id": "session-bad-cost",
            "start_date_time": "2026-07-11T09:00:00Z",
            "kwh": 1.0,
            "cdr_token": {
                "country_code": "NL", "party_id": "ABC", "uid": "1",
                "type": "RFID", "contract_id": "NL-ABC-C1"
            },
            "auth_method": "WHITELIST",
            "location_id": "LOC1",
            "evse_uid": "EVSE1",
            "connector_id": "1",
            "currency": "EUR",
            "total_cost": { "taxes": [] },
            "status": "ACTIVE",
            "last_updated": "2026-07-11T09:00:00Z"
        }"#;
        assert!(serde_json::from_str::<Session>(json).is_err());
    }

    /// Proof this is a genuine wire fork, not an alias: a 2.3.0 session whose
    /// `total_cost` is the reworked `before_taxes` Price is NOT a valid 2.2.1
    /// [`Session`](crate::v2_2_1::Session) (whose `total_cost` needs `excl_vat`).
    #[test]
    fn session_2_3_0_total_cost_shape_differs_from_2_2_1() {
        let json = r#"{
            "country_code": "CA",
            "party_id": "EXA",
            "id": "session-delta",
            "start_date_time": "2026-07-11T09:00:00Z",
            "kwh": 1.0,
            "cdr_token": {
                "country_code": "CA", "party_id": "EXA", "uid": "1",
                "type": "RFID", "contract_id": "CA-EXA-C1"
            },
            "auth_method": "WHITELIST",
            "location_id": "LOC1",
            "evse_uid": "EVSE1",
            "connector_id": "1",
            "currency": "CAD",
            "total_cost": { "before_taxes": 1.0 },
            "status": "ACTIVE",
            "last_updated": "2026-07-11T09:00:00Z"
        }"#;
        assert!(serde_json::from_str::<Session>(json).is_ok());
        assert!(serde_json::from_str::<crate::v2_2_1::Session>(json).is_err());
    }

    /// A session with no `total_cost` at all *is* wire-compatible with 2.2.1
    /// (the fork only changes the cost value type): the same cost-free payload
    /// parses under both versions, which is why the delta is scoped to the cost
    /// field alone.
    #[test]
    fn cost_free_session_still_parses_under_2_2_1() {
        let json = r#"{
            "country_code": "NL",
            "party_id": "ABC",
            "id": "session-shared",
            "start_date_time": "2026-07-11T09:00:00Z",
            "kwh": 3.0,
            "cdr_token": {
                "country_code": "NL", "party_id": "ABC", "uid": "1",
                "type": "RFID", "contract_id": "NL-ABC-C1"
            },
            "auth_method": "WHITELIST",
            "location_id": "LOC1",
            "evse_uid": "EVSE1",
            "connector_id": "1",
            "currency": "EUR",
            "status": "ACTIVE",
            "last_updated": "2026-07-11T09:00:00Z"
        }"#;
        assert!(serde_json::from_str::<Session>(json).is_ok());
        assert!(serde_json::from_str::<crate::v2_2_1::Session>(json).is_ok());
    }
}

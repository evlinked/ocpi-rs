//! OCPI **2.3.0** CDRs — the North-American tax delta over 2.2.1 (slice 2 of #188).
//!
//! Through 2.2.1 a [`Cdr`]'s cost fields are the VAT-only
//! [`crate::common::Price`] (`excl_vat` / `incl_vat`). 2.3.0 reworks that value
//! type into the tax-itemised [`Price`](super::Price) (`before_taxes` + an
//! itemised [`TaxAmount`](super::TaxAmount) list, slice 1 / #190). Per
//! `specs/ocpi/2.3.0/mod_cdrs.asciidoc` the CDR object is otherwise structurally
//! the 2.2.1 shape, so this fork changes exactly two things relative to
//! [`crate::v2_2_1::Cdr`]:
//!
//! - every cost field ([`total_cost`](Cdr::total_cost),
//!   [`total_fixed_cost`](Cdr::total_fixed_cost),
//!   [`total_energy_cost`](Cdr::total_energy_cost),
//!   [`total_time_cost`](Cdr::total_time_cost),
//!   [`total_parking_cost`](Cdr::total_parking_cost),
//!   [`total_reservation_cost`](Cdr::total_reservation_cost)) carries the
//!   reworked [`Price`](super::Price); and
//! - the embedded [`tariffs`](Cdr::tariffs) list is the 2.3.0
//!   [`Tariff`](super::Tariff) (with its required `tax_included` flag), as the
//!   CDR object table references the 2.3.0 Tariff object.
//!
//! Every other field is wire-identical to 2.2.1, so the sub-types
//! ([`CdrToken`](crate::v2_2_1::CdrToken), [`AuthMethod`](crate::v2_2_1::AuthMethod),
//! [`CdrLocation`](crate::v2_2_1::CdrLocation),
//! [`ChargingPeriod`](crate::v2_2_1::ChargingPeriod) — which carries no cost, only
//! metered dimensions — and [`SignedData`](crate::v2_2_1::SignedData)) stay plain
//! re-uses of the 2.2.1 types.
//!
//! ### The trust boundary
//!
//! [`total_cost`](Cdr::total_cost) is required (spec card. `1`), and both it and
//! the embedded [`Price`](super::Price) route through serde's derived
//! `Deserialize`: a payload that omits `total_cost`, or whose `total_cost` omits
//! the required `before_taxes`, is **rejected on deserialize** rather than
//! silently defaulted — faithful to the crate's core promise (*the unsupported
//! case is rejected explicitly, never silently mangled*).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::common::{CiString2, CiString3, CiString36, CiString39};
use crate::v2_2_1::{AuthMethod, CdrLocation, CdrToken, ChargingPeriod, SignedData};

use super::{Price, Tariff};

/// A Charge Detail Record — the billing artifact for a completed charging
/// session (OCPI **2.3.0**).
///
/// The 2.2.1 [`Cdr`](crate::v2_2_1::Cdr) shape with its cost fields reworked onto
/// the tax-itemised 2.3.0 [`Price`](super::Price) and its embedded
/// [`tariffs`](Cdr::tariffs) carrying the 2.3.0 [`Tariff`](super::Tariff). CDRs
/// are immutable after creation; corrections are issued via a Credit CDR (set
/// `credit = true` and point `credit_reference_id` at the original CDR).
///
/// Spec: `specs/ocpi/2.3.0/mod_cdrs.asciidoc` — CDR object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cdr {
    /// ISO 3166-1 alpha-2 country code of the CPO that owns this CDR.
    pub country_code: CiString2,
    /// Party ID of the CPO that owns this CDR.
    pub party_id: CiString3,
    /// Unique CDR ID. Credit CDRs may append a suffix (e.g. `-C`), raising
    /// the limit to 39 chars; normal CDRs must stay within 36.
    pub id: CiString39,
    /// Timestamp when charging started (or reservation started).
    pub start_date_time: DateTime<Utc>,
    /// Timestamp when the session ended (charging + any post-charge parking).
    pub end_date_time: DateTime<Utc>,
    /// ID of the corresponding `Session`, if the Sessions module is in use.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub session_id: Option<CiString36>,
    /// Token used to start the session.
    pub cdr_token: CdrToken,
    /// Authentication method used (the last method if multiple were used).
    pub auth_method: AuthMethod,
    /// Authorization reference provided by the eMSP, when applicable.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub authorization_reference: Option<CiString36>,
    /// Compact location snapshot for the EVSE and connector used.
    pub cdr_location: CdrLocation,
    /// Meter identifier inside the charge point, if known.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub meter_id: Option<String>,
    /// ISO 4217 currency code for all cost fields.
    pub currency: String,
    /// Tariffs applicable to this session (may be empty). The 2.3.0
    /// [`Tariff`](super::Tariff), carrying the North-American `tax_included` flag.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tariffs: Vec<Tariff>,
    /// Charging periods that make up this session (one or more required).
    pub charging_periods: Vec<ChargingPeriod>,
    /// Signed metering data (e.g. Eichrecht), when present.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub signed_data: Option<SignedData>,
    /// Total cost of the transaction in `currency`.
    pub total_cost: Price,
    /// Fixed cost component (start fee, etc.), excluding parking/reservation fixed fees.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub total_fixed_cost: Option<Price>,
    /// Total energy charged, in kWh.
    pub total_energy: f64,
    /// Cost of all energy consumed.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub total_energy_cost: Option<Price>,
    /// Total session duration (charging + non-charging), in hours.
    pub total_time: f64,
    /// Cost of charging-time duration.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub total_time_cost: Option<Price>,
    /// Time the EV was connected but not charging, in hours.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub total_parking_time: Option<f64>,
    /// Cost of post-charge parking.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub total_parking_cost: Option<Price>,
    /// Cost of the reservation, if any.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub total_reservation_cost: Option<Price>,
    /// Human-readable remark (e.g. reason for early stop).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub remark: Option<String>,
    /// Invoice reference — links this CDR to a future invoice.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub invoice_reference_id: Option<CiString39>,
    /// `true` if this CDR is a Credit CDR that negates a previous session.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub credit: Option<bool>,
    /// ID of the original CDR being credited (required when `credit` is `true`).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub credit_reference_id: Option<CiString39>,
    /// `true` if this session used the driver's home charger and energy cost
    /// compensation applies.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub home_charging_compensation: Option<bool>,
    /// Timestamp of the last update (or creation) of this CDR object.
    pub last_updated: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::Cdr;

    /// A North-American CDR: `total_cost` carries `before_taxes` plus an itemised
    /// GST+QST `taxes` list, and the embedded tariff carries `tax_included`. The
    /// whole object round-trips unmangled.
    #[test]
    fn north_american_taxed_cdr_round_trips() {
        let json = r#"{
            "country_code": "CA",
            "party_id": "EXA",
            "id": "cdr-2-3-0-na",
            "start_date_time": "2026-07-11T09:00:00Z",
            "end_date_time": "2026-07-11T10:00:00Z",
            "cdr_token": {
                "country_code": "CA",
                "party_id": "EXA",
                "uid": "12345",
                "type": "RFID",
                "contract_id": "CA-EXA-C12345"
            },
            "auth_method": "WHITELIST",
            "cdr_location": {
                "id": "LOC1",
                "address": "F.Rooseveltlaan 3A",
                "city": "Ottawa",
                "country": "CAN",
                "coordinates": { "latitude": "45.421", "longitude": "-75.697" },
                "evse_uid": "EVSE1",
                "evse_id": "CA*EXA*E1",
                "connector_id": "1",
                "connector_standard": "IEC_62196_T2",
                "connector_format": "SOCKET",
                "connector_power_type": "AC_1_PHASE"
            },
            "currency": "CAD",
            "tariffs": [
                {
                    "country_code": "CA",
                    "party_id": "EXA",
                    "id": "19",
                    "currency": "CAD",
                    "tax_included": "NO",
                    "elements": [
                        { "price_components": [ { "type": "TIME", "price": 2.0, "step_size": 60 } ] }
                    ],
                    "last_updated": "2026-07-11T09:00:00Z"
                }
            ],
            "charging_periods": [
                {
                    "start_date_time": "2026-07-11T09:00:00Z",
                    "dimensions": [ { "type": "TIME", "volume": 1.0 } ]
                }
            ],
            "total_cost": {
                "before_taxes": 2.00,
                "taxes": [
                    { "name": "GST", "percentage": 5.0, "amount": 0.10 },
                    { "name": "QST", "account_number": "1234567890", "percentage": 9.975, "amount": 0.1995 }
                ]
            },
            "total_time_cost": { "before_taxes": 2.00, "taxes": [ { "name": "GST", "amount": 0.10 } ] },
            "total_energy": 0.0,
            "total_time": 1.0,
            "last_updated": "2026-07-11T10:00:00Z"
        }"#;
        let cdr: Cdr = serde_json::from_str(json).unwrap();
        assert_eq!(cdr.total_cost.before_taxes, 2.00);
        assert_eq!(cdr.total_cost.taxes.len(), 2);
        assert_eq!(cdr.total_cost.taxes[1].name, "QST");
        assert_eq!(
            cdr.total_cost.taxes[1].account_number.as_deref(),
            Some("1234567890")
        );
        // The embedded tariff is the 2.3.0 fork — its required `tax_included` parsed.
        assert_eq!(cdr.tariffs.len(), 1);
        assert_eq!(cdr.tariffs[0].tax_included, super::super::TaxIncluded::No);
        assert_eq!(cdr.total_time_cost.as_ref().unwrap().taxes[0].name, "GST");

        let out = serde_json::to_string(&cdr).unwrap();
        let back: Cdr = serde_json::from_str(&out).unwrap();
        assert_eq!(back, cdr);
    }

    /// A tax-free CDR (European-style, empty `taxes`): the optional cost fields
    /// and the empty `taxes` list are omitted on the wire.
    #[test]
    fn tax_free_cdr_omits_empty_cost_extras_on_the_wire() {
        let json = r#"{
            "country_code": "NL",
            "party_id": "ABC",
            "id": "cdr-free",
            "start_date_time": "2026-07-11T09:00:00Z",
            "end_date_time": "2026-07-11T10:00:00Z",
            "cdr_token": {
                "country_code": "NL",
                "party_id": "ABC",
                "uid": "12345",
                "type": "RFID",
                "contract_id": "NL-ABC-C12345"
            },
            "auth_method": "WHITELIST",
            "cdr_location": {
                "id": "LOC1",
                "address": "F.Rooseveltlaan 3A",
                "city": "Rotterdam",
                "country": "NLD",
                "coordinates": { "latitude": "51.047", "longitude": "3.729" },
                "evse_uid": "EVSE1",
                "evse_id": "NL*ABC*E1",
                "connector_id": "1",
                "connector_standard": "IEC_62196_T2",
                "connector_format": "SOCKET",
                "connector_power_type": "AC_1_PHASE"
            },
            "currency": "EUR",
            "charging_periods": [
                {
                    "start_date_time": "2026-07-11T09:00:00Z",
                    "dimensions": [ { "type": "ENERGY", "volume": 5.0 } ]
                }
            ],
            "total_cost": { "before_taxes": 0.00 },
            "total_energy": 5.0,
            "total_time": 1.0,
            "last_updated": "2026-07-11T10:00:00Z"
        }"#;
        let cdr: Cdr = serde_json::from_str(json).unwrap();
        assert!(cdr.total_cost.taxes.is_empty());
        assert!(cdr.total_time_cost.is_none());

        let out = serde_json::to_string(&cdr).unwrap();
        assert!(!out.contains("\"taxes\""), "empty taxes omitted: {out}");
        assert!(!out.contains("total_time_cost"));
        assert!(!out.contains("tariffs"));
    }

    /// `total_cost` is required (card. 1). A CDR omitting it is rejected on
    /// deserialize rather than defaulted.
    #[test]
    fn cdr_without_total_cost_is_rejected() {
        let json = r#"{
            "country_code": "NL",
            "party_id": "ABC",
            "id": "cdr-no-cost",
            "start_date_time": "2026-07-11T09:00:00Z",
            "end_date_time": "2026-07-11T10:00:00Z",
            "cdr_token": {
                "country_code": "NL", "party_id": "ABC", "uid": "1",
                "type": "RFID", "contract_id": "NL-ABC-C1"
            },
            "auth_method": "WHITELIST",
            "cdr_location": {
                "id": "LOC1", "address": "a", "city": "c", "country": "NLD",
                "coordinates": { "latitude": "51.0", "longitude": "3.7" },
                "evse_uid": "E1", "evse_id": "NL*ABC*E1", "connector_id": "1",
                "connector_standard": "IEC_62196_T2", "connector_format": "SOCKET",
                "connector_power_type": "AC_1_PHASE"
            },
            "currency": "EUR",
            "charging_periods": [
                { "start_date_time": "2026-07-11T09:00:00Z", "dimensions": [] }
            ],
            "total_energy": 5.0,
            "total_time": 1.0,
            "last_updated": "2026-07-11T10:00:00Z"
        }"#;
        assert!(serde_json::from_str::<Cdr>(json).is_err());
    }

    /// Proof this is a genuine wire fork, not an alias: the 2.3.0 CDR's
    /// `total_cost` (a `before_taxes` body) is NOT a valid 2.2.1 CDR `total_cost`
    /// (which needs `excl_vat`), so the same payload fails to parse as a 2.2.1
    /// [`Cdr`](crate::v2_2_1::Cdr).
    #[test]
    fn cdr_2_3_0_total_cost_shape_differs_from_2_2_1() {
        let json = r#"{
            "country_code": "NL",
            "party_id": "ABC",
            "id": "cdr-delta",
            "start_date_time": "2026-07-11T09:00:00Z",
            "end_date_time": "2026-07-11T10:00:00Z",
            "cdr_token": {
                "country_code": "NL", "party_id": "ABC", "uid": "1",
                "type": "RFID", "contract_id": "NL-ABC-C1"
            },
            "auth_method": "WHITELIST",
            "cdr_location": {
                "id": "LOC1", "address": "a", "city": "c", "country": "NLD",
                "coordinates": { "latitude": "51.0", "longitude": "3.7" },
                "evse_uid": "E1", "evse_id": "NL*ABC*E1", "connector_id": "1",
                "connector_standard": "IEC_62196_T2", "connector_format": "SOCKET",
                "connector_power_type": "AC_1_PHASE"
            },
            "currency": "EUR",
            "charging_periods": [
                { "start_date_time": "2026-07-11T09:00:00Z", "dimensions": [] }
            ],
            "total_cost": { "before_taxes": 1.0 },
            "total_energy": 5.0,
            "total_time": 1.0,
            "last_updated": "2026-07-11T10:00:00Z"
        }"#;
        assert!(serde_json::from_str::<Cdr>(json).is_ok());
        assert!(serde_json::from_str::<crate::v2_2_1::Cdr>(json).is_err());
    }
}

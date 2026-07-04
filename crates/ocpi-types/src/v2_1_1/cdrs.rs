//! OCPI 2.1.1 — CDRs module types.
//!
//! A [`Cdr`] (Charge Detail Record) is the billing-of-record document for one
//! completed charging session, owned and pushed by the CPO. It is the
//! settlement source of truth, so the 2.1.1 shape must be reproduced exactly
//! for any back-coverage settlement path.
//!
//! ## Deltas from the 2.2.1 [`crate::v2_2_1::Cdr`]
//!
//! - **No `country_code` / `party_id`** (added in 2.2 once CDRs became
//!   addressable per party).
//! - **No `session_id`** — 2.1.1 does not link a CDR back to a Session object.
//! - Authorization is a bare **`auth_id: CiString36`** + **`auth_method`**, not
//!   the 2.2.1 embedded `cdr_token: CdrToken` object.
//! - **`location` is the full embedded [`Location`] object**, not the 2.2.1
//!   slim `cdr_location: CdrLocation` snapshot.
//! - The stop timestamp is **`stop_date_time`** (a 2.1.1 spelling; 2.2 renamed
//!   it to `end_date_time`). See [`Cdr::stop_date_time`].
//! - **A single `total_cost: number`** plus `total_energy` / `total_time` /
//!   optional `total_parking_time` — none of the 2.2.1 cost breakdown
//!   (`total_fixed_cost` / `total_energy_cost` / `total_time_cost` / …) and no
//!   `Price` object; 2.1.1 costs are bare numbers.
//! - **No** `signed_data`, `authorization_reference`, `invoice_reference_id`,
//!   `credit` / `credit_reference_id`, or `home_charging_compensation` (all
//!   2.2+).
//!
//! The shared charging-period value types ([`AuthMethod`], [`CdrDimension`],
//! [`CdrDimensionType`], [`ChargingPeriod`]) are spec-defined in this CDRs
//! module (§10.4) but used by both Sessions and CDRs. They are authored in
//! [`super::sessions`] (so the Sessions PR is self-contained) and reused here
//! rather than redefined, keeping a single source of truth.
//!
//! Spec: OCPI 2.1.1 — *CDRs module*, *CDR Object* (§10.3.1) and *Data types*
//! (§10.4), `specs/ocpi/2.1.1/OCPI_2.1.1.pdf`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::locations::Location;
use super::sessions::{AuthMethod, ChargingPeriod};
use super::tariffs::Tariff;
use crate::common::CiString36;

// ── Cdr ───────────────────────────────────────────────────────────────────────

/// An OCPI **2.1.1** Charge Detail Record — the costed description of one
/// completed charging session, owned and pushed by the CPO.
///
/// See the module-level documentation for the full delta list against 2.2.1.
///
/// Spec: OCPI 2.1.1 — *CDRs* module, *CDR Object* (§10.3.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cdr {
    /// Uniquely identifies the CDR within the CPO's platform (and suboperator
    /// platforms).
    pub id: CiString36,
    /// Start timestamp of the charging session.
    pub start_date_time: DateTime<Utc>,
    /// Stop timestamp of the charging session.
    ///
    /// Wire field is **`stop_date_time`** — the 2.1.1 spelling, renamed to
    /// `end_date_time` in 2.2.
    pub stop_date_time: DateTime<Utc>,
    /// Reference to the token that authorized the session — the `auth_id` field
    /// of the [`crate::v2_1_1::Token`], not an embedded `CdrToken` (which is
    /// 2.2.1).
    pub auth_id: CiString36,
    /// Method used for authentication.
    pub auth_method: AuthMethod,
    /// Location where the charging session took place, including only the
    /// relevant EVSE and connector. Embedded full object in 2.1.1.
    pub location: Location,
    /// Identification of the meter inside the charge point, if known.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub meter_id: Option<String>,
    /// ISO 4217 currency code for all cost fields.
    pub currency: String,
    /// Relevant tariff elements. When applicable a "Free of Charge" tariff
    /// should also appear here. May be empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tariffs: Vec<Tariff>,
    /// Charging periods that make up this session (one or more). Each period
    /// may reference a different relevant tariff.
    pub charging_periods: Vec<ChargingPeriod>,
    /// Total cost (excluding VAT) of this transaction, in `currency`. A bare
    /// number in 2.1.1 — not a `Price` object.
    pub total_cost: f64,
    /// Total energy charged, in kWh.
    pub total_energy: f64,
    /// Total duration of the session (charging and not charging), in hours.
    pub total_time: f64,
    /// Total duration during the session that the EV was not charging, in
    /// hours.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub total_parking_time: Option<f64>,
    /// Optional human-readable remark (e.g. why a transaction was stopped).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub remark: Option<String>,
    /// Timestamp when this CDR was last updated (or created).
    pub last_updated: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v2_1_1::{CdrDimension, CdrDimensionType};

    #[test]
    fn cdr_serde_spec_example() {
        // Ported from the OCPI 2.1.1 "CDR Object" example (§10.3.1): note
        // `stop_date_time` (not `end_date_time`), `auth_id` (not a `cdr_token`),
        // an embedded full `location` object, and bare numeric cost fields. No
        // `country_code` / `party_id` / `session_id`.
        let json = r#"{
            "id": "12345",
            "start_date_time": "2015-06-29T21:39:09Z",
            "stop_date_time": "2015-06-29T23:37:32Z",
            "auth_id": "DE8ACC12E46L89",
            "auth_method": "WHITELIST",
            "location": {
                "id": "LOC1",
                "type": "ON_STREET",
                "name": "Gent Zuid",
                "address": "F.Rooseveltlaan 3A",
                "city": "Gent",
                "postal_code": "9000",
                "country": "BEL",
                "coordinates": { "latitude": "51.047599", "longitude": "3.729944" },
                "evses": [{
                    "uid": "3256",
                    "evse_id": "BE-BEC-E041503003",
                    "status": "AVAILABLE",
                    "connectors": [{
                        "id": "1",
                        "standard": "IEC_62196_T2",
                        "format": "SOCKET",
                        "power_type": "AC_1_PHASE",
                        "voltage": 230,
                        "amperage": 64,
                        "tariff_id": "11",
                        "last_updated": "2015-06-29T21:39:01Z"
                    }],
                    "last_updated": "2015-06-29T21:39:01Z"
                }],
                "last_updated": "2015-06-29T21:39:01Z"
            },
            "currency": "EUR",
            "tariffs": [{
                "id": "12",
                "currency": "EUR",
                "elements": [{
                    "price_components": [{
                        "type": "TIME",
                        "price": 2.00,
                        "step_size": 300
                    }]
                }],
                "last_updated": "2015-02-02T14:15:01Z"
            }],
            "charging_periods": [{
                "start_date_time": "2015-06-29T21:39:09Z",
                "dimensions": [{ "type": "TIME", "volume": 1.973 }]
            }],
            "total_cost": 4.00,
            "total_energy": 15.342,
            "total_time": 1.973,
            "last_updated": "2015-06-29T22:01:13Z"
        }"#;

        let cdr: Cdr = serde_json::from_str(json).unwrap();
        assert_eq!(cdr.id.as_str(), "12345");
        assert_eq!(cdr.auth_id.as_str(), "DE8ACC12E46L89");
        assert_eq!(cdr.auth_method, AuthMethod::Whitelist);
        assert_eq!(cdr.location.id.as_str(), "LOC1");
        assert_eq!(cdr.location.evses.len(), 1);
        assert_eq!(cdr.tariffs.len(), 1);
        assert_eq!(cdr.charging_periods.len(), 1);
        assert_eq!(
            cdr.charging_periods[0].dimensions[0].dimension_type,
            CdrDimensionType::Time
        );
        assert_eq!(cdr.total_cost, 4.00);
        assert_eq!(cdr.total_energy, 15.342);
        assert_eq!(cdr.total_time, 1.973);
        assert!(cdr.total_parking_time.is_none());
        assert!(cdr.meter_id.is_none());

        let back: Cdr = serde_json::from_str(&serde_json::to_string(&cdr).unwrap()).unwrap();
        assert_eq!(back, cdr);
    }

    /// A minimal valid 2.1.1 CDR built from typed values (empty `evses` keeps
    /// the embedded location terse) for wire-shape assertions.
    fn sample_cdr() -> Cdr {
        Cdr {
            id: "12345".try_into().unwrap(),
            start_date_time: "2015-06-29T21:39:09Z".parse().unwrap(),
            stop_date_time: "2015-06-29T23:37:32Z".parse().unwrap(),
            auth_id: "DE8ACC12E46L89".try_into().unwrap(),
            auth_method: AuthMethod::AuthRequest,
            location: Location {
                id: "LOC1".try_into().unwrap(),
                location_type: crate::v2_1_1::LocationType::OnStreet,
                name: Some("Gent Zuid".to_string()),
                address: "F.Rooseveltlaan 3A".to_string(),
                city: "Gent".to_string(),
                postal_code: "9000".to_string(),
                country: "BEL".to_string(),
                coordinates: crate::common::GeoLocation {
                    latitude: "51.047599".to_string(),
                    longitude: "3.729944".to_string(),
                },
                related_locations: vec![],
                evses: vec![],
                directions: vec![],
                operator: None,
                suboperator: None,
                owner: None,
                facilities: vec![],
                time_zone: Some("Europe/Brussels".to_string()),
                opening_times: None,
                charging_when_closed: None,
                images: vec![],
                energy_mix: None,
                last_updated: "2015-06-29T21:39:01Z".parse().unwrap(),
            },
            meter_id: Some("METER-1".to_string()),
            currency: "EUR".to_string(),
            tariffs: vec![],
            charging_periods: vec![ChargingPeriod {
                start_date_time: "2015-06-29T21:39:09Z".parse().unwrap(),
                dimensions: vec![CdrDimension {
                    dimension_type: CdrDimensionType::Energy,
                    volume: 15.342,
                }],
            }],
            total_cost: 4.0,
            total_energy: 15.342,
            total_time: 1.973,
            total_parking_time: None,
            remark: None,
            last_updated: "2015-06-29T22:01:13Z".parse().unwrap(),
        }
    }

    #[test]
    fn cdr_wire_uses_2_1_1_shape_and_omits_2_2_fields() {
        let cdr = sample_cdr();
        let json = serde_json::to_string(&cdr).unwrap();

        // Bare numeric costs (not a `Price` object).
        assert!(json.contains("\"total_cost\":4.0"), "{json}");
        assert!(json.contains("\"total_energy\":15.342"), "{json}");

        // Inspect the *top-level* CDR keys precisely — a substring scan would
        // false-positive on the embedded `ChargingPeriod.start_date_time` and
        // on the embedded `Location`.
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let keys = value.as_object().unwrap();

        // 2.1.1 carries `stop_date_time`, not the 2.2 `end_date_time`.
        assert!(keys.contains_key("stop_date_time"), "{json}");
        assert!(keys.contains_key("auth_id"), "{json}");
        assert!(keys.contains_key("auth_method"), "{json}");

        // None of the 2.2 / 2.2.1 additions may appear at the CDR level.
        for absent in [
            "end_date_time",
            "country_code",
            "party_id",
            "session_id",
            "cdr_token",
            "cdr_location",
            "authorization_reference",
            "signed_data",
            "total_fixed_cost",
            "total_energy_cost",
            "total_time_cost",
            "total_parking_cost",
            "total_reservation_cost",
            "invoice_reference_id",
            "credit",
            "credit_reference_id",
            "home_charging_compensation",
        ] {
            assert!(
                !keys.contains_key(absent),
                "2.1.1 CDR must not carry top-level `{absent}`: {json}"
            );
        }

        let back: Cdr = serde_json::from_str(&json).unwrap();
        assert_eq!(back, cdr);
    }

    #[test]
    fn cdr_omits_optional_fields_when_absent() {
        let mut cdr = sample_cdr();
        cdr.meter_id = None;
        cdr.tariffs = vec![];
        cdr.total_parking_time = None;
        cdr.remark = None;
        let json = serde_json::to_string(&cdr).unwrap();
        for absent in ["meter_id", "tariffs", "total_parking_time", "remark"] {
            assert!(
                !json.contains(absent),
                "absent 2.1.1 CDR field `{absent}` must be skipped: {json}"
            );
        }
    }
}

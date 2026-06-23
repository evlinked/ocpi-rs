//! OCPI 2.1.1 — Sessions module types.
//!
//! A [`Session`] describes one charging session, owned and pushed by the CPO.
//! The 2.1.1 shape predates several 2.2 additions and carries a few genuine
//! wire-format quirks that this module reproduces faithfully:
//!
//! - The Session timestamps are **`start_datetime` / `end_datetime`** — one
//!   word, not the `start_date_time` form used everywhere else (and corrected
//!   in 2.2). See [`Session::start_datetime`].
//! - **No `country_code` / `party_id`** on the object (added in 2.2 once
//!   sessions became addressable per party).
//! - Authorization is a bare **`auth_id: CiString36`** — the `auth_id` of the
//!   driver's token — not the 2.2.1 embedded `CdrToken` object.
//! - **`location` is the full embedded [`Location`] object**, not the 2.2.1
//!   `location_id` / `evse_uid` / `connector_id` reference triple.
//! - **No** smart-charging fields (`ChargingPreferences`, `ProfileType`,
//!   `charging_preferences`) — those are 2.2+.
//!
//! The shared charging-period value types ([`AuthMethod`], [`CdrDimension`],
//! [`CdrDimensionType`], [`ChargingPeriod`]) are spec-defined in the 2.1.1 CDRs
//! module (§10.4) but used by both Sessions and CDRs; they live here so the
//! Sessions PR is self-contained, and the forthcoming 2.1.1 CDRs module reuses
//! them via `super::sessions::{…}` rather than redefining them.
//!
//! Spec: OCPI 2.1.1 — *Sessions module* (§9) and *CDRs module* data types
//! (§10.4), `specs/ocpi/2.1.1/OCPI_2.1.1.pdf`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::locations::Location;
use crate::common::CiString36;

// ── AuthMethod ──────────────────────────────────────────────────────────────

/// Method used to authenticate a charging session.
///
/// ## Delta from 2.2.1
///
/// OCPI 2.1.1 defines only `AUTH_REQUEST` and `WHITELIST` (§10.4.1). The
/// `COMMAND` variant — authorization via a `StartSession`/`ReserveNow` command
/// — was introduced in 2.2 and is deliberately absent: a 2.1.1 peer never
/// emits it.
///
/// Spec: OCPI 2.1.1 — *CDRs* module, `AuthMethod` enum (§10.4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuthMethod {
    /// Authentication request was sent to the eMSP.
    AuthRequest,
    /// Whitelist was used; no real-time request was made to the eMSP.
    Whitelist,
}

// ── CdrDimensionType ────────────────────────────────────────────────────────

/// Measurement dimension type for a [`ChargingPeriod`].
///
/// ## Delta from 2.2.1
///
/// The 2.1.1 set is exactly `ENERGY`, `FLAT`, `MAX_CURRENT`, `MIN_CURRENT`,
/// `PARKING_TIME`, `TIME` (§10.4.3). It still carries **`FLAT`** (a flat fee
/// with no unit), which 2.2 moved out of `CdrDimensionType`; and it lacks every
/// dimension added later (`CURRENT`, `POWER`, `STATE_OF_CHARGE`,
/// `ENERGY_IMPORT/EXPORT`, `MAX_POWER`, `MIN_POWER`, `RESERVATION_TIME`).
///
/// Spec: OCPI 2.1.1 — *CDRs* module, `CdrDimensionType` enum (§10.4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CdrDimensionType {
    /// Consumed energy in kWh (default `step_size` 1 Wh).
    Energy,
    /// Flat fee — no unit.
    Flat,
    /// Maximum current reached during the session, in A.
    MaxCurrent,
    /// Minimum current used during the session, in A.
    MinCurrent,
    /// Time *not* charging, in hours (default `step_size` 1 second).
    ParkingTime,
    /// Time charging, in hours (default `step_size` 1 second).
    Time,
}

// ── CdrDimension ────────────────────────────────────────────────────────────

/// A single measured dimension within a [`ChargingPeriod`].
///
/// Spec: OCPI 2.1.1 — *CDRs* module, `CdrDimension` class (§10.4.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CdrDimension {
    /// The dimension type. Wire name is `type` (Rust keyword conflict).
    #[serde(rename = "type")]
    pub dimension_type: CdrDimensionType,
    /// Volume consumed, in the unit implied by `dimension_type`.
    pub volume: f64,
}

// ── ChargingPeriod ──────────────────────────────────────────────────────────

/// A contiguous sub-interval of a session with its dimension measurements.
///
/// ## Delta from 2.2.1
///
/// The 2.1.1 `ChargingPeriod` has **only** `start_date_time` and `dimensions`
/// (§10.4.4). The `tariff_id` field is a 2.2 addition and is absent here.
///
/// Spec: OCPI 2.1.1 — *CDRs* module, `ChargingPeriod` class (§10.4.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChargingPeriod {
    /// Start of this period. It ends when the next period starts (or the
    /// session ends, for the last period).
    pub start_date_time: DateTime<Utc>,
    /// Relevant dimension measurements for this period (at least one).
    pub dimensions: Vec<CdrDimension>,
}

// ── SessionStatus ───────────────────────────────────────────────────────────

/// State of a charging session.
///
/// ## Delta from 2.2.1
///
/// 2.1.1 defines `ACTIVE`, `COMPLETED`, `INVALID`, `PENDING` (§9.4.1). The
/// `RESERVATION` status was added in 2.2 and is absent here.
///
/// Spec: OCPI 2.1.1 — *Sessions* module, `SessionStatus` enum (§9.4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SessionStatus {
    /// Accepted and active; the EV is, or can be, charging.
    Active,
    /// Finished successfully; no further modifications will be made.
    Completed,
    /// Declared invalid; will not be billed.
    Invalid,
    /// Pending — not yet started; the initial state. May never become active.
    Pending,
}

// ── Session ─────────────────────────────────────────────────────────────────

/// An OCPI **2.1.1** charging session object, owned and pushed by the CPO.
///
/// ## Deltas from the 2.2.1 [`crate::v2_2_1::Session`]
///
/// - **No `country_code` / `party_id`.**
/// - Wire timestamps are **`start_datetime` / `end_datetime`** (one word) —
///   the 2.1.1 quirk corrected to `start_date_time` / `end_date_time` in 2.2.
/// - Authorization is **`auth_id: CiString36`**, not a `cdr_token: CdrToken`.
/// - **`location`** is the full embedded [`Location`] object, not a
///   `location_id` + `evse_uid` + `connector_id` reference triple.
/// - **`total_cost`** is a bare `number` (`Option<f64>`), not a `Price` object.
/// - **No** `authorization_reference`, `connector_id`, or charging-preferences
///   fields (all 2.2+).
///
/// Spec: OCPI 2.1.1 — *Sessions* module, *Session Object* (§9.3.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    /// Unique session ID within the CPO's platform (max 36).
    pub id: CiString36,
    /// Timestamp when the session became active.
    ///
    /// Wire field is **`start_datetime`** (a 2.1.1-only spelling).
    #[serde(rename = "start_datetime")]
    pub start_datetime: DateTime<Utc>,
    /// Timestamp when the session was completed. `None` while still running.
    ///
    /// Wire field is **`end_datetime`** (a 2.1.1-only spelling).
    #[serde(
        rename = "end_datetime",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub end_datetime: Option<DateTime<Utc>>,
    /// How many kWh have been charged.
    pub kwh: f64,
    /// Reference to the token that started the session — the `auth_id` field of
    /// the [`crate::v2_1_1::Token`].
    pub auth_id: CiString36,
    /// Method used for authentication.
    pub auth_method: AuthMethod,
    /// The location where this session took place, including only the relevant
    /// EVSE and connector.
    pub location: Location,
    /// Optional identification of the kWh meter.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub meter_id: Option<String>,
    /// ISO 4217 currency code (max 3).
    pub currency: String,
    /// Optional charging periods used to calculate and verify the total cost.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub charging_periods: Vec<ChargingPeriod>,
    /// Total cost (excluding VAT) of the session, in `currency`. `None` when no
    /// price information is given (which does *not* imply free of charge).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub total_cost: Option<f64>,
    /// Current status of the session.
    pub status: SessionStatus,
    /// Timestamp when this session was last updated (or created).
    pub last_updated: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v2_1_1::LocationType;

    #[test]
    fn auth_method_serde_roundtrip() {
        for (m, wire) in [
            (AuthMethod::AuthRequest, "\"AUTH_REQUEST\""),
            (AuthMethod::Whitelist, "\"WHITELIST\""),
        ] {
            assert_eq!(serde_json::to_string(&m).unwrap(), wire);
            assert_eq!(serde_json::from_str::<AuthMethod>(wire).unwrap(), m);
        }
    }

    #[test]
    fn auth_method_rejects_2_2_only_command() {
        // `COMMAND` is a 2.2 addition and must not parse as 2.1.1.
        assert!(serde_json::from_str::<AuthMethod>("\"COMMAND\"").is_err());
    }

    #[test]
    fn cdr_dimension_type_serde_roundtrip() {
        for (ty, wire) in [
            (CdrDimensionType::Energy, "\"ENERGY\""),
            (CdrDimensionType::Flat, "\"FLAT\""),
            (CdrDimensionType::MaxCurrent, "\"MAX_CURRENT\""),
            (CdrDimensionType::MinCurrent, "\"MIN_CURRENT\""),
            (CdrDimensionType::ParkingTime, "\"PARKING_TIME\""),
            (CdrDimensionType::Time, "\"TIME\""),
        ] {
            assert_eq!(serde_json::to_string(&ty).unwrap(), wire);
            assert_eq!(serde_json::from_str::<CdrDimensionType>(wire).unwrap(), ty);
        }
    }

    #[test]
    fn cdr_dimension_type_rejects_later_additions() {
        // Dimensions introduced in 2.2+ must not parse as 2.1.1.
        for absent in [
            "\"CURRENT\"",
            "\"POWER\"",
            "\"STATE_OF_CHARGE\"",
            "\"ENERGY_IMPORT\"",
            "\"ENERGY_EXPORT\"",
            "\"MAX_POWER\"",
            "\"RESERVATION_TIME\"",
        ] {
            assert!(
                serde_json::from_str::<CdrDimensionType>(absent).is_err(),
                "{absent} must not parse as a 2.1.1 CdrDimensionType"
            );
        }
    }

    #[test]
    fn session_status_serde_roundtrip_and_rejects_reservation() {
        for (s, wire) in [
            (SessionStatus::Active, "\"ACTIVE\""),
            (SessionStatus::Completed, "\"COMPLETED\""),
            (SessionStatus::Invalid, "\"INVALID\""),
            (SessionStatus::Pending, "\"PENDING\""),
        ] {
            assert_eq!(serde_json::to_string(&s).unwrap(), wire);
            assert_eq!(serde_json::from_str::<SessionStatus>(wire).unwrap(), s);
        }
        // `RESERVATION` is a 2.2 addition.
        assert!(serde_json::from_str::<SessionStatus>("\"RESERVATION\"").is_err());
    }

    #[test]
    fn charging_period_has_no_tariff_id() {
        let period = ChargingPeriod {
            start_date_time: "2015-06-29T22:39:09Z".parse().unwrap(),
            dimensions: vec![CdrDimension {
                dimension_type: CdrDimensionType::Energy,
                volume: 120.0,
            }],
        };
        let json = serde_json::to_string(&period).unwrap();
        assert!(
            !json.contains("tariff_id"),
            "2.1.1 ChargingPeriod must not carry `tariff_id`: {json}"
        );
        let back: ChargingPeriod = serde_json::from_str(&json).unwrap();
        assert_eq!(back, period);
    }

    /// Minimal but valid embedded 2.1.1 [`Location`] for Session fixtures.
    fn sample_location() -> Location {
        Location {
            id: "LOC1".try_into().unwrap(),
            location_type: LocationType::OnStreet,
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
            last_updated: "2015-06-29T20:39:09Z".parse().unwrap(),
        }
    }

    #[test]
    fn session_serde_spec_example() {
        // Ported from the OCPI 2.1.1 "Sessions" object example (§9.3.1): note
        // `start_datetime` (one word), `auth_id` (not cdr_token), an embedded
        // full `location` object, and a bare numeric `total_cost`. No
        // `country_code` / `party_id`.
        let json = r#"{
            "id": "101",
            "start_datetime": "2015-06-29T22:39:09Z",
            "kwh": 0.0,
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
                "last_updated": "2015-06-29T20:39:09Z"
            },
            "currency": "EUR",
            "total_cost": 2.5,
            "status": "PENDING",
            "last_updated": "2015-06-29T22:39:09Z"
        }"#;
        let session: Session = serde_json::from_str(json).unwrap();
        assert_eq!(session.id.as_str(), "101");
        assert_eq!(session.auth_id.as_str(), "DE8ACC12E46L89");
        assert_eq!(session.auth_method, AuthMethod::Whitelist);
        assert_eq!(session.status, SessionStatus::Pending);
        assert_eq!(session.total_cost, Some(2.5));
        assert!(session.end_datetime.is_none());
        assert_eq!(session.location.id.as_str(), "LOC1");

        let back: Session =
            serde_json::from_str(&serde_json::to_string(&session).unwrap()).unwrap();
        assert_eq!(back, session);
    }

    #[test]
    fn session_wire_uses_start_datetime_and_omits_2_2_fields() {
        let session = Session {
            id: "101".try_into().unwrap(),
            start_datetime: "2015-06-29T22:39:09Z".parse().unwrap(),
            end_datetime: Some("2015-06-29T23:50:16Z".parse().unwrap()),
            kwh: 12.5,
            auth_id: "DE8ACC12E46L89".try_into().unwrap(),
            auth_method: AuthMethod::AuthRequest,
            location: sample_location(),
            meter_id: Some("METER-1".to_string()),
            currency: "EUR".to_string(),
            charging_periods: vec![ChargingPeriod {
                start_date_time: "2015-06-29T22:39:09Z".parse().unwrap(),
                dimensions: vec![CdrDimension {
                    dimension_type: CdrDimensionType::Time,
                    volume: 1.973,
                }],
            }],
            total_cost: Some(4.0),
            status: SessionStatus::Completed,
            last_updated: "2015-06-29T23:50:17Z".parse().unwrap(),
        };
        let json = serde_json::to_string(&session).unwrap();

        // `total_cost` serializes as a JSON number, not a Price object.
        assert!(json.contains("\"total_cost\":4.0"), "{json}");

        // Inspect the *top-level* Session keys precisely — a substring scan
        // would false-positive on the embedded `ChargingPeriod.start_date_time`.
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let keys = value.as_object().unwrap();

        // 2.1.1 timestamp spelling (one word).
        assert!(keys.contains_key("start_datetime"), "{json}");
        assert!(keys.contains_key("end_datetime"), "{json}");

        // None of the 2.2+ / corrected fields may appear at the Session level.
        for absent in [
            "start_date_time",
            "end_date_time",
            "country_code",
            "party_id",
            "cdr_token",
            "authorization_reference",
            "connector_id",
            "evse_uid",
            "location_id",
            "charging_preferences",
        ] {
            assert!(
                !keys.contains_key(absent),
                "2.1.1 Session must not carry top-level `{absent}`: {json}"
            );
        }

        let back: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(back, session);
    }
}

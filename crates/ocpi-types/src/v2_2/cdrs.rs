//! OCPI **2.2** CDRs module — the wire-delta overrides over 2.2.1.
//!
//! Only the three CDR types that genuinely differ on the 2.2 wire live here;
//! everything else the module needs (`AuthMethod`, `ChargingPeriod`,
//! `SignedData`, `SignedValue`, `CdrDimension*`) is wire-identical to 2.2.1 and
//! re-exported unchanged by [`super`].
//!
//! The deltas, per `specs/ocpi/2.2.1/version_history.asciidoc` (read backwards —
//! these are the 2.2.1 additions that 2.2 does **not** have) and the 2.2 module
//! spec `specs/ocpi/2.2/mod_cdrs.asciidoc`:
//!
//! - [`CdrToken`] — no `country_code` / `party_id` (added in 2.2.1).
//! - [`CdrLocation`] — `postal_code` is **required** and there is no `state`
//!   field (2.2.1 made `postal_code` optional and added `state`).
//! - [`Cdr`] — no `home_charging_compensation` (added in 2.2.1).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::common::{CiString36, CiString39, CiString48, GeoLocation, Price};

// Shared, wire-identical types are pulled from the `v2_2` re-export surface so
// that when the Commands/Locations slices land (#153) and override
// `ConnectorType` / `ConnectorFormat` / `PowerType`, `CdrLocation` picks up the
// 2.2 shapes automatically without another edit here.
use super::{
    AuthMethod, ChargingPeriod, ConnectorFormat, ConnectorType, PowerType, SignedData, Tariff,
    TokenType,
};

// ── CdrToken ──────────────────────────────────────────────────────────────────

/// Compact token record embedded in a Session or CDR (OCPI 2.2 shape).
///
/// The 2.2 `CdrToken` identifies the token by `uid` + `type` + `contract_id`
/// only. OCPI 2.2.1 added `country_code` / `party_id` to disambiguate the owning
/// eMSP across a hub; those fields do **not** exist on the 2.2 wire.
///
/// Spec: `specs/ocpi/2.2/mod_cdrs.asciidoc` — CdrToken class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CdrToken {
    /// Unique identifier of the token (e.g. the RFID card UID).
    pub uid: CiString36,
    /// Type of the token.
    #[serde(rename = "type")]
    pub token_type: TokenType,
    /// Contract ID (eMA ID) of the EV driver within the eMSP's platform.
    pub contract_id: CiString36,
}

// ── CdrLocation ───────────────────────────────────────────────────────────────

/// Compact location snapshot embedded in a CDR (OCPI 2.2 shape).
///
/// Two deltas from 2.2.1: `postal_code` is **required** here (2.2.1 relaxed it
/// to optional for highway locations), and there is no `state` field (2.2.1
/// added one).
///
/// Spec: `specs/ocpi/2.2/mod_cdrs.asciidoc` — CdrLocation class.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CdrLocation {
    /// Unique location ID within the CPO's platform.
    pub id: CiString36,
    /// Human-readable display name of the location.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub name: Option<String>,
    /// Street/block name and house number.
    pub address: String,
    /// City or town.
    pub city: String,
    /// Postal code of the location (required in 2.2).
    pub postal_code: String,
    /// ISO 3166-1 alpha-3 country code.
    pub country: String,
    /// GPS coordinates of the location.
    pub coordinates: GeoLocation,
    /// Technical EVSE identifier within the CPO's platform.
    pub evse_uid: CiString36,
    /// eMI3-compliant human-readable EVSE ID (up to 48 chars).
    pub evse_id: CiString48,
    /// Connector identifier within the EVSE.
    pub connector_id: CiString36,
    /// Physical connector standard.
    pub connector_standard: ConnectorType,
    /// Connector format (socket or attached cable).
    pub connector_format: ConnectorFormat,
    /// Electrical power type.
    pub connector_power_type: PowerType,
}

// ── Cdr ───────────────────────────────────────────────────────────────────────

/// A Charge Detail Record — the final, billable record of a charging session
/// (OCPI 2.2 shape).
///
/// Identical to 2.2.1 except it omits `home_charging_compensation` (an optional
/// field 2.2.1 added). The embedded `cdr_token` / `cdr_location` are the 2.2
/// shapes above.
///
/// Spec: `specs/ocpi/2.2/mod_cdrs.asciidoc` — CDR object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cdr {
    /// ISO 3166-1 alpha-2 country code of the CPO that owns this CDR.
    pub country_code: crate::common::CiString2,
    /// Party ID of the CPO that owns this CDR.
    pub party_id: crate::common::CiString3,
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
    /// Tariffs applicable to this session (may be empty).
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
    /// Timestamp of the last update (or creation) of this CDR object.
    pub last_updated: DateTime<Utc>,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{Cdr, CdrLocation, CdrToken};

    #[test]
    fn cdr_token_2_2_has_no_country_code_or_party_id() {
        // A faithful 2.2 CdrToken: uid + type + contract_id only.
        let json = r#"{
            "uid": "12345678905880",
            "type": "RFID",
            "contract_id": "NL-TST-C12345678-S"
        }"#;
        let token: CdrToken = serde_json::from_str(json).unwrap();
        assert_eq!(token.uid.as_str(), "12345678905880");
        assert_eq!(token.contract_id.as_str(), "NL-TST-C12345678-S");

        // Negative: the 2.2.1-only routing fields must not appear on the wire.
        let out = serde_json::to_string(&token).unwrap();
        assert!(
            !out.contains("country_code") && !out.contains("party_id"),
            "2.2 CdrToken must not emit 2.2.1's country_code/party_id: {out}"
        );

        let back: CdrToken = serde_json::from_str(&out).unwrap();
        assert_eq!(back, token);
    }

    #[test]
    fn cdr_location_2_2_requires_postal_code_and_has_no_state() {
        let loc = CdrLocation {
            id: "LOC1".try_into().unwrap(),
            name: Some("Gent Zuid".to_string()),
            address: "F.Rooseveltlaan 3A".to_string(),
            city: "Gent".to_string(),
            postal_code: "9000".to_string(),
            country: "BEL".to_string(),
            coordinates: crate::common::GeoLocation {
                latitude: "3.729944".to_string(),
                longitude: "51.047599".to_string(),
            },
            evse_uid: "3256".try_into().unwrap(),
            evse_id: "BE*BEC*E041503001".try_into().unwrap(),
            connector_id: "1".try_into().unwrap(),
            connector_standard: crate::v2_2_1::ConnectorType::Iec62196T2,
            connector_format: crate::v2_2_1::ConnectorFormat::Socket,
            connector_power_type: crate::v2_2_1::PowerType::Ac3Phase,
        };

        let out = serde_json::to_string(&loc).unwrap();
        // Negative: 2.2.1 added `state`; it must not appear on the 2.2 wire.
        assert!(
            !out.contains("\"state\""),
            "2.2 CdrLocation must not emit 2.2.1's `state`: {out}"
        );
        // `postal_code` is required (card. 1), so it is always serialized.
        assert!(out.contains("\"postal_code\":\"9000\""), "{out}");

        let back: CdrLocation = serde_json::from_str(&out).unwrap();
        assert_eq!(back, loc);
    }

    #[test]
    fn cdr_location_2_2_rejects_missing_postal_code() {
        // `postal_code` is mandatory in 2.2 — an object without it must fail to
        // deserialize (unlike 2.2.1, where it is optional).
        let json = r#"{
            "id": "LOC1",
            "address": "F.Rooseveltlaan 3A",
            "city": "Gent",
            "country": "BEL",
            "coordinates": { "latitude": "3.72", "longitude": "51.04" },
            "evse_uid": "3256",
            "evse_id": "BE*BEC*E041503001",
            "connector_id": "1",
            "connector_standard": "IEC_62196_T2",
            "connector_format": "SOCKET",
            "connector_power_type": "AC_3_PHASE"
        }"#;
        let err = serde_json::from_str::<CdrLocation>(json).unwrap_err();
        assert!(
            err.to_string().contains("postal_code"),
            "missing postal_code should be rejected: {err}"
        );
    }

    #[test]
    fn cdr_2_2_omits_home_charging_compensation() {
        let json = r#"{
            "country_code": "BE",
            "party_id": "BEC",
            "id": "12345",
            "start_date_time": "2015-06-29T21:39:09Z",
            "end_date_time": "2015-06-29T23:37:32Z",
            "cdr_token": {
                "uid": "012345678",
                "type": "RFID",
                "contract_id": "DE8ACC12E46L89"
            },
            "auth_method": "WHITELIST",
            "cdr_location": {
                "id": "LOC1",
                "address": "F.Rooseveltlaan 3A",
                "city": "Gent",
                "postal_code": "9000",
                "country": "BEL",
                "coordinates": { "latitude": "3.729944", "longitude": "51.047599" },
                "evse_uid": "3256",
                "evse_id": "BE*BEC*E041503001",
                "connector_id": "1",
                "connector_standard": "IEC_62196_T2",
                "connector_format": "SOCKET",
                "connector_power_type": "AC_3_PHASE"
            },
            "currency": "EUR",
            "charging_periods": [{
                "start_date_time": "2015-06-29T21:39:09Z",
                "dimensions": [{ "type": "ENERGY", "volume": 120 }]
            }],
            "total_cost": { "excl_vat": 4.00 },
            "total_energy": 15.0,
            "total_time": 1.973,
            "last_updated": "2015-06-29T23:37:32Z"
        }"#;
        let cdr: Cdr = serde_json::from_str(json).unwrap();
        assert_eq!(cdr.id.as_str(), "12345");
        assert_eq!(cdr.cdr_location.postal_code, "9000");
        assert_eq!(cdr.cdr_token.uid.as_str(), "012345678");

        // Round-trip, and prove the 2.2.1-only field never surfaces.
        let out = serde_json::to_string(&cdr).unwrap();
        assert!(
            !out.contains("home_charging_compensation"),
            "2.2 Cdr must not emit 2.2.1's home_charging_compensation: {out}"
        );
        let back: Cdr = serde_json::from_str(&out).unwrap();
        assert_eq!(back, cdr);
    }
}

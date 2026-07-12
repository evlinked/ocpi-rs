//! OCPI **2.3.0** Locations module — the wire-delta overrides over 2.2.1
//! (milestone **M8**, slice 1 of #177).
//!
//! OCPI 2.3.0 extends the 2.2.1 Locations objects additively. Per
//! `specs/ocpi/2.3.0/mod_locations.asciidoc` + `changelog.asciidoc`, the deltas
//! are:
//!
//! - a new [`Parking`] object linked to the EVSE (with [`VehicleType`]s), listed
//!   on the [`Location`] via `parking_places`;
//! - a support telephone number on the [`Location`] (`help_phone`);
//! - references to those parking spaces on the `EVSE` (`parking`) plus an
//!   `accepted_service_providers` list;
//! - ISO 15118 Plug-and-Charge compatibility flags on the `Connector`
//!   (`capabilities`).
//!
//! **This module is slice 1**: it defines the new [`Parking`] object, the
//! [`VehicleType`] / [`ParkingDirection`] enums it needs, and the [`Location`]
//! fork carrying `parking_places` + `help_phone`. The `EVSE`- and
//! `Connector`-level deltas fork those composites in a follow-up; until then a
//! `v2_3_0::Location` embeds the **2.2.1** [`crate::v2_2_1::Evse`] unchanged
//! (its own `parking`/`accepted_service_providers` and the connector 15118
//! flags land in slice 2). Every other Locations type stays a plain re-export.
//!
//! ## Why a fork of `Location` (not a mutation of the 2.2.1 struct)
//!
//! Keeping the 2.3.0 additions on a `v2_3_0`-local [`Location`] means a 2.2.1
//! peer still round-trips the exact 2.2.1 shape — `parking_places` / `help_phone`
//! cannot leak onto a 2.2.1 connection — while a 2.3.0 peer's parking-reporting
//! data (needed for EU AFIR / National Access Point compliance) is parsed and
//! held faithfully rather than dropped on the floor.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::common::{
    BusinessDetails, CiString2, CiString25, CiString3, CiString36, DisplayText, EnergyMix,
    GeoLocation, Image,
};
use crate::v2_2_1::{AdditionalGeoLocation, Evse, Facility, Hours, ParkingType, PublishTokenType};

// ── VehicleType ───────────────────────────────────────────────────────────────

/// A categorization of vehicles indicating which can use a certain EVSE
/// (OCPI 2.3.0).
///
/// Modelled as a strict enum: a value outside this set is rejected on
/// deserialize rather than silently accepted (the forward-compat policy for
/// unknown OpenEnum values is tracked separately in #184).
///
/// Spec: `specs/ocpi/2.3.0/mod_locations.asciidoc` — VehicleType enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VehicleType {
    /// A motorcycle (UNECE L).
    Motorcycle,
    /// A personal vehicle / passenger car (UNECE M1).
    PersonalVehicle,
    /// A personal vehicle with a trailer attached (UNECE M1 + O).
    PersonalVehicleWithTrailer,
    /// A light-duty van under 275 cm tall (UNECE N1).
    Van,
    /// A heavy-duty tractor unit without a trailer (UNECE T).
    SemiTractor,
    /// A heavy-duty truck without an articulation point (UNECE N2/N3).
    Rigid,
    /// A heavy-duty truck (tractor or rigid) with a trailer (UNECE N2/N3 + O).
    TruckWithTrailer,
    /// A bus or motor coach (UNECE M2/M3).
    Bus,
    /// A vehicle with a disabled-parking permit.
    Disabled,
}

// ── ParkingDirection ──────────────────────────────────────────────────────────

/// The direction in which parking occurs relative to the approach roadway
/// (OCPI 2.3.0).
///
/// Spec: `specs/ocpi/2.3.0/mod_locations.asciidoc` — ParkingDirection enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ParkingDirection {
    /// Parking parallel to the approach roadway.
    Parallel,
    /// Parking perpendicular to the approach roadway.
    Perpendicular,
    /// Parking at an angle to the approach roadway (echelon parking).
    Angle,
}

// ── Parking ───────────────────────────────────────────────────────────────────

/// A parking space a vehicle can occupy while charging (OCPI 2.3.0, new).
///
/// Parking objects let EU CPOs report parking-spot count and properties to
/// National Access Points as required by the EU Alternative Fuel Infrastructure
/// Regulation (AFIR). Receivers that are not NAPs may ignore them. For EVSEs
/// without delineated spaces (e.g. streetside), a `Parking` may describe the
/// limitations that apply near the EVSE without describing a specific space.
///
/// Spec: `specs/ocpi/2.3.0/mod_locations.asciidoc` — Parking object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parking {
    /// Identifier of the parking space, unique among all `Parking` in the
    /// enclosing [`Location`].
    pub id: CiString36,
    /// A visible on-site identifier for the parking place (e.g. painted number).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub physical_reference: Option<String>,
    /// The vehicle types the parking is designed to accommodate (at least one).
    pub vehicle_types: Vec<VehicleType>,
    /// Maximum vehicle weight, in kilograms.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max_vehicle_weight: Option<f64>,
    /// Maximum vehicle height, in centimetres.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max_vehicle_height: Option<f64>,
    /// Maximum vehicle length, in centimetres.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max_vehicle_length: Option<f64>,
    /// Maximum vehicle width, in centimetres.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max_vehicle_width: Option<f64>,
    /// Length of the parking space, in centimetres.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub parking_space_length: Option<f64>,
    /// Width of the parking space, in centimetres.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub parking_space_width: Option<f64>,
    /// Whether vehicles carrying dangerous goods may park here.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub dangerous_goods_allowed: Option<bool>,
    /// Direction in which the vehicle is to be parked next to the EVSE.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub direction: Option<ParkingDirection>,
    /// Whether a vehicle can charge and proceed without reversing in or out.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub drive_through: Option<bool>,
    /// Whether vehicles of a type not listed in `vehicle_types` are forbidden
    /// from parking here even if they physically fit.
    pub restricted_to_type: bool,
    /// Whether a reservation is required to park here.
    pub reservation_required: bool,
    /// Maximum permitted parking duration, in minutes.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub time_limit: Option<f64>,
    /// Whether the vehicle is parked under a roof while charging.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub roofed: Option<bool>,
    /// Photos of the parking space.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<Image>,
    /// Whether the space is lit by artificial lighting.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub lighting: Option<bool>,
    /// Whether a power outlet is available for a truck's refrigeration load.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub refrigeration_outlet: Option<bool>,
    /// Standards the parking space conforms to (e.g. PAS 1899 for disabled
    /// parking).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub standards: Vec<CiString36>,
    /// Reference to an Alliance for Parking Data Standards (APDS) element
    /// describing this parking.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub apds_reference: Option<String>,
}

// ── Location ──────────────────────────────────────────────────────────────────

/// A charging location containing one or more EVSEs (OCPI 2.3.0).
///
/// Structurally the 2.2.1 [`crate::v2_2_1::Location`] plus the two 2.3.0
/// additions: [`Location::parking_places`] (the AFIR/NAP parking report) and
/// [`Location::help_phone`] (a Driver support number). The `evses` remain the
/// 2.2.1 [`Evse`] in this slice; the EVSE/Connector 2.3.0 deltas land in the
/// follow-up.
///
/// Spec: `specs/ocpi/2.3.0/mod_locations.asciidoc` — Location object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Location {
    /// ISO 3166-1 alpha-2 country code of the CPO that owns this location.
    pub country_code: CiString2,
    /// eMI3 party identifier of the CPO (3 chars).
    pub party_id: CiString3,
    /// Location identifier, unique within the CPO's platform.
    pub id: CiString36,
    /// Whether this location may be published publicly.
    pub publish: bool,
    /// Token filter list (only used when `publish = false`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub publish_allowed_to: Vec<PublishTokenType>,
    /// Display name of the location.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub name: Option<String>,
    /// Street/block name and house number.
    pub address: String,
    /// City or town.
    pub city: String,
    /// Postal code (may be absent at some highway locations).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub postal_code: Option<String>,
    /// State or province (only when relevant).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub state: Option<String>,
    /// ISO 3166-1 alpha-3 country code (e.g. `"NLD"`).
    pub country: String,
    /// Coordinates of the location.
    pub coordinates: GeoLocation,
    /// Related geographic points (e.g. parking entrance).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_locations: Vec<AdditionalGeoLocation>,
    /// General parking type at the charge point location.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub parking_type: Option<ParkingType>,
    /// EVSEs at this location.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evses: Vec<Evse>,
    /// Parking places usable by vehicles charging at this location
    /// (OCPI 2.3.0 addition — AFIR/NAP reporting).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parking_places: Vec<Parking>,
    /// Human-readable directions to reach the location.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub directions: Vec<DisplayText>,
    /// Operator details (if absent, use credentials module data).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub operator: Option<BusinessDetails>,
    /// Sub-operator details.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub suboperator: Option<BusinessDetails>,
    /// Owner details.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub owner: Option<BusinessDetails>,
    /// Facilities this location belongs to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub facilities: Vec<Facility>,
    /// IANA timezone string (e.g. `"Europe/Amsterdam"`).
    pub time_zone: String,
    /// Opening hours of the location.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub opening_times: Option<Hours>,
    /// Whether EVSEs keep charging when the location is closed. Default: `true`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub charging_when_closed: Option<bool>,
    /// Images (photos, logos) for this location.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<Image>,
    /// Energy mix details for this location.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub energy_mix: Option<EnergyMix>,
    /// A telephone number a Driver may call for assistance
    /// (OCPI 2.3.0 addition).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub help_phone: Option<CiString25>,
    /// Last update timestamp (UTC).
    pub last_updated: DateTime<Utc>,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vehicle_type_round_trips_screaming_snake() {
        for (variant, wire) in [
            (VehicleType::Motorcycle, "\"MOTORCYCLE\""),
            (VehicleType::PersonalVehicle, "\"PERSONAL_VEHICLE\""),
            (
                VehicleType::PersonalVehicleWithTrailer,
                "\"PERSONAL_VEHICLE_WITH_TRAILER\"",
            ),
            (VehicleType::Van, "\"VAN\""),
            (VehicleType::SemiTractor, "\"SEMI_TRACTOR\""),
            (VehicleType::Rigid, "\"RIGID\""),
            (VehicleType::TruckWithTrailer, "\"TRUCK_WITH_TRAILER\""),
            (VehicleType::Bus, "\"BUS\""),
            (VehicleType::Disabled, "\"DISABLED\""),
        ] {
            assert_eq!(serde_json::to_string(&variant).unwrap(), wire);
            assert_eq!(serde_json::from_str::<VehicleType>(wire).unwrap(), variant);
        }
    }

    #[test]
    fn parking_direction_round_trips_screaming_snake() {
        for (variant, wire) in [
            (ParkingDirection::Parallel, "\"PARALLEL\""),
            (ParkingDirection::Perpendicular, "\"PERPENDICULAR\""),
            (ParkingDirection::Angle, "\"ANGLE\""),
        ] {
            assert_eq!(serde_json::to_string(&variant).unwrap(), wire);
            assert_eq!(
                serde_json::from_str::<ParkingDirection>(wire).unwrap(),
                variant
            );
        }
    }

    #[test]
    fn unknown_vehicle_type_is_rejected_on_deserialize() {
        // The unsupported case is rejected explicitly, never silently dropped.
        assert!(serde_json::from_str::<VehicleType>("\"SPACESHIP\"").is_err());
    }

    #[test]
    fn minimal_parking_round_trips_and_omits_absent_fields() {
        // The two required booleans + the required non-empty vehicle_types are
        // the minimal shape; nothing optional is emitted.
        let json = r#"{
            "id": "p1",
            "vehicle_types": ["PERSONAL_VEHICLE"],
            "restricted_to_type": false,
            "reservation_required": false
        }"#;
        let parking: Parking = serde_json::from_str(json).unwrap();
        assert_eq!(parking.vehicle_types, vec![VehicleType::PersonalVehicle]);
        assert!(!parking.restricted_to_type);
        assert!(parking.direction.is_none());

        let out = serde_json::to_string(&parking).unwrap();
        assert!(!out.contains("physical_reference"));
        assert!(!out.contains("max_vehicle_weight"));
        assert!(!out.contains("images"));
        let back: Parking = serde_json::from_str(&out).unwrap();
        assert_eq!(back, parking);
    }

    #[test]
    fn full_truck_parking_round_trips() {
        // A heavy-goods parking space with the full AFIR-relevant property set.
        let json = r#"{
            "id": "truck-bay-3",
            "physical_reference": "T3",
            "vehicle_types": ["RIGID", "TRUCK_WITH_TRAILER"],
            "max_vehicle_weight": 40000.0,
            "max_vehicle_height": 400.0,
            "max_vehicle_length": 1650.0,
            "max_vehicle_width": 255.0,
            "parking_space_length": 1800.0,
            "parking_space_width": 300.0,
            "dangerous_goods_allowed": false,
            "direction": "PERPENDICULAR",
            "drive_through": true,
            "restricted_to_type": true,
            "reservation_required": true,
            "time_limit": 120.0,
            "roofed": false,
            "lighting": true,
            "refrigeration_outlet": true,
            "standards": ["PAS-1899"],
            "apds_reference": "apds:place:42"
        }"#;
        let parking: Parking = serde_json::from_str(json).unwrap();
        assert_eq!(parking.vehicle_types.len(), 2);
        assert_eq!(parking.direction, Some(ParkingDirection::Perpendicular));
        assert_eq!(parking.drive_through, Some(true));
        assert_eq!(parking.max_vehicle_weight, Some(40000.0));

        let back: Parking =
            serde_json::from_str(&serde_json::to_string(&parking).unwrap()).unwrap();
        assert_eq!(back, parking);
    }

    #[test]
    fn location_with_parking_places_and_help_phone_round_trips() {
        // A 2.3.0 location carrying the two new fields; the parking-report data
        // survives a full serialize → deserialize cycle unmangled.
        let json = r#"{
            "country_code": "NL",
            "party_id": "ABC",
            "id": "LOC1",
            "publish": true,
            "address": "F.Rooseveltlaan 3A",
            "city": "Amsterdam",
            "country": "NLD",
            "coordinates": { "latitude": "52.376364", "longitude": "4.898168" },
            "parking_type": "ON_STREET",
            "parking_places": [
                {
                    "id": "space-1",
                    "vehicle_types": ["PERSONAL_VEHICLE", "DISABLED"],
                    "restricted_to_type": true,
                    "reservation_required": false,
                    "direction": "PARALLEL"
                }
            ],
            "time_zone": "Europe/Amsterdam",
            "help_phone": "+31201234567",
            "last_updated": "2026-07-12T10:00:00Z"
        }"#;
        let loc: Location = serde_json::from_str(json).unwrap();
        assert_eq!(loc.parking_places.len(), 1);
        assert_eq!(
            loc.parking_places[0].vehicle_types,
            vec![VehicleType::PersonalVehicle, VehicleType::Disabled]
        );
        assert_eq!(loc.help_phone.as_ref().unwrap().as_str(), "+31201234567");

        let out = serde_json::to_string(&loc).unwrap();
        assert!(out.contains("\"parking_places\""));
        assert!(out.contains("\"help_phone\""));
        let back: Location = serde_json::from_str(&out).unwrap();
        assert_eq!(back, loc);
    }

    #[test]
    fn location_without_2_3_0_additions_omits_them_on_the_wire() {
        // A location with neither new field must not emit them — so a 2.3.0
        // Location that happens to carry no parking data is wire-compatible with
        // a 2.2.1 peer.
        let loc = Location {
            country_code: CiString2::try_from("NL").unwrap(),
            party_id: CiString3::try_from("ABC").unwrap(),
            id: CiString36::try_from("LOC1").unwrap(),
            publish: true,
            publish_allowed_to: Vec::new(),
            name: None,
            address: "F.Rooseveltlaan 3A".into(),
            city: "Amsterdam".into(),
            postal_code: None,
            state: None,
            country: "NLD".into(),
            coordinates: GeoLocation {
                latitude: "52.376364".into(),
                longitude: "4.898168".into(),
            },
            related_locations: Vec::new(),
            parking_type: None,
            evses: Vec::new(),
            parking_places: Vec::new(),
            directions: Vec::new(),
            operator: None,
            suboperator: None,
            owner: None,
            facilities: Vec::new(),
            time_zone: "Europe/Amsterdam".into(),
            opening_times: None,
            charging_when_closed: None,
            images: Vec::new(),
            energy_mix: None,
            help_phone: None,
            last_updated: "2026-07-12T10:00:00Z".parse().unwrap(),
        };
        let out = serde_json::to_string(&loc).unwrap();
        assert!(!out.contains("parking_places"));
        assert!(!out.contains("help_phone"));
    }

    #[test]
    fn help_phone_over_twenty_five_chars_is_rejected() {
        // help_phone is CiString(25); an over-length value fails at the trust
        // boundary rather than being silently truncated.
        let json = r#"{
            "country_code": "NL",
            "party_id": "ABC",
            "id": "LOC1",
            "publish": true,
            "address": "Street 1",
            "city": "Amsterdam",
            "country": "NLD",
            "coordinates": { "latitude": "52.376364", "longitude": "4.898168" },
            "time_zone": "Europe/Amsterdam",
            "help_phone": "+00000000000000000000000000000",
            "last_updated": "2026-07-12T10:00:00Z"
        }"#;
        assert!(serde_json::from_str::<Location>(json).is_err());
    }
}

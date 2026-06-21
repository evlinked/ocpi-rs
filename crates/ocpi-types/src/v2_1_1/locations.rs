//! OCPI 2.1.1 — Locations module types.
//!
//! The Locations module is the highest-traffic roaming module: a CPO publishes
//! its [`Location`]s, each holding [`Evse`]s, each holding [`Connector`]s.
//!
//! ## Deltas from the 2.2.1 Locations types
//!
//! The 2.1.1 wire shape predates several 2.2 additions, so these types are
//! version-pinned rather than reused from [`crate::v2_2_1`]:
//!
//! - **`Location` has a required `type: LocationType`** — the general
//!   location category. 2.2 dropped this in favour of `parking_type` on the
//!   EVSE; there is no [`LocationType`] in 2.2.1.
//! - **No `country_code` / `party_id` on `Location`** — owner identifiers were
//!   introduced in 2.2. In 2.1.1 ownership is conveyed only via the
//!   `operator` / `suboperator` / `owner` [`BusinessDetails`].
//! - **`Location.postal_code` is required** (it became optional in 2.2.1).
//! - **No `publish` / `publish_allowed_to` / `parking_type`** on `Location`.
//! - **`Connector.tariff_id` is a single optional `String`**, not the 2.2.1
//!   `tariff_ids: Vec<String>`. There is no `max_electric_power`.
//! - Smaller enum sets: [`Capability`] (6 variants), [`PowerType`] (no
//!   two-phase), [`Facility`].
//!
//! Shapes identical across versions are reused from [`crate::common`]
//! ([`GeoLocation`], [`DisplayText`], [`Image`], [`BusinessDetails`],
//! [`crate::common::EnergyMix`]).
//!
//! Spec: OCPI 2.1.1 — *Locations* module (`specs/ocpi/2.1.1/OCPI_2.1.1.pdf`,
//! chapter 8).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::common::{
    BusinessDetails, CiString36, CiString39, CiString48, DisplayText, EnergyMix, GeoLocation,
    Image, Url,
};

// ── LocationType ──────────────────────────────────────────────────────────────

/// The general type of a charge point's [`Location`].
///
/// 2.1.1-only: 2.2 removed `Location.type` in favour of EVSE `parking_type`,
/// so this enum has no 2.2.1 equivalent.
///
/// Spec: OCPI 2.1.1 — *Locations* module, `LocationType` enum (§8.4.17).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LocationType {
    /// Parking in public space.
    OnStreet,
    /// Multistorey car park.
    ParkingGarage,
    /// Multistorey car park, mainly underground.
    UndergroundGarage,
    /// A cleared area intended for parking (supermarket, bar, etc.).
    ParkingLot,
    /// None of the given possibilities.
    Other,
    /// Parking location type is not known by the operator (default).
    Unknown,
}

// ── Capability ────────────────────────────────────────────────────────────────

/// Functionality that an [`Evse`] supports.
///
/// ## Delta from 2.2.1
///
/// 2.1.1 defines exactly six capabilities. The many payment/token additions
/// (`CHIP_CARD_SUPPORT`, `CONTACTLESS_CARD_SUPPORT`, `DEBIT_CARD_PAYABLE`,
/// `PED_TERMINAL`, `TOKEN_GROUP_CAPABLE`, `CHARGING_PREFERENCES_CAPABLE`,
/// `START_SESSION_CONNECTOR_REQUIRED`) all arrived in later versions.
///
/// Spec: OCPI 2.1.1 — *Locations* module, `Capability` enum (§8.4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Capability {
    /// The EVSE supports charging profiles.
    ChargingProfileCapable,
    /// Charging at this EVSE can be paid with a credit card.
    CreditCardPayable,
    /// The EVSE can be remotely started/stopped.
    RemoteStartStopCapable,
    /// The EVSE can be reserved.
    Reservable,
    /// Charging can be authorized with an RFID token.
    RfidReader,
    /// Connectors have a mechanical lock that can be unlocked by the eMSP.
    UnlockCapable,
}

// ── Facility ──────────────────────────────────────────────────────────────────

/// A facility a charge [`Location`] directly belongs to.
///
/// Spec: OCPI 2.1.1 — *Locations* module, `Facility` enum (§8.4.12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Facility {
    /// A hotel.
    Hotel,
    /// A restaurant.
    Restaurant,
    /// A cafe.
    Cafe,
    /// A mall or shopping center.
    Mall,
    /// A supermarket.
    Supermarket,
    /// Sport facilities: gym, field, etc.
    Sport,
    /// A recreation area.
    RecreationArea,
    /// In, or close to, a park or nature reserve.
    Nature,
    /// A museum.
    Museum,
    /// A bus stop.
    BusStop,
    /// A taxi stand.
    TaxiStand,
    /// A train station.
    TrainStation,
    /// An airport.
    Airport,
    /// A carpool parking.
    CarpoolParking,
    /// A fuel station.
    FuelStation,
    /// Wifi or other internet available.
    Wifi,
}

// ── ParkingRestriction ────────────────────────────────────────────────────────

/// A restriction that applies to a parking spot.
///
/// Identical to the 2.2.1 set, but version-pinned here so the 2.1.1 surface is
/// complete.
///
/// Spec: OCPI 2.1.1 — *Locations* module, `ParkingRestriction` enum (§8.4.18).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ParkingRestriction {
    /// Reserved for electric vehicles only.
    EvOnly,
    /// Parking allowed only while plugged in (charging).
    Plugged,
    /// Reserved for disabled people with a valid ID.
    Disabled,
    /// For customers/guests only.
    Customers,
    /// Only suitable for (electric) motorcycles or scooters.
    Motorcycles,
}

// ── Status ────────────────────────────────────────────────────────────────────

/// The status of an [`Evse`] (or a [`Connector`] in a [`StatusSchedule`]).
///
/// Spec: OCPI 2.1.1 — *Locations* module, `Status` enum (§8.4.21).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Status {
    /// Able to start a new charging session.
    Available,
    /// Not accessible because of a physical barrier, e.g. a car.
    Blocked,
    /// In use.
    Charging,
    /// Not yet active, or no longer available (deleted).
    Inoperative,
    /// Currently out of order. Serializes as `OUTOFORDER`.
    Outoforder,
    /// Planned, will be operating soon.
    Planned,
    /// Discontinued/removed.
    Removed,
    /// Reserved for a particular EV driver.
    Reserved,
    /// No status information available (also used when offline).
    Unknown,
}

// ── ConnectorFormat ───────────────────────────────────────────────────────────

/// Whether a [`Connector`] is a socket or an attached cable.
///
/// Spec: OCPI 2.1.1 — *Locations* module, `ConnectorFormat` enum (§8.4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConnectorFormat {
    /// A socket; the EV user needs to bring a fitting plug.
    Socket,
    /// An attached cable; the EV needs a fitting inlet.
    Cable,
}

// ── PowerType ─────────────────────────────────────────────────────────────────

/// Electrical power type at a [`Connector`].
///
/// ## Delta from 2.2.1
///
/// 2.1.1 has only mono-phase AC, three-phase AC, and DC. The `AC_2_PHASE` and
/// `AC_2_PHASE_SPLIT` variants were added in 2.2.1.
///
/// Spec: OCPI 2.1.1 — *Locations* module, `PowerType` enum (§8.4.19).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PowerType {
    /// AC mono phase.
    #[serde(rename = "AC_1_PHASE")]
    Ac1Phase,
    /// AC three phase.
    #[serde(rename = "AC_3_PHASE")]
    Ac3Phase,
    /// Direct current.
    #[serde(rename = "DC")]
    Dc,
}

// ── ConnectorType ─────────────────────────────────────────────────────────────

/// The socket or plug standard of a [`Connector`].
///
/// ## Delta from 2.2.1
///
/// The 2.1.1 set has 26 variants. Later versions add `CHAOJI`, `DOMESTIC_M/N/O`,
/// the `GBT_*` and `NEMA_*` families, and the `PANTOGRAPH_*` variants — none of
/// which are valid in 2.1.1.
///
/// Spec: OCPI 2.1.1 — *Locations* module, `ConnectorType` enum (§8.4.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConnectorType {
    /// CHAdeMO, DC.
    Chademo,
    /// Domestic household type A — NEMA 1-15, 2 pins.
    DomesticA,
    /// Domestic household type B — NEMA 5-15, 3 pins.
    DomesticB,
    /// Domestic household type C — CEE 7/17, 2 pins.
    DomesticC,
    /// Domestic household type D — 3 pin.
    DomesticD,
    /// Domestic household type E — CEE 7/5, 3 pins.
    DomesticE,
    /// Domestic household type F — CEE 7/4 Schuko, 3 pins.
    DomesticF,
    /// Domestic household type G — BS 1363, Commonwealth, 3 pins.
    DomesticG,
    /// Domestic household type H — SI-32, 3 pins.
    DomesticH,
    /// Domestic household type I — AS 3112, 3 pins.
    DomesticI,
    /// Domestic household type J — SEV 1011, 3 pins.
    DomesticJ,
    /// Domestic household type K — DS 60884-2-D1, 3 pins.
    DomesticK,
    /// Domestic household type L — CEI 23-16-VII, 3 pins.
    DomesticL,
    /// IEC 60309-2 Industrial single phase 16 A (usually blue).
    #[serde(rename = "IEC_60309_2_single_16")]
    Iec6030921Single16,
    /// IEC 60309-2 Industrial three phase 16 A (usually red).
    #[serde(rename = "IEC_60309_2_three_16")]
    Iec6030922Three16,
    /// IEC 60309-2 Industrial three phase 32 A (usually red).
    #[serde(rename = "IEC_60309_2_three_32")]
    Iec6030922Three32,
    /// IEC 60309-2 Industrial three phase 64 A (usually red).
    #[serde(rename = "IEC_60309_2_three_64")]
    Iec6030922Three64,
    /// IEC 62196 Type 1 "SAE J1772".
    #[serde(rename = "IEC_62196_T1")]
    Iec62196T1,
    /// Combo Type 1 based, DC.
    #[serde(rename = "IEC_62196_T1_COMBO")]
    Iec62196T1Combo,
    /// IEC 62196 Type 2 "Mennekes".
    #[serde(rename = "IEC_62196_T2")]
    Iec62196T2,
    /// Combo Type 2 based, DC.
    #[serde(rename = "IEC_62196_T2_COMBO")]
    Iec62196T2Combo,
    /// IEC 62196 Type 3A.
    #[serde(rename = "IEC_62196_T3A")]
    Iec62196T3a,
    /// IEC 62196 Type 3C "Scame".
    #[serde(rename = "IEC_62196_T3C")]
    Iec62196T3c,
    /// Tesla Connector "Roadster"-type (round, 4 pin).
    TeslaR,
    /// Tesla Connector "Model-S"-type (oval, 5 pin).
    TeslaS,
}

// ── AdditionalGeoLocation ─────────────────────────────────────────────────────

/// A geographic point related to a [`Location`], with an optional name.
///
/// Spec: OCPI 2.1.1 — *Locations* module, `AdditionalGeoLocation` class
/// (§8.4.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdditionalGeoLocation {
    /// Latitude in decimal degrees (max 10 chars). Same format as
    /// [`GeoLocation::latitude`].
    pub latitude: String,
    /// Longitude in decimal degrees (max 11 chars). Same format as
    /// [`GeoLocation::longitude`].
    pub longitude: String,
    /// Name of the point in local language or as written at the location.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub name: Option<DisplayText>,
}

// ── RegularHours / ExceptionalPeriod / Hours ──────────────────────────────────

/// Regular recurring operation or access hours (weekday-based).
///
/// Spec: OCPI 2.1.1 — *Locations* module, `RegularHours` class (§8.4.20).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegularHours {
    /// Day of week: 1 = Monday … 7 = Sunday.
    pub weekday: u8,
    /// Begin of the period in local time, format `HH:MM` (24h, leading zeros).
    pub period_begin: String,
    /// End of the period in local time; must be later than `period_begin`.
    pub period_end: String,
}

/// One exceptional opening or closing period for a [`Location`].
///
/// Spec: OCPI 2.1.1 — *Locations* module, `ExceptionalPeriod` class (§8.4.11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExceptionalPeriod {
    /// Begin of the exception (UTC).
    pub period_begin: DateTime<Utc>,
    /// End of the exception (UTC).
    pub period_end: DateTime<Utc>,
}

/// Opening and access hours of a [`Location`].
///
/// Either `twentyfourseven` is `true` or `regular_hours` is populated.
///
/// Spec: OCPI 2.1.1 — *Locations* module, `Hours` class (§8.4.14).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hours {
    /// `true` = open 24 hours a day, 7 days a week, except the given exceptions.
    pub twentyfourseven: bool,
    /// Weekday-based regular hours (should not be set when `twentyfourseven`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub regular_hours: Vec<RegularHours>,
    /// Exceptional opening periods (additional to regular hours).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exceptional_openings: Vec<ExceptionalPeriod>,
    /// Exceptional closing periods (override regular hours and openings).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exceptional_closings: Vec<ExceptionalPeriod>,
}

// ── StatusSchedule ────────────────────────────────────────────────────────────

/// A scheduled future [`Status`] period for an [`Evse`].
///
/// Purely informational; when the status actually changes the CPO must push an
/// update to the EVSE's `status` field.
///
/// Spec: OCPI 2.1.1 — *Locations* module, `StatusSchedule` class (§8.4.22).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusSchedule {
    /// Begin of the scheduled period (UTC).
    pub period_begin: DateTime<Utc>,
    /// End of the scheduled period, if known.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub period_end: Option<DateTime<Utc>>,
    /// Status value during the scheduled period.
    pub status: Status,
}

// ── Connector ─────────────────────────────────────────────────────────────────

/// A socket or cable available for the EV to use; belongs to an [`Evse`].
///
/// ## Delta from 2.2.1
///
/// `tariff_id` is a **single optional** identifier in 2.1.1, not the 2.2.1
/// `tariff_ids: Vec<String>`. There is no `max_electric_power`.
///
/// Spec: OCPI 2.1.1 — *Locations* module, `Connector` object (§8.3.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Connector {
    /// Identifier of the connector within the EVSE.
    pub id: CiString36,
    /// The standard of the installed connector.
    pub standard: ConnectorType,
    /// The format (socket/cable) of the installed connector.
    pub format: ConnectorFormat,
    /// Electrical power type.
    pub power_type: PowerType,
    /// Voltage of the connector (line to neutral for AC_3_PHASE), in volt [V].
    pub voltage: i32,
    /// Maximum amperage of the connector, in ampere [A].
    pub amperage: i32,
    /// Identifier of the current charging tariff (single, optional in 2.1.1).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tariff_id: Option<CiString36>,
    /// URL to the operator's terms and conditions.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub terms_and_conditions: Option<Url>,
    /// Timestamp when this connector was last updated (or created).
    pub last_updated: DateTime<Utc>,
}

// ── Evse ──────────────────────────────────────────────────────────────────────

/// The part that controls power supply to a single EV in a single session;
/// belongs to a [`Location`].
///
/// Spec: OCPI 2.1.1 — *Locations* module, `EVSE` object (§8.3.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evse {
    /// Uniquely identifies the EVSE within the CPO's platform (technical ID).
    pub uid: CiString39,
    /// eMI3-compliant human-readable EVSE ID (up to 48 chars). Optional because
    /// it can be removed when an EVSE is `REMOVED` so the ID may be re-used.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub evse_id: Option<CiString48>,
    /// Current status of the EVSE.
    pub status: Status,
    /// Planned future statuses of the EVSE.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub status_schedule: Vec<StatusSchedule>,
    /// Functionalities the EVSE is capable of.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<Capability>,
    /// Available connectors on the EVSE (at least one).
    pub connectors: Vec<Connector>,
    /// Level on which the charging station is located.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub floor_level: Option<String>,
    /// Coordinates of the EVSE.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub coordinates: Option<GeoLocation>,
    /// A number/string printed on the outside of the EVSE for identification.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub physical_reference: Option<String>,
    /// Multi-language directions on how to reach the EVSE from the location.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub directions: Vec<DisplayText>,
    /// Restrictions that apply to the parking spot.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parking_restrictions: Vec<ParkingRestriction>,
    /// Links to images related to the EVSE.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<Image>,
    /// Timestamp when this EVSE or one of its connectors was last updated.
    pub last_updated: DateTime<Utc>,
}

// ── Location ──────────────────────────────────────────────────────────────────

/// An OCPI **2.1.1** Location — where a group of [`Evse`]s is installed.
///
/// ## Deltas from the 2.2.1 [`crate::v2_2_1::Location`]
///
/// - **Required `type: LocationType`** (removed in 2.2 in favour of EVSE
///   `parking_type`).
/// - **No** `country_code` / `party_id` (owner identifiers added in 2.2).
/// - **`postal_code` is required** (became optional in 2.2.1).
/// - **No** `publish` / `publish_allowed_to` / `parking_type`.
///
/// Spec: OCPI 2.1.1 — *Locations* module, `Location` object (§8.3.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Location {
    /// Uniquely identifies the location within the CPO's platform (max 39).
    pub id: CiString39,
    /// The general type of the charge point location.
    #[serde(rename = "type")]
    pub location_type: LocationType,
    /// Display name of the location.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub name: Option<String>,
    /// Street/block name and house number if available.
    pub address: String,
    /// City or town.
    pub city: String,
    /// Postal code of the location.
    pub postal_code: String,
    /// ISO 3166-1 alpha-3 code for the country of this location.
    pub country: String,
    /// Coordinates of the location.
    pub coordinates: GeoLocation,
    /// Geographical locations of related points relevant to the user.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_locations: Vec<AdditionalGeoLocation>,
    /// EVSEs that belong to this location.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evses: Vec<Evse>,
    /// Human-readable directions on how to reach the location.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub directions: Vec<DisplayText>,
    /// Information about the operator.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub operator: Option<BusinessDetails>,
    /// Information about the suboperator, if available.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub suboperator: Option<BusinessDetails>,
    /// Information about the owner, if available.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub owner: Option<BusinessDetails>,
    /// Facilities this charge location directly belongs to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub facilities: Vec<Facility>,
    /// IANA tzdata TZ value, e.g. `"Europe/Oslo"`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub time_zone: Option<String>,
    /// Times when the EVSEs can be accessed for charging.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub opening_times: Option<Hours>,
    /// Whether EVSEs may still charge outside the location's opening hours.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub charging_when_closed: Option<bool>,
    /// Links to images related to the location.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<Image>,
    /// Details on the energy supplied at this location.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub energy_mix: Option<EnergyMix>,
    /// Timestamp when this location, an EVSE, or a connector was last updated.
    pub last_updated: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connector_type_serde_renames() {
        for (ty, wire) in [
            (ConnectorType::Chademo, "\"CHADEMO\""),
            (ConnectorType::DomesticL, "\"DOMESTIC_L\""),
            (
                ConnectorType::Iec6030921Single16,
                "\"IEC_60309_2_single_16\"",
            ),
            (ConnectorType::Iec62196T2Combo, "\"IEC_62196_T2_COMBO\""),
            (ConnectorType::Iec62196T3a, "\"IEC_62196_T3A\""),
            (ConnectorType::TeslaS, "\"TESLA_S\""),
        ] {
            assert_eq!(serde_json::to_string(&ty).unwrap(), wire);
            let back: ConnectorType = serde_json::from_str(wire).unwrap();
            assert_eq!(back, ty);
        }
    }

    #[test]
    fn connector_type_rejects_later_version_variants() {
        // CHAOJI / GBT_* / NEMA_* / PANTOGRAPH_* / DOMESTIC_M+ are not in 2.1.1.
        for wire in [
            "\"CHAOJI\"",
            "\"GBT_AC\"",
            "\"NEMA_5_20\"",
            "\"PANTOGRAPH_BOTTOM_UP\"",
            "\"DOMESTIC_M\"",
        ] {
            assert!(
                serde_json::from_str::<ConnectorType>(wire).is_err(),
                "{wire} must not be a valid 2.1.1 ConnectorType"
            );
        }
    }

    #[test]
    fn power_type_has_no_two_phase() {
        assert_eq!(
            serde_json::to_string(&PowerType::Ac1Phase).unwrap(),
            "\"AC_1_PHASE\""
        );
        assert_eq!(serde_json::to_string(&PowerType::Dc).unwrap(), "\"DC\"");
        for wire in ["\"AC_2_PHASE\"", "\"AC_2_PHASE_SPLIT\""] {
            assert!(
                serde_json::from_str::<PowerType>(wire).is_err(),
                "{wire} must not be a valid 2.1.1 PowerType"
            );
        }
    }

    #[test]
    fn capability_set_is_2_1_1_only() {
        // The full 2.1.1 set round-trips...
        for cap in [
            Capability::ChargingProfileCapable,
            Capability::CreditCardPayable,
            Capability::RemoteStartStopCapable,
            Capability::Reservable,
            Capability::RfidReader,
            Capability::UnlockCapable,
        ] {
            let wire = serde_json::to_string(&cap).unwrap();
            assert_eq!(serde_json::from_str::<Capability>(&wire).unwrap(), cap);
        }
        // ...and 2.2+ additions are rejected.
        for wire in [
            "\"CHIP_CARD_SUPPORT\"",
            "\"DEBIT_CARD_PAYABLE\"",
            "\"TOKEN_GROUP_CAPABLE\"",
            "\"START_SESSION_CONNECTOR_REQUIRED\"",
        ] {
            assert!(serde_json::from_str::<Capability>(wire).is_err());
        }
    }

    #[test]
    fn status_outoforder_wire_form() {
        assert_eq!(
            serde_json::to_string(&Status::Outoforder).unwrap(),
            "\"OUTOFORDER\""
        );
        assert_eq!(
            serde_json::to_string(&Status::Available).unwrap(),
            "\"AVAILABLE\""
        );
    }

    #[test]
    fn location_type_serde() {
        assert_eq!(
            serde_json::to_string(&LocationType::OnStreet).unwrap(),
            "\"ON_STREET\""
        );
        assert_eq!(
            serde_json::from_str::<LocationType>("\"UNDERGROUND_GARAGE\"").unwrap(),
            LocationType::UndergroundGarage
        );
    }

    #[test]
    fn connector_tariff_id_is_singular() {
        let c = Connector {
            id: CiString36::try_from("1").unwrap(),
            standard: ConnectorType::Iec62196T2,
            format: ConnectorFormat::Cable,
            power_type: PowerType::Ac3Phase,
            voltage: 220,
            amperage: 16,
            tariff_id: Some(CiString36::try_from("11").unwrap()),
            terms_and_conditions: None,
            last_updated: "2015-03-16T10:10:02Z".parse().unwrap(),
        };
        let json = serde_json::to_string(&c).unwrap();
        // Singular scalar `tariff_id`, never the 2.2.1 `tariff_ids` array.
        assert!(json.contains("\"tariff_id\":\"11\""), "{json}");
        assert!(!json.contains("tariff_ids"), "{json}");
        assert!(!json.contains("max_electric_power"), "{json}");
        let back: Connector = serde_json::from_str(&json).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn location_serde_spec_example() {
        // Ported from the OCPI 2.1.1 spec Location example (§8.3.1.1), trimmed
        // to a single EVSE. Note the required `type`, no country_code/party_id,
        // and a singular `tariff_id` per connector.
        //
        // The spec example JSON labels the EVSE's human-readable ID `"id"`,
        // but the EVSE object field table (§8.3.2) names it `evse_id` — a known
        // inconsistency in the 2.1.1 document. We follow the normative field
        // table (`evse_id`), which is also what 2.2+ standardised on.
        let json = r#"{
            "id": "LOC1",
            "type": "ON_STREET",
            "name": "Gent Zuid",
            "address": "F.Rooseveltlaan 3A",
            "city": "Gent",
            "postal_code": "9000",
            "country": "BEL",
            "coordinates": { "latitude": "51.047590", "longitude": "3.729940" },
            "evses": [{
                "uid": "3256",
                "evse_id": "BE-BEC-E041503001",
                "status": "AVAILABLE",
                "status_schedule": [],
                "capabilities": ["RESERVABLE"],
                "connectors": [{
                    "id": "1",
                    "standard": "IEC_62196_T2",
                    "format": "CABLE",
                    "power_type": "AC_3_PHASE",
                    "voltage": 220,
                    "amperage": 16,
                    "tariff_id": "11",
                    "last_updated": "2015-03-16T10:10:02Z"
                }],
                "physical_reference": "1",
                "floor_level": "-1",
                "last_updated": "2015-06-28T08:12:01Z"
            }],
            "operator": { "name": "BeCharged" },
            "last_updated": "2015-06-29T20:39:09Z"
        }"#;
        let loc: Location = serde_json::from_str(json).unwrap();
        assert_eq!(loc.id.as_str(), "LOC1");
        assert_eq!(loc.location_type, LocationType::OnStreet);
        assert_eq!(loc.postal_code, "9000");
        assert_eq!(loc.country, "BEL");
        assert_eq!(loc.evses.len(), 1);
        let evse = &loc.evses[0];
        assert_eq!(evse.status, Status::Available);
        assert_eq!(evse.evse_id.as_ref().unwrap().as_str(), "BE-BEC-E041503001");
        assert_eq!(evse.connectors[0].standard, ConnectorType::Iec62196T2);
        assert_eq!(
            evse.connectors[0].tariff_id.as_ref().unwrap().as_str(),
            "11"
        );
        assert_eq!(loc.operator.as_ref().unwrap().name, "BeCharged");

        let back: Location = serde_json::from_str(&serde_json::to_string(&loc).unwrap()).unwrap();
        assert_eq!(back, loc);
    }

    #[test]
    fn location_wire_form_omits_2_2_fields() {
        let loc = Location {
            id: CiString39::try_from("LOC1").unwrap(),
            location_type: LocationType::OnStreet,
            name: None,
            address: "F.Rooseveltlaan 3A".into(),
            city: "Gent".into(),
            postal_code: "9000".into(),
            country: "BEL".into(),
            coordinates: GeoLocation {
                latitude: "51.047590".into(),
                longitude: "3.729940".into(),
            },
            related_locations: Vec::new(),
            evses: Vec::new(),
            directions: Vec::new(),
            operator: None,
            suboperator: None,
            owner: None,
            facilities: Vec::new(),
            time_zone: None,
            opening_times: None,
            charging_when_closed: None,
            images: Vec::new(),
            energy_mix: None,
            last_updated: "2015-06-29T20:39:09Z".parse().unwrap(),
        };
        let json = serde_json::to_string(&loc).unwrap();
        for absent in ["country_code", "party_id", "publish", "parking_type"] {
            assert!(
                !json.contains(absent),
                "2.1.1 Location must not carry {absent}: {json}"
            );
        }
        // The 2.1.1-specific required `type` field IS present.
        assert!(json.contains("\"type\":\"ON_STREET\""), "{json}");
    }
}

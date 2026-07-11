//! OCPI **2.2** Locations module — the wire-delta overrides over 2.2.1.
//!
//! Only two Connector enums genuinely differ on the 2.2 wire; every other
//! Locations type (`Location`, `Evse`, `Connector`, `ConnectorFormat`,
//! `Capability`, `GeoLocation`, …) is wire-identical to 2.2.1 and re-exported
//! unchanged by [`super`].
//!
//! The deltas, per `specs/ocpi/2.2.1/version_history.asciidoc` (read backwards —
//! these are the 2.2.1 additions that 2.2 does **not** have) and the two vendored
//! module specs `specs/ocpi/2.2/mod_locations.asciidoc` vs
//! `specs/ocpi/2.2.1/mod_locations.asciidoc`:
//!
//! - [`PowerType`] — 2.2 has only `AC_1_PHASE`, `AC_3_PHASE`, `DC`. 2.2.1 added
//!   `AC_2_PHASE` and `AC_2_PHASE_SPLIT`.
//! - [`ConnectorType`] — 2.2 omits the values 2.2.1 added: `CHAOJI`,
//!   `DOMESTIC_M`/`DOMESTIC_N`/`DOMESTIC_O`, `GBT_AC`/`GBT_DC`, and the
//!   `NEMA_5_20`/`NEMA_6_30`/`NEMA_6_50`/`NEMA_10_30`/`NEMA_10_50`/`NEMA_14_30`/
//!   `NEMA_14_50` family.
//!
//! Both enums keep their 2.2.1 Rust variant names and `serde` renames for the
//! values they share, so a 2.2 value round-trips to the identical wire string —
//! only the 2.2.1-added values are absent, which a 2.2 [`super`]-level consumer
//! must not be able to name or deserialize.
//!
//! ## Scope: the enums *and* the composites that carry them
//!
//! The two enums above are the only genuine 2.2 wire delta, but the composite
//! Locations objects that embed them — [`Connector`] → [`Evse`] → [`Location`] —
//! must be `v2_2`-local too, otherwise a `v2_2::Location` would keep referencing
//! the *2.2.1* `PowerType`/`ConnectorType` (via the re-export) and could silently
//! admit a 2.2.1-only power/plug value on a 2.2 connector. So this module also
//! defines [`Connector`]/[`Evse`]/[`Location`], **structurally identical** to
//! their 2.2.1 counterparts except that [`Connector::standard`] /
//! [`Connector::power_type`] are the 2.2 enums. Every other field type
//! (`ConnectorFormat`, `Status`, `Capability`, `GeoLocation`, `DisplayText`,
//! `EnergyMix`, …) is wire-identical and re-used from [`crate::v2_2_1`] /
//! [`crate::common`] unchanged — no duplication beyond the three composites the
//! enums flow through.
//!
//! The result: anything pinned to `v2_2::{Connector, Evse, Location}` rejects a
//! 2.2.1-only power/plug value on deserialize rather than coercing it. The
//! remaining Locations follow-up (issue #167) is the **client sender + server
//! receiver wiring** over these composites, mirroring the 2.1.1 Locations
//! routers.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::common::{
    BusinessDetails, CiString2, CiString3, CiString36, DisplayText, EnergyMix, GeoLocation, Image,
    Url,
};
use crate::v2_2_1::{
    AdditionalGeoLocation, Capability, ConnectorFormat, Facility, Hours, ParkingRestriction,
    ParkingType, PublishTokenType, Status, StatusSchedule,
};

// ── ConnectorType ─────────────────────────────────────────────────────────────

/// Standard of an EVSE connector socket or plug (OCPI 2.2 value set).
///
/// Identical to the 2.2.1 enum minus the values 2.2.1 introduced: `CHAOJI`,
/// `DOMESTIC_M`/`N`/`O`, `GBT_AC`/`GBT_DC`, and the extended NEMA family. A 2.2
/// peer never emits those; deserializing one into this enum fails rather than
/// being silently coerced.
///
/// Spec: `specs/ocpi/2.2/mod_locations.asciidoc` — ConnectorType enum.
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
    /// IEC 60309-2 Industrial single phase 16 A (typically blue).
    #[serde(rename = "IEC_60309_2_single_16")]
    Iec6030921Single16,
    /// IEC 60309-2 Industrial three phase 16 A (typically red).
    #[serde(rename = "IEC_60309_2_three_16")]
    Iec6030922Three16,
    /// IEC 60309-2 Industrial three phase 32 A (typically red).
    #[serde(rename = "IEC_60309_2_three_32")]
    Iec6030922Three32,
    /// IEC 60309-2 Industrial three phase 64 A (typically red).
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
    /// On-board bottom-up pantograph (typically for bus charging).
    PantographBottomUp,
    /// Off-board top-down pantograph (typically for bus charging).
    PantographTopDown,
    /// Tesla Connector "Roadster"-type (round, 4 pin).
    TeslaR,
    /// Tesla Connector "Model S"-type (oval, 5 pin).
    TeslaS,
}

// ── PowerType ─────────────────────────────────────────────────────────────────

/// Electrical power type at an EVSE (OCPI 2.2 value set).
///
/// 2.2 knows only single-phase AC, three-phase AC, and DC. The `AC_2_PHASE` /
/// `AC_2_PHASE_SPLIT` values were added in 2.2.1 and are absent here — a 2.2
/// peer never emits them, and deserializing one fails rather than coercing.
///
/// Spec: `specs/ocpi/2.2/mod_locations.asciidoc` — PowerType enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PowerType {
    /// AC single phase.
    #[serde(rename = "AC_1_PHASE")]
    Ac1Phase,
    /// AC three phases.
    #[serde(rename = "AC_3_PHASE")]
    Ac3Phase,
    /// Direct current.
    #[serde(rename = "DC")]
    Dc,
}

// ── Connector ─────────────────────────────────────────────────────────────────

/// A single connector on an EVSE (OCPI 2.2).
///
/// Structurally identical to [`crate::v2_2_1::Connector`] except that
/// [`Connector::standard`] and [`Connector::power_type`] are the **2.2**
/// [`ConnectorType`] / [`PowerType`] — so a connector negotiated down to 2.2
/// cannot carry a 2.2.1-only plug/power value.
///
/// Spec: `specs/ocpi/2.2/mod_locations.asciidoc` — Connector object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Connector {
    /// Connector identifier within the EVSE (unique per EVSE, not globally).
    pub id: CiString36,
    /// Plug/socket standard (2.2 value set).
    pub standard: ConnectorType,
    /// Socket or cable format.
    pub format: ConnectorFormat,
    /// AC or DC power type (2.2 value set).
    pub power_type: PowerType,
    /// Maximum voltage (line-to-neutral for AC_3_PHASE), in volts.
    pub max_voltage: u32,
    /// Maximum amperage, in amperes.
    pub max_amperage: u32,
    /// Maximum power in watts (when lower than voltage × amperage).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max_electric_power: Option<u32>,
    /// IDs of currently valid tariffs for this connector.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tariff_ids: Vec<CiString36>,
    /// URL to the operator's terms and conditions.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub terms_and_conditions: Option<Url>,
    /// Last update timestamp (UTC).
    pub last_updated: DateTime<Utc>,
}

// ── Evse ──────────────────────────────────────────────────────────────────────

/// An EVSE (Electric Vehicle Supply Equipment) within a location (OCPI 2.2).
///
/// Structurally identical to [`crate::v2_2_1::Evse`] but its [`Evse::connectors`]
/// are the 2.2 [`Connector`], so the 2.2 enum set flows all the way down.
///
/// Spec: `specs/ocpi/2.2/mod_locations.asciidoc` — EVSE object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evse {
    /// Technical identifier, unique within the CPO's platform.
    pub uid: CiString36,
    /// eMI3 EVSE ID (optional; may be absent when status is REMOVED).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub evse_id: Option<String>,
    /// Current status.
    pub status: Status,
    /// Planned status transitions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub status_schedule: Vec<StatusSchedule>,
    /// EVSE capabilities.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<Capability>,
    /// Connectors on this EVSE (at least one required).
    pub connectors: Vec<Connector>,
    /// Floor level in a garage (e.g. `"-1"`, `"2"`).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub floor_level: Option<String>,
    /// EVSE coordinates (more precise than the location coordinates).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub coordinates: Option<GeoLocation>,
    /// Visual reference number printed on the EVSE.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub physical_reference: Option<String>,
    /// Multi-language directions to reach this EVSE from the location.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub directions: Vec<DisplayText>,
    /// Parking restrictions at this EVSE.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parking_restrictions: Vec<ParkingRestriction>,
    /// Images (photos, logos) for this EVSE.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<Image>,
    /// Last update timestamp (UTC).
    pub last_updated: DateTime<Utc>,
}

// ── Location ──────────────────────────────────────────────────────────────────

/// A charging location containing one or more EVSEs (OCPI 2.2).
///
/// Structurally identical to [`crate::v2_2_1::Location`] but its [`Location::evses`]
/// are the 2.2 [`Evse`] — the composite that carries the 2.2 connector enums.
///
/// Spec: `specs/ocpi/2.2/mod_locations.asciidoc` — Location object.
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
    /// General parking type.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub parking_type: Option<ParkingType>,
    /// EVSEs at this location.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evses: Vec<Evse>,
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
    /// Last update timestamp (UTC).
    pub last_updated: DateTime<Utc>,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{Connector, ConnectorType, Evse, Location, PowerType};
    use crate::common::{CiString2, CiString3, CiString36, GeoLocation};
    use crate::v2_2_1::ConnectorFormat;
    use chrono::{TimeZone, Utc};

    #[test]
    fn power_type_2_2_round_trips_its_values() {
        for (value, wire) in [
            (PowerType::Ac1Phase, "\"AC_1_PHASE\""),
            (PowerType::Ac3Phase, "\"AC_3_PHASE\""),
            (PowerType::Dc, "\"DC\""),
        ] {
            assert_eq!(serde_json::to_string(&value).unwrap(), wire);
            assert_eq!(serde_json::from_str::<PowerType>(wire).unwrap(), value);
        }
    }

    #[test]
    fn power_type_2_2_rejects_2_2_1_two_phase_values() {
        // `AC_2_PHASE` / `AC_2_PHASE_SPLIT` were added in 2.2.1; a 2.2 enum must
        // reject them rather than silently coerce — no data smuggled across the
        // version boundary.
        assert!(serde_json::from_str::<PowerType>("\"AC_2_PHASE\"").is_err());
        assert!(serde_json::from_str::<PowerType>("\"AC_2_PHASE_SPLIT\"").is_err());
    }

    #[test]
    fn connector_type_2_2_round_trips_shared_values() {
        // A representative spread of the shared 2.2 values, including the renamed
        // IEC variants, must round-trip to the identical wire string.
        for (value, wire) in [
            (ConnectorType::Chademo, "\"CHADEMO\""),
            (ConnectorType::DomesticF, "\"DOMESTIC_F\""),
            (ConnectorType::DomesticL, "\"DOMESTIC_L\""),
            (
                ConnectorType::Iec6030921Single16,
                "\"IEC_60309_2_single_16\"",
            ),
            (ConnectorType::Iec6030922Three64, "\"IEC_60309_2_three_64\""),
            (ConnectorType::Iec62196T2Combo, "\"IEC_62196_T2_COMBO\""),
            (ConnectorType::Iec62196T3a, "\"IEC_62196_T3A\""),
            (
                ConnectorType::PantographBottomUp,
                "\"PANTOGRAPH_BOTTOM_UP\"",
            ),
            (ConnectorType::TeslaS, "\"TESLA_S\""),
        ] {
            assert_eq!(serde_json::to_string(&value).unwrap(), wire);
            assert_eq!(serde_json::from_str::<ConnectorType>(wire).unwrap(), value);
        }
    }

    #[test]
    fn connector_type_2_2_rejects_2_2_1_added_values() {
        // Every value 2.2.1 introduced must fail to deserialize into the 2.2 enum
        // (proving no silent coercion of a value a 2.2 peer cannot mean).
        for wire in [
            "\"CHAOJI\"",
            "\"DOMESTIC_M\"",
            "\"DOMESTIC_N\"",
            "\"DOMESTIC_O\"",
            "\"GBT_AC\"",
            "\"GBT_DC\"",
            "\"NEMA_5_20\"",
            "\"NEMA_6_30\"",
            "\"NEMA_6_50\"",
            "\"NEMA_10_30\"",
            "\"NEMA_10_50\"",
            "\"NEMA_14_30\"",
            "\"NEMA_14_50\"",
        ] {
            assert!(
                serde_json::from_str::<ConnectorType>(wire).is_err(),
                "2.2 ConnectorType must reject 2.2.1-added value {wire}"
            );
        }
    }

    // ── Composite overrides ───────────────────────────────────────────────────

    fn sample_connector() -> Connector {
        Connector {
            id: CiString36::try_from("1").unwrap(),
            standard: ConnectorType::Iec62196T2,
            format: ConnectorFormat::Socket,
            power_type: PowerType::Ac3Phase,
            max_voltage: 400,
            max_amperage: 32,
            max_electric_power: Some(22_000),
            tariff_ids: vec![CiString36::try_from("TARIFF_A").unwrap()],
            terms_and_conditions: None,
            last_updated: Utc.with_ymd_and_hms(2026, 7, 11, 12, 0, 0).unwrap(),
        }
    }

    fn sample_location() -> Location {
        Location {
            country_code: CiString2::try_from("NL").unwrap(),
            party_id: CiString3::try_from("EXA").unwrap(),
            id: CiString36::try_from("LOC1").unwrap(),
            publish: true,
            publish_allowed_to: vec![],
            name: Some("Example Location".into()),
            address: "Street 1".into(),
            city: "Amsterdam".into(),
            postal_code: Some("1000 AA".into()),
            state: None,
            country: "NLD".into(),
            coordinates: GeoLocation {
                latitude: "52.370216".into(),
                longitude: "4.895168".into(),
            },
            related_locations: vec![],
            parking_type: None,
            evses: vec![Evse {
                uid: CiString36::try_from("EVSE1").unwrap(),
                evse_id: Some("NL*EXA*E1".into()),
                status: crate::v2_2_1::Status::Available,
                status_schedule: vec![],
                capabilities: vec![],
                connectors: vec![sample_connector()],
                floor_level: None,
                coordinates: None,
                physical_reference: None,
                directions: vec![],
                parking_restrictions: vec![],
                images: vec![],
                last_updated: Utc.with_ymd_and_hms(2026, 7, 11, 12, 0, 0).unwrap(),
            }],
            directions: vec![],
            operator: None,
            suboperator: None,
            owner: None,
            facilities: vec![],
            time_zone: "Europe/Amsterdam".into(),
            opening_times: None,
            charging_when_closed: None,
            images: vec![],
            energy_mix: None,
            last_updated: Utc.with_ymd_and_hms(2026, 7, 11, 12, 0, 0).unwrap(),
        }
    }

    #[test]
    fn location_2_2_round_trips_through_its_2_2_enums() {
        // A full 2.2 Location whose connector carries the 2.2 enum set must
        // round-trip byte-for-byte — the composite is a faithful 2.2 object, not
        // a coercion of the 2.2.1 struct.
        let loc = sample_location();
        let json = serde_json::to_string(&loc).unwrap();
        let back: Location = serde_json::from_str(&json).unwrap();
        assert_eq!(back, loc);
        assert_eq!(
            back.evses[0].connectors[0].standard,
            ConnectorType::Iec62196T2
        );
        assert_eq!(back.evses[0].connectors[0].power_type, PowerType::Ac3Phase);
    }

    #[test]
    fn connector_2_2_rejects_2_2_1_only_power_type() {
        // A connector JSON with a 2.2.1-only `power_type` (`AC_2_PHASE`) must fail
        // to deserialize into the 2.2 `Connector` — the enum override propagates
        // through the composite, so no 2.2.1-only value can ride a 2.2 connector.
        let json = r#"{
            "id": "1",
            "standard": "IEC_62196_T2",
            "format": "SOCKET",
            "power_type": "AC_2_PHASE",
            "max_voltage": 400,
            "max_amperage": 32,
            "last_updated": "2026-07-11T12:00:00Z"
        }"#;
        assert!(
            serde_json::from_str::<Connector>(json).is_err(),
            "2.2 Connector must reject a 2.2.1-only power_type"
        );
    }

    #[test]
    fn connector_2_2_rejects_2_2_1_only_connector_type() {
        // Likewise a 2.2.1-only `standard` (`GBT_DC`) must not deserialize into a
        // 2.2 `Connector`.
        let json = r#"{
            "id": "1",
            "standard": "GBT_DC",
            "format": "CABLE",
            "power_type": "DC",
            "max_voltage": 800,
            "max_amperage": 200,
            "last_updated": "2026-07-11T12:00:00Z"
        }"#;
        assert!(
            serde_json::from_str::<Connector>(json).is_err(),
            "2.2 Connector must reject a 2.2.1-only standard"
        );
    }
}

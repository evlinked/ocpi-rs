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
//! ## Scope: the enums, not `Connector`
//!
//! The re-exported [`super::Connector`] struct still references the *2.2.1*
//! `PowerType`/`ConnectorType` internally — overriding the composite Locations
//! objects (`Connector` → `Evse` → `Location`) to carry these 2.2 enums is the
//! Locations client/server wiring follow-up, not this types-only slice (mirrors
//! how the CDRs and Commands slices landed their delta types before any wiring).
//! What this module delivers is a **faithful standalone 2.2 enum**: a value the
//! 2.2.1 enum accepts (e.g. `GBT_DC`) is rejected here, so nothing that pins its
//! type to `v2_2::ConnectorType`/`v2_2::PowerType` can silently admit a 2.2.1-only
//! value.

use serde::{Deserialize, Serialize};

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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{ConnectorType, PowerType};

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
}

//! OCPI 2.1.1 — Tariffs module types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::common::{CiString36, DisplayText, EnergyMix, Url};

// ── TariffDimensionType ───────────────────────────────────────────────────────

/// The dimension a 2.1.1 [`PriceComponent`] is priced against.
///
/// OCPI 2.1.1 defines exactly these four; later versions add more. Identical to
/// the 2.2.1 enum, but defined here so the 2.1.1 Tariffs surface is complete
/// and version-pinned.
///
/// Spec: OCPI 2.1.1 — *Tariffs* module, `TariffDimensionType`
/// (`specs/ocpi/2.1.1/OCPI_2.1.1.pdf`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TariffDimensionType {
    /// Defined in kWh, `step_size` multiplier: 1 Wh.
    Energy,
    /// Flat fee, no unit.
    Flat,
    /// Time not charging: defined in hours, `step_size` multiplier: 1 second.
    ParkingTime,
    /// Time charging: defined in hours, `step_size` multiplier: 1 second.
    Time,
}

// ── PriceComponent ────────────────────────────────────────────────────────────

/// A single priced dimension within a 2.1.1 [`TariffElement`].
///
/// ## Delta from 2.2.1
///
/// The 2.1.1 `PriceComponent` has **no `vat`** field — per-component VAT was
/// added in 2.2.1.
///
/// Spec: OCPI 2.1.1 — *Tariffs* module, `PriceComponent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriceComponent {
    /// The dimension that this component applies to.
    ///
    /// Field renamed from spec `type` (Rust keyword).
    #[serde(rename = "type")]
    pub component_type: TariffDimensionType,
    /// Price per unit (excl. VAT) for this component.
    pub price: f64,
    /// Minimum amount to be billed. Unit depends on the `component_type`.
    pub step_size: u32,
}

// ── TariffRestrictions ────────────────────────────────────────────────────────

/// Conditions under which a 2.1.1 [`TariffElement`] applies.
///
/// ## Delta from 2.2.1
///
/// The 2.1.1 restriction set has **no `min_current` / `max_current`** (added in
/// 2.2) and **no `reservation`** restriction (added in 2.2.1). It is the sole
/// place validity windows live in 2.1.1 — the `Tariff` object itself carries no
/// `start_date_time` / `end_date_time`.
///
/// Spec: OCPI 2.1.1 — *Tariffs* module, `TariffRestrictions`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TariffRestrictions {
    /// Start time of day in local time, applies every day, format: `HH:MM` (24h).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub start_time: Option<String>,
    /// End time of day in local time, applies every day, format: `HH:MM` (24h).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub end_time: Option<String>,
    /// Start date, applies from this day (inclusive), format: `YYYY-MM-DD`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub start_date: Option<String>,
    /// End date, applies until this day (inclusive), format: `YYYY-MM-DD`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub end_date: Option<String>,
    /// Minimum consumed energy (kWh) — inclusive.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub min_kwh: Option<f64>,
    /// Maximum consumed energy (kWh) — exclusive.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max_kwh: Option<f64>,
    /// Minimum power (kW) over the entire charging session — inclusive.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub min_power: Option<f64>,
    /// Maximum power (kW) over the entire charging session — exclusive.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max_power: Option<f64>,
    /// Minimum duration (seconds) of the charging session — inclusive.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub min_duration: Option<u32>,
    /// Maximum duration (seconds) of the charging session — exclusive.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max_duration: Option<u32>,
    /// Applicable days of the week. Reuses the version-agnostic
    /// [`crate::v2_2_1::DayOfWeek`] (the Mon–Sun enum is identical across versions).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub day_of_week: Vec<crate::v2_2_1::DayOfWeek>,
}

// ── TariffElement ─────────────────────────────────────────────────────────────

/// A group of price components with optional restrictions on when they apply.
///
/// Spec: OCPI 2.1.1 — *Tariffs* module, `TariffElement`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TariffElement {
    /// List of price components that make up the pricing of this tariff.
    pub price_components: Vec<PriceComponent>,
    /// Optional restrictions that constrain when this element applies.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub restrictions: Option<TariffRestrictions>,
}

// ── Tariff ────────────────────────────────────────────────────────────────────

/// An OCPI **2.1.1** Tariff — the pricing a CPO advertises for using its EVSEs.
///
/// ## Deltas from the 2.2.1 [`crate::v2_2_1::Tariff`]
///
/// - **No** `country_code` / `party_id` (added in 2.2).
/// - **No** `type: TariffType` (ad-hoc/profile tariff typing is 2.2).
/// - **No** `min_price` / `max_price` (added in 2.2.1).
/// - **No** top-level `start_date_time` / `end_date_time` — validity windows
///   live only inside [`TariffRestrictions`] in 2.1.1.
///
/// Spec: OCPI 2.1.1 — *Tariffs* module, *Object description*
/// (`specs/ocpi/2.1.1/OCPI_2.1.1.pdf`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tariff {
    /// Unique tariff ID within the CPO's platform.
    pub id: CiString36,
    /// ISO 4217 currency code.
    pub currency: String,
    /// Human-readable name(s) or description(s) in various languages.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tariff_alt_text: Vec<DisplayText>,
    /// Alternative URL to a tariff information web page.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tariff_alt_url: Option<Url>,
    /// List of tariff elements.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub elements: Vec<TariffElement>,
    /// Details on the energy supplied with this tariff.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub energy_mix: Option<EnergyMix>,
    /// Timestamp of the last update (or creation) of this tariff.
    pub last_updated: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v2_2_1::DayOfWeek;

    #[test]
    fn tariff_dimension_type_serde_roundtrip() {
        for (ty, wire) in [
            (TariffDimensionType::Energy, "\"ENERGY\""),
            (TariffDimensionType::Flat, "\"FLAT\""),
            (TariffDimensionType::ParkingTime, "\"PARKING_TIME\""),
            (TariffDimensionType::Time, "\"TIME\""),
        ] {
            assert_eq!(serde_json::to_string(&ty).unwrap(), wire);
            let back: TariffDimensionType = serde_json::from_str(wire).unwrap();
            assert_eq!(back, ty);
        }
    }

    #[test]
    fn price_component_has_no_vat() {
        let pc = PriceComponent {
            component_type: TariffDimensionType::Energy,
            price: 0.25,
            step_size: 1,
        };
        let json = serde_json::to_string(&pc).unwrap();
        assert!(
            !json.contains("vat"),
            "2.1.1 PriceComponent must not carry `vat`: {json}"
        );
    }

    #[test]
    fn tariff_serde_spec_example() {
        // Ported from the OCPI 2.1.1 spec Tariff example: a simple energy +
        // parking tariff with a time-window restriction. Note: no country_code/
        // party_id/type/min_price/max_price, and no top-level validity dates.
        // Spec ref: specs/ocpi/2.1.1/OCPI_2.1.1.pdf, "Tariffs" chapter.
        let json = r#"{
            "id": "12",
            "currency": "EUR",
            "tariff_alt_text": [
                {"language": "en", "text": "2.00 euro p/hour"}
            ],
            "elements": [
                {
                    "price_components": [
                        {"type": "TIME", "price": 2.00, "step_size": 300}
                    ]
                },
                {
                    "price_components": [
                        {"type": "PARKING_TIME", "price": 3.00, "step_size": 60}
                    ],
                    "restrictions": {
                        "start_time": "09:00",
                        "end_time": "18:00",
                        "day_of_week": ["MONDAY", "TUESDAY"]
                    }
                }
            ],
            "last_updated": "2018-12-17T11:36:01Z"
        }"#;
        let tariff: Tariff = serde_json::from_str(json).unwrap();
        assert_eq!(tariff.id.as_str(), "12");
        assert_eq!(tariff.currency, "EUR");
        assert_eq!(tariff.elements.len(), 2);
        assert_eq!(
            tariff.elements[0].price_components[0].component_type,
            TariffDimensionType::Time
        );
        let restr = tariff.elements[1].restrictions.as_ref().unwrap();
        assert_eq!(restr.start_time.as_deref(), Some("09:00"));
        assert_eq!(
            restr.day_of_week,
            vec![DayOfWeek::Monday, DayOfWeek::Tuesday]
        );

        let back: Tariff = serde_json::from_str(&serde_json::to_string(&tariff).unwrap()).unwrap();
        assert_eq!(back, tariff);
    }

    #[test]
    fn tariff_wire_form_omits_2_2_fields() {
        let tariff = Tariff {
            id: CiString36::try_from("12").unwrap(),
            currency: "EUR".into(),
            tariff_alt_text: Vec::new(),
            tariff_alt_url: None,
            elements: vec![TariffElement {
                price_components: vec![PriceComponent {
                    component_type: TariffDimensionType::Flat,
                    price: 1.0,
                    step_size: 1,
                }],
                restrictions: None,
            }],
            energy_mix: None,
            last_updated: "2018-12-17T11:36:01Z".parse().unwrap(),
        };
        let json = serde_json::to_string(&tariff).unwrap();
        // The Tariff object itself carries none of these 2.2/2.2.1 additions.
        // (`type` is not checked by substring here because a nested
        // PriceComponent legitimately uses the `type` key; the absence of a
        // Tariff-level `tariff_type` field is guaranteed structurally.)
        for absent in [
            "country_code",
            "party_id",
            "min_price",
            "max_price",
            "start_date_time",
            "end_date_time",
        ] {
            assert!(
                !json.contains(absent),
                "2.1.1 Tariff must not carry {absent}: {json}"
            );
        }
    }
}

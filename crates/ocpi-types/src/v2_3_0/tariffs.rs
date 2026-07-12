//! OCPI **2.3.0** Tariffs — the North-American tax delta over 2.2.1.
//!
//! 2.3.0 adds explicit support for **North-American taxes**, where tax is added
//! on top of a listed price rather than being baked into it (the VAT model). Per
//! `specs/ocpi/2.3.0/mod_tariffs.asciidoc` and `changelog.asciidoc` ("Support
//! for North American taxes"), the [`Tariff`] object gains, relative to its
//! 2.2.1 shape:
//!
//! - a required [`tax_included`](Tariff::tax_included) flag ([`TaxIncluded`]) —
//!   whether the amounts in the tariff already include tax, do not, or no tax
//!   applies;
//! - [`min_price`](Tariff::min_price) / [`max_price`](Tariff::max_price) become
//!   the tax-aware [`PriceLimit`] (a `before_taxes` floor/ceiling plus an
//!   optional `after_taxes` one) instead of the VAT-only [`Price`](crate::Price);
//! - a [`preauthorize_amount`](Tariff::preauthorize_amount) a Payment Terminal
//!   Provider should preauthorize for card payment.
//!
//! Everything else is byte-for-byte the 2.2.1 shape, so the sub-types
//! ([`TariffElement`], [`PriceComponent`](crate::v2_2_1::PriceComponent),
//! [`TariffRestrictions`](crate::v2_2_1::TariffRestrictions), the enums) stay
//! plain re-exports of the 2.2.1 types — only [`Tariff`] itself forks, plus the
//! two new value types [`PriceLimit`] and [`TaxIncluded`] this module defines.
//!
//! The North-American case leaves the [`PriceComponent`](crate::v2_2_1::PriceComponent)
//! `vat` field empty (tax rates are not known to the CPO beforehand); the
//! top-level [`tax_included`](Tariff::tax_included) flag then says whether the
//! listed prices are inclusive of tax or not.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::common::{CiString2, CiString3, CiString36, DisplayText, EnergyMix, Url};
use crate::v2_2_1::{TariffElement, TariffType};

/// Describes whether tax has to be added to the amounts in a [`Tariff`].
///
/// The North-American tax delta of 2.3.0: unlike the VAT model (where tax is
/// always included and carried per price component), a Tariff states once, at
/// the top level, how tax relates to its listed prices.
///
/// Spec: `specs/ocpi/2.3.0/mod_tariffs.asciidoc` — TaxIncluded enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaxIncluded {
    /// Taxes are included in the prices in this tariff.
    #[serde(rename = "YES")]
    Yes,
    /// Taxes are not included, and will be added on top of the prices afterward.
    #[serde(rename = "NO")]
    No,
    /// No taxes are applicable to this tariff.
    #[serde(rename = "N/A")]
    NotApplicable,
}

/// A minimum or maximum price on a [`Tariff`], carrying both a pre-tax and an
/// optional post-tax bound.
///
/// Because the tax on different parts of a session may differ, the two bounds
/// apply independently: the session cost before taxes never crosses
/// [`before_taxes`](PriceLimit::before_taxes), and — when set — the cost after
/// taxes never crosses [`after_taxes`](PriceLimit::after_taxes).
///
/// Replaces the VAT-only [`Price`](crate::Price) that 2.2.1 used for
/// `min_price` / `max_price`.
///
/// Spec: `specs/ocpi/2.3.0/mod_tariffs.asciidoc` — PriceLimit class.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriceLimit {
    /// Maximum or minimum cost excluding taxes.
    pub before_taxes: f64,
    /// Maximum or minimum cost including taxes.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub after_taxes: Option<f64>,
}

/// An OCPI **2.3.0** Tariff object — the 2.2.1 pricing scheme plus North-American
/// tax handling.
///
/// Structurally identical to [`crate::v2_2_1::Tariff`] except that
/// `min_price` / `max_price` are the tax-aware [`PriceLimit`], and it carries the
/// required [`tax_included`](Tariff::tax_included) flag and the optional
/// [`preauthorize_amount`](Tariff::preauthorize_amount).
///
/// Spec: `specs/ocpi/2.3.0/mod_tariffs.asciidoc` — Tariff object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tariff {
    /// ISO 3166-1 alpha-2 country code of the CPO that owns this tariff.
    pub country_code: CiString2,
    /// Party ID of the CPO that owns this tariff.
    pub party_id: CiString3,
    /// Unique tariff ID within the CPO's platform.
    pub id: CiString36,
    /// ISO 4217 currency code.
    pub currency: String,
    /// The optional tariff type. When absent this tariff can be used for all sessions.
    ///
    /// Field renamed from spec `type` (Rust keyword).
    #[serde(rename = "type", skip_serializing_if = "Option::is_none", default)]
    pub tariff_type: Option<TariffType>,
    /// Human-readable name(s) or description(s) in various languages.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tariff_alt_text: Vec<DisplayText>,
    /// Alternative URL to a tariff information web page.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tariff_alt_url: Option<Url>,
    /// Minimum charging price. When set, the session cost will never be lower
    /// than this limit (both the pre-tax and, if present, the post-tax bound).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub min_price: Option<PriceLimit>,
    /// Maximum charging price. When set, the session cost will never exceed this
    /// limit (both the pre-tax and, if present, the post-tax bound).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max_price: Option<PriceLimit>,
    /// Amount a Payment Terminal Provider should preauthorize when handling card
    /// payment for a session with this tariff.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub preauthorize_amount: Option<f64>,
    /// List of tariff elements.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub elements: Vec<TariffElement>,
    /// Whether taxes are included in the amounts in this tariff.
    pub tax_included: TaxIncluded,
    /// When this tariff becomes active.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub start_date_time: Option<DateTime<Utc>>,
    /// When this tariff becomes inactive.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub end_date_time: Option<DateTime<Utc>>,
    /// Details on the energy supplied with this tariff.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub energy_mix: Option<EnergyMix>,
    /// Timestamp of the last update (or creation) of this tariff.
    pub last_updated: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::{PriceLimit, Tariff, TaxIncluded};

    #[test]
    fn tax_included_round_trips_spec_wire_values() {
        for (variant, wire) in [
            (TaxIncluded::Yes, "\"YES\""),
            (TaxIncluded::No, "\"NO\""),
            (TaxIncluded::NotApplicable, "\"N/A\""),
        ] {
            assert_eq!(serde_json::to_string(&variant).unwrap(), wire);
            let back: TaxIncluded = serde_json::from_str(wire).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn price_limit_omits_absent_after_taxes() {
        let pl = PriceLimit {
            before_taxes: 10.0,
            after_taxes: None,
        };
        let json = serde_json::to_string(&pl).unwrap();
        assert_eq!(json, r#"{"before_taxes":10.0}"#);
        let back: PriceLimit = serde_json::from_str(&json).unwrap();
        assert_eq!(back, pl);
    }

    #[test]
    fn price_limit_carries_both_bounds() {
        let json = r#"{"before_taxes":0.5,"after_taxes":0.55}"#;
        let pl: PriceLimit = serde_json::from_str(json).unwrap();
        assert_eq!(pl.before_taxes, 0.5);
        assert_eq!(pl.after_taxes, Some(0.55));
        let back: PriceLimit = serde_json::from_str(&serde_json::to_string(&pl).unwrap()).unwrap();
        assert_eq!(back, pl);
    }

    /// Spec example "Simple Tariff with North American taxes" (§tariff_19):
    /// C$ 2.00 per hour of charging time, taxes **not** included, billed per
    /// 60 s. The North-American convention leaves the `vat` field empty and
    /// signals tax handling with the top-level `tax_included` flag.
    #[test]
    fn north_american_tax_excluded_tariff_round_trips() {
        let json = r#"{
            "country_code": "CA",
            "party_id": "EXA",
            "id": "19",
            "currency": "CAD",
            "tax_included": "NO",
            "elements": [
                {
                    "price_components": [
                        { "type": "TIME", "price": 2.0, "step_size": 60 }
                    ]
                }
            ],
            "last_updated": "2026-07-11T09:00:00Z"
        }"#;
        let tariff: Tariff = serde_json::from_str(json).unwrap();
        assert_eq!(tariff.tax_included, TaxIncluded::No);
        assert_eq!(tariff.currency, "CAD");
        assert_eq!(tariff.elements[0].price_components[0].price, 2.0);
        // Tax not known beforehand → no per-component VAT.
        assert!(tariff.elements[0].price_components[0].vat.is_none());
        let back: Tariff = serde_json::from_str(&serde_json::to_string(&tariff).unwrap()).unwrap();
        assert_eq!(back, tariff);
    }

    /// Spec example "Simple Tariff with North American taxes, price inclusive of
    /// tax" (§tariff_20): C$ 2.10 per hour, taxes **included**.
    #[test]
    fn north_american_tax_included_tariff_round_trips() {
        let json = r#"{
            "country_code": "US",
            "party_id": "EXA",
            "id": "20",
            "currency": "USD",
            "tax_included": "YES",
            "elements": [
                {
                    "price_components": [
                        { "type": "TIME", "price": 2.1, "step_size": 60 }
                    ]
                }
            ],
            "last_updated": "2026-07-11T09:00:00Z"
        }"#;
        let tariff: Tariff = serde_json::from_str(json).unwrap();
        assert_eq!(tariff.tax_included, TaxIncluded::Yes);
        let back: Tariff = serde_json::from_str(&serde_json::to_string(&tariff).unwrap()).unwrap();
        assert_eq!(back, tariff);
    }

    #[test]
    fn tariff_with_price_limits_and_preauthorize_round_trips() {
        let json = r#"{
            "country_code": "CA",
            "party_id": "EXA",
            "id": "min-max",
            "currency": "CAD",
            "tax_included": "NO",
            "min_price": { "before_taxes": 0.5, "after_taxes": 0.55 },
            "max_price": { "before_taxes": 10.0 },
            "preauthorize_amount": 25.0,
            "elements": [
                {
                    "price_components": [
                        { "type": "ENERGY", "price": 0.25, "step_size": 1 }
                    ]
                }
            ],
            "last_updated": "2026-07-11T09:00:00Z"
        }"#;
        let tariff: Tariff = serde_json::from_str(json).unwrap();
        assert_eq!(tariff.min_price.as_ref().unwrap().before_taxes, 0.5);
        assert_eq!(tariff.min_price.as_ref().unwrap().after_taxes, Some(0.55));
        assert_eq!(tariff.max_price.as_ref().unwrap().before_taxes, 10.0);
        assert_eq!(tariff.max_price.as_ref().unwrap().after_taxes, None);
        assert_eq!(tariff.preauthorize_amount, Some(25.0));
        let back: Tariff = serde_json::from_str(&serde_json::to_string(&tariff).unwrap()).unwrap();
        assert_eq!(back, tariff);
    }

    /// `tax_included` is required (card. 1). A payload omitting it is rejected on
    /// deserialize rather than silently defaulted — the spec's mandatory tax
    /// stance is enforced at the type boundary.
    #[test]
    fn tariff_without_tax_included_is_rejected() {
        let json = r#"{
            "country_code": "CA",
            "party_id": "EXA",
            "id": "no-tax-flag",
            "currency": "CAD",
            "elements": [
                {
                    "price_components": [
                        { "type": "TIME", "price": 2.0, "step_size": 60 }
                    ]
                }
            ],
            "last_updated": "2026-07-11T09:00:00Z"
        }"#;
        let err = serde_json::from_str::<Tariff>(json).unwrap_err();
        assert!(
            err.to_string().contains("tax_included"),
            "error should name the missing field: {err}"
        );
    }

    /// A 2.3.0 Tariff with `tax_included` cannot be parsed through the 2.2.1
    /// `Tariff`'s VAT-only `min_price`/`max_price` shape without loss: the 2.2.1
    /// type takes a [`Price`](crate::Price) (`excl_vat`) there, so the tax-aware
    /// `PriceLimit` value fails to deserialize — proving the fork is a genuine
    /// wire delta, not a transparent alias.
    #[test]
    fn price_limit_shape_differs_from_2_2_1_price() {
        let json = r#"{
            "country_code": "CA",
            "party_id": "EXA",
            "id": "delta",
            "currency": "CAD",
            "tax_included": "NO",
            "min_price": { "before_taxes": 0.5 },
            "elements": [
                {
                    "price_components": [
                        { "type": "TIME", "price": 2.0, "step_size": 60 }
                    ]
                }
            ],
            "last_updated": "2026-07-11T09:00:00Z"
        }"#;
        // Parses as 2.3.0 …
        assert!(serde_json::from_str::<Tariff>(json).is_ok());
        // … but the 2.2.1 Tariff's `min_price: Price` needs `excl_vat`, so a
        // PriceLimit body is not a valid 2.2.1 Price.
        assert!(serde_json::from_str::<crate::v2_2_1::Tariff>(json).is_err());
    }
}

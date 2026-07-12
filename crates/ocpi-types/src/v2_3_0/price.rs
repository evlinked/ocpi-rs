//! OCPI **2.3.0** reworked _Price_ value type + the new _TaxAmount_ class —
//! the North-American tax delta's load-bearing value types.
//!
//! ## Why 2.3.0 reworks `Price`
//!
//! Through 2.2.1, [`crate::common::Price`] models the **European VAT**
//! convention: a single `excl_vat` amount plus an optional `incl_vat`. That
//! shape cannot express **North-American** taxation, where tax is added *on top*
//! of a listed price, is often itemised (federal + provincial, e.g. GST + QST),
//! and whose rate may not be known to the CPO beforehand. Per
//! `specs/ocpi/2.3.0/types.asciidoc` (§`Price` / §`TaxAmount`) and
//! `specs/ocpi/2.3.0/changelog.asciidoc` ("Support for North American taxes"),
//! 2.3.0 replaces the VAT-only pair with:
//!
//! - [`Price::before_taxes`] — the price/cost **excluding** taxes (the successor
//!   of `excl_vat`), and
//! - [`Price::taxes`] — a list of [`TaxAmount`] entries, each an itemised tax
//!   applicable to this price and relevant to the receiver of the Session/CDR
//!   (replacing the single `incl_vat` number).
//!
//! This is a genuine wire fork, so it lives here as a `v2_3_0`-local type rather
//! than a re-export of `crate::common::Price`. It is the value type the 2.3.0
//! `Cdr` / `Session` cost-field forks are written against (tracked in #188); the
//! Tariffs-side tax delta (`tax_included` + `PriceLimit`) is a separate fork in
//! [`super::tariffs`] and does **not** use this `Price`.
//!
//! ### The trust boundary
//!
//! `before_taxes` and each `TaxAmount`'s `name`/`amount` are **required** (spec
//! card. `1`), and every field routes through serde's derived `Deserialize`, so
//! a payload that omits a mandatory field is **rejected on deserialize** rather
//! than silently defaulted — faithful to the crate's core promise (*the
//! unsupported case is rejected explicitly, never silently mangled*).

use serde::{Deserialize, Serialize};

// ── Price ─────────────────────────────────────────────────────────────────────

/// A price/cost with itemised taxes (OCPI **2.3.0**).
///
/// Replaces the VAT-only [`crate::common::Price`] (`excl_vat`/`incl_vat`). The
/// pre-tax amount is carried in `before_taxes`; every applicable tax is itemised
/// in `taxes`. A tax-free price simply carries an empty `taxes` list (omitted on
/// the wire).
///
/// Spec: `specs/ocpi/2.3.0/types.asciidoc` — Price class.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Price {
    /// Price/Cost **excluding** taxes.
    pub before_taxes: f64,
    /// All taxes applicable to this price and relevant to the receiver of the
    /// Session or CDR. Empty when no tax applies (then omitted on the wire).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub taxes: Vec<TaxAmount>,
}

// ── TaxAmount ─────────────────────────────────────────────────────────────────

/// One itemised tax applicable to a [`Price`] (OCPI **2.3.0**, new).
///
/// In countries that require a named tax (e.g. Canada) `name` carries something
/// like `"QST"`; elsewhere it can be a generic `"VAT"` / `"General Sales Tax"`.
/// `account_number` and `percentage` are optional (not required in every
/// jurisdiction); `amount` is the money actually due for this tax.
///
/// Spec: `specs/ocpi/2.3.0/types.asciidoc` — TaxAmount class.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaxAmount {
    /// A description of the tax (e.g. `"QST"`, `"VAT"`, `"General Sales Tax"`).
    pub name: String,
    /// Tax Account Number of the business entity remitting these taxes.
    /// Optional — not required in all countries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_number: Option<String>,
    /// Tax percentage. Optional — not required in all countries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percentage: Option<f64>,
    /// The amount of money of this tax that is due.
    pub amount: f64,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{Price, TaxAmount};

    #[test]
    fn tax_free_price_round_trips_and_omits_empty_taxes() {
        // A price with no applicable tax: `taxes` is empty, so it must not appear
        // on the wire (the 2.3.0 spec models it card. `*`).
        let p = Price {
            before_taxes: 2.00,
            taxes: vec![],
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(
            !json.contains("\"taxes\""),
            "empty taxes must be omitted on the wire: {json}"
        );
        assert!(json.contains("\"before_taxes\":2.0"));
        let back: Price = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn north_american_itemised_price_round_trips() {
        // A Canadian price with two itemised taxes (GST federal + QST provincial),
        // the shape a North-American CDR/Session cost field carries.
        let json = r#"{
            "before_taxes": 10.00,
            "taxes": [
                { "name": "GST", "percentage": 5.0, "amount": 0.50 },
                { "name": "QST", "account_number": "1234567890", "percentage": 9.975, "amount": 0.9975 }
            ]
        }"#;
        let price: Price = serde_json::from_str(json).unwrap();
        assert_eq!(price.before_taxes, 10.00);
        assert_eq!(price.taxes.len(), 2);
        assert_eq!(price.taxes[0].name, "GST");
        assert_eq!(price.taxes[0].account_number, None);
        assert_eq!(price.taxes[1].name, "QST");
        assert_eq!(price.taxes[1].account_number.as_deref(), Some("1234567890"));
        assert_eq!(price.taxes[1].percentage, Some(9.975));

        let out = serde_json::to_string(&price).unwrap();
        let back: Price = serde_json::from_str(&out).unwrap();
        assert_eq!(back, price);
    }

    #[test]
    fn tax_amount_omits_absent_optionals() {
        let t = TaxAmount {
            name: "VAT".to_string(),
            account_number: None,
            percentage: None,
            amount: 1.23,
        };
        let json = serde_json::to_string(&t).unwrap();
        assert!(!json.contains("account_number"));
        assert!(!json.contains("percentage"));
        assert!(json.contains("\"name\":\"VAT\""));
        assert!(json.contains("\"amount\":1.23"));
        let back: TaxAmount = serde_json::from_str(&json).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn price_without_before_taxes_is_rejected() {
        // `before_taxes` is required (card. 1) — a payload omitting it must fail
        // on deserialize rather than default to zero.
        let json = r#"{ "taxes": [] }"#;
        assert!(serde_json::from_str::<Price>(json).is_err());
    }

    #[test]
    fn tax_amount_without_name_or_amount_is_rejected() {
        // `name` and `amount` are required (card. 1).
        assert!(serde_json::from_str::<TaxAmount>(r#"{ "amount": 1.0 }"#).is_err());
        assert!(serde_json::from_str::<TaxAmount>(r#"{ "name": "VAT" }"#).is_err());
    }

    #[test]
    fn price_2_3_0_shape_differs_from_2_2_1_vat_price() {
        // Proof this is a genuine wire fork, not an alias: the 2.3.0 `Price` body
        // (a `before_taxes` field) is NOT a valid 2.2.1 `Price` (which requires
        // `excl_vat`), and vice-versa.
        let json_2_3_0 = r#"{ "before_taxes": 2.0 }"#;
        assert!(serde_json::from_str::<Price>(json_2_3_0).is_ok());
        assert!(serde_json::from_str::<crate::common::Price>(json_2_3_0).is_err());

        let json_2_2_1 = r#"{ "excl_vat": 2.0, "incl_vat": 2.4 }"#;
        assert!(serde_json::from_str::<crate::common::Price>(json_2_2_1).is_ok());
        assert!(serde_json::from_str::<Price>(json_2_2_1).is_err());
    }
}

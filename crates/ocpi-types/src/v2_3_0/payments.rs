//! OCPI **2.3.0** _Payments_ module — the headline addition of the release.
//!
//! The Payments module supports the **direct-payment** use case: a Payment
//! Terminal Provider (**PTP**) owns physical payment [`Terminal`]s that a CPO
//! maps to its Locations/EVSEs, and — when the CPO issues the invoice — the PTP
//! pushes a [`FinancialAdviceConfirmation`] carrying the captured amount and the
//! Electronic-Funds-Transfer (EFT) data the invoice legally requires.
//!
//! This is a brand-new 2.3.0 module (no 2.2.1 predecessor), so it is defined
//! here as a `v2_3_0`-local module rather than a re-export. Its wire identifier
//! is [`crate::version::ModuleID::Payments`] (`"payments"`).
//!
//! Spec: `specs/ocpi/2.3.0/mod_payments.asciidoc`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::common::{CiString2, CiString255, CiString3, CiString36, GeoLocation, Price, Url};

// ── Terminal ──────────────────────────────────────────────────────────────────

/// One physical payment terminal, owned and created by the **PTP**.
///
/// A Terminal establishes a mapping between charge points (Locations and/or
/// EVSEs) and a payment terminal, and configures payment-related data such as a
/// customer reference and the invoice base URL. A single terminal can serve
/// multiple Locations/EVSEs.
///
/// Only `terminal_id` and `last_updated` are required; every other field is
/// optional (an activation POST, for instance, may omit `terminal_id` itself —
/// the PTP assigns it — but this type keeps it required for the common
/// create/update/get shape, matching the object definition's cardinality).
///
/// Spec: `specs/ocpi/2.3.0/mod_payments.asciidoc` — Terminal object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Terminal {
    /// Unique ID that identifies a terminal.
    pub terminal_id: CiString36,
    /// Reference used to link the terminal to a CSMS; may also be provided via
    /// the order process.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub customer_reference: Option<CiString36>,
    /// Party ID — an alternative to `customer_reference`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub party_id: Option<CiString3>,
    /// ISO 3166-1 alpha-2 country code — an alternative to `customer_reference`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub country_code: Option<CiString2>,
    /// Street/block name and house number, if available.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub address: Option<String>,
    /// City or town.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub city: Option<String>,
    /// Postal code of the terminal; may only be omitted when the terminal has
    /// no postal code.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub postal_code: Option<String>,
    /// State or province, only to be used when relevant.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub state: Option<String>,
    /// ISO 3166-1 alpha-3 code for the country of this terminal.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub country: Option<String>,
    /// Coordinates of the terminal.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub coordinates: Option<GeoLocation>,
    /// Base URL to the downloadable invoice.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub invoice_base_url: Option<Url>,
    /// Which party creates the invoice for the eDriver.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub invoice_creator: Option<InvoiceCreator>,
    /// Mapping value as issued by the PTP (e.g. a serial number).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reference: Option<CiString36>,
    /// All Locations assigned to this terminal.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub location_ids: Vec<CiString36>,
    /// All EVSEs assigned to this terminal.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evse_uids: Vec<CiString36>,
    /// Timestamp when this Terminal was last updated (or created).
    pub last_updated: DateTime<Utc>,
}

// ── FinancialAdviceConfirmation ────────────────────────────────────────────────

/// Financial details of a transaction processed at a payment terminal.
///
/// Pushed by the **PTP** (after it captures at the PSP) when the CPO issues the
/// invoice. It correlates the payment to a charging session via the
/// `authorization_reference` shared with `Commands.StartSession`, the Session,
/// and the CDR, and carries the `eft_data` an invoice legally requires.
///
/// Spec: `specs/ocpi/2.3.0/mod_payments.asciidoc` — FinancialAdviceConfirmation
/// object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinancialAdviceConfirmation {
    /// Unique ID that identifies a financial advice confirmation.
    pub id: CiString36,
    /// Reference to the authorization given by the PTP in
    /// `Commands.StartSession`.
    pub authorization_reference: CiString36,
    /// Real amount captured at the PSP — a consumer price with VAT.
    pub total_costs: Price,
    /// ISO-4217 code of the currency of this confirmation.
    pub currency: CiString3,
    /// Invoice-relevant data from the direct payment (at least one entry).
    pub eft_data: Vec<CiString255>,
    /// Code identifying the financial-advice (capture) status.
    pub capture_status_code: CaptureStatusCode,
    /// Message about any error at the financial advice.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub capture_status_message: Option<CiString255>,
    /// Timestamp when this confirmation was last updated (or created).
    pub last_updated: DateTime<Utc>,
}

// ── InvoiceCreator ──────────────────────────────────────────────────────────────

/// Which party creates the invoice for the eDriver.
///
/// Spec: `specs/ocpi/2.3.0/mod_payments.asciidoc` — InvoiceCreator enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InvoiceCreator {
    /// The CPO issues the invoice and provides it via `invoice_base_url` +
    /// `authorization_reference`.
    Cpo,
    /// The PTP issues the invoice and shows/provides it to the eDriver via the
    /// payment terminal.
    Ptp,
}

// ── CaptureStatusCode ───────────────────────────────────────────────────────────

/// Status of the payment-capture process following a transaction at a charging
/// station.
///
/// Spec: `specs/ocpi/2.3.0/mod_payments.asciidoc` — CaptureStatusCode enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CaptureStatusCode {
    /// The payment capture completed successfully; funds were secured.
    Success,
    /// Only part of the amount was approved, or conditions were altered during
    /// processing.
    PartialSuccess,
    /// The capture attempt was unsuccessful (insufficient funds, expiry,
    /// network issues, issuer refusal, …).
    Failed,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version::ModuleID;

    #[test]
    fn module_id_payments_serializes_as_payments() {
        assert_eq!(
            serde_json::to_string(&ModuleID::Payments).unwrap(),
            "\"payments\""
        );
        assert_eq!(
            serde_json::from_str::<ModuleID>("\"payments\"").unwrap(),
            ModuleID::Payments
        );
    }

    #[test]
    fn invoice_creator_round_trips_screaming_snake() {
        assert_eq!(
            serde_json::to_string(&InvoiceCreator::Cpo).unwrap(),
            "\"CPO\""
        );
        assert_eq!(
            serde_json::to_string(&InvoiceCreator::Ptp).unwrap(),
            "\"PTP\""
        );
        assert_eq!(
            serde_json::from_str::<InvoiceCreator>("\"PTP\"").unwrap(),
            InvoiceCreator::Ptp
        );
    }

    #[test]
    fn capture_status_code_round_trips_screaming_snake() {
        for (value, wire) in [
            (CaptureStatusCode::Success, "\"SUCCESS\""),
            (CaptureStatusCode::PartialSuccess, "\"PARTIAL_SUCCESS\""),
            (CaptureStatusCode::Failed, "\"FAILED\""),
        ] {
            assert_eq!(serde_json::to_string(&value).unwrap(), wire);
            assert_eq!(
                serde_json::from_str::<CaptureStatusCode>(wire).unwrap(),
                value
            );
        }
    }

    #[test]
    fn minimal_terminal_round_trips_and_omits_empty_fields() {
        // The minimal create shape (spec: "Create a minimal Terminal"): just the
        // id and the timestamp — every optional field and both lists are absent.
        let json = r#"{
            "terminal_id": "12345678",
            "last_updated": "2024-01-01T00:00:00Z"
        }"#;
        let terminal: Terminal = serde_json::from_str(json).unwrap();
        assert_eq!(terminal.terminal_id.as_str(), "12345678");
        assert!(terminal.customer_reference.is_none());
        assert!(terminal.location_ids.is_empty());
        assert!(terminal.evse_uids.is_empty());

        // Absent optionals / empty lists must not be emitted on the wire.
        let out = serde_json::to_string(&terminal).unwrap();
        assert!(
            !out.contains("customer_reference"),
            "should omit absent option: {out}"
        );
        assert!(
            !out.contains("location_ids"),
            "should omit empty list: {out}"
        );
        assert!(!out.contains("evse_uids"), "should omit empty list: {out}");
        let back: Terminal = serde_json::from_str(&out).unwrap();
        assert_eq!(back, terminal);
    }

    #[test]
    fn full_terminal_with_assigned_locations_and_evses_round_trips() {
        // A fully-populated terminal (spec: "Terminal with assigned locations
        // and EVSEs"): the full address block, coordinates, the invoice
        // configuration, and both assignment lists.
        let json = r#"{
            "terminal_id": "55719888-ed09-4cca-82cc-803bdb77bf26",
            "customer_reference": "CUSTOMER-42",
            "party_id": "ABC",
            "country_code": "NL",
            "address": "F.Rooseveltlaan 3A",
            "city": "Kerkrade",
            "postal_code": "6419",
            "state": "Limburg",
            "country": "NLD",
            "coordinates": {
                "latitude": "50.770774",
                "longitude": "-126.104965"
            },
            "invoice_base_url": "https://www.server.com/invoices",
            "invoice_creator": "CPO",
            "reference": "SN-000123",
            "location_ids": ["LOC1", "LOC2"],
            "evse_uids": ["EVSE1", "EVSE2", "EVSE3"],
            "last_updated": "2024-01-02T10:15:30Z"
        }"#;
        let terminal: Terminal = serde_json::from_str(json).unwrap();
        assert_eq!(terminal.invoice_creator, Some(InvoiceCreator::Cpo));
        assert_eq!(terminal.location_ids.len(), 2);
        assert_eq!(terminal.evse_uids.len(), 3);
        assert_eq!(terminal.coordinates.as_ref().unwrap().latitude, "50.770774");

        let out = serde_json::to_string(&terminal).unwrap();
        let back: Terminal = serde_json::from_str(&out).unwrap();
        assert_eq!(back, terminal);
    }

    #[test]
    fn financial_advice_confirmation_success_round_trips() {
        // A successful capture (spec: "successful capture at the PSP"): the
        // total price with VAT, the EFT data list, and a SUCCESS status.
        let json = r#"{
            "id": "fac-0001",
            "authorization_reference": "AUTH-REF-123",
            "total_costs": { "excl_vat": 8.50, "incl_vat": 10.29 },
            "currency": "EUR",
            "eft_data": ["****1234", "PSP-TXN-9988"],
            "capture_status_code": "SUCCESS",
            "last_updated": "2024-01-03T12:00:00Z"
        }"#;
        let fac: FinancialAdviceConfirmation = serde_json::from_str(json).unwrap();
        assert_eq!(fac.capture_status_code, CaptureStatusCode::Success);
        assert_eq!(fac.total_costs.incl_vat, Some(10.29));
        assert_eq!(fac.eft_data.len(), 2);
        assert!(fac.capture_status_message.is_none());

        let out = serde_json::to_string(&fac).unwrap();
        assert!(
            !out.contains("capture_status_message"),
            "omit absent option: {out}"
        );
        let back: FinancialAdviceConfirmation = serde_json::from_str(&out).unwrap();
        assert_eq!(back, fac);
    }

    #[test]
    fn financial_advice_confirmation_failure_carries_message() {
        // An unsuccessful capture (spec: "unsuccessful capture at the PSP")
        // carries a FAILED status and an explanatory message.
        let json = r#"{
            "id": "fac-0002",
            "authorization_reference": "AUTH-REF-456",
            "total_costs": { "excl_vat": 0.0 },
            "currency": "EUR",
            "eft_data": ["DECLINED"],
            "capture_status_code": "FAILED",
            "capture_status_message": "Card issuer refused the transaction",
            "last_updated": "2024-01-03T12:05:00Z"
        }"#;
        let fac: FinancialAdviceConfirmation = serde_json::from_str(json).unwrap();
        assert_eq!(fac.capture_status_code, CaptureStatusCode::Failed);
        assert_eq!(
            fac.capture_status_message.as_ref().unwrap().as_str(),
            "Card issuer refused the transaction"
        );
        assert!(fac.total_costs.incl_vat.is_none());

        let out = serde_json::to_string(&fac).unwrap();
        let back: FinancialAdviceConfirmation = serde_json::from_str(&out).unwrap();
        assert_eq!(back, fac);
    }
}

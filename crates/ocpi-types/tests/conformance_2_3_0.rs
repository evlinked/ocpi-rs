//! **Conformance corpus — OCPI 2.3.0** (milestone **M9**; issues #226 → #234).
//!
//! Extends the 2.2.1 conformance corpus (#226) to OCPI **2.3.0**, the M8
//! close-out version with the richest genuine wire deltas over 2.2.1: the new
//! Payments module, the North-American tax rework on Tariffs / CDRs / Sessions,
//! the `hub_party_id` Credentials, and the Parking-bearing Location. Each
//! fixture under [`conformance/2.3.0/<module>/<name>.json`](../../../conformance/2.3.0/)
//! is a spec-example payload transcribed from `specs/ocpi/2.3.0/*.asciidoc`
//! (via the per-module unit tests that already carry them); the invariant is the
//! same `T → JSON → T` serde round-trip the 2.2.1 harness asserts.
//!
//! Coverage is deliberately the 2.3.0 *delta* surface — the modules whose wire
//! shape genuinely differs from 2.2.1 — since the wire-identical modules
//! (Tokens, Commands, ChargingProfiles, HubClientInfo, Versions) are
//! byte-for-byte re-exports already exercised by the 2.2.1 corpus and the
//! `reuse_types_stay_aliases_of_2_2_1` compile-time identity in
//! `crates/ocpi-types/src/v2_3_0/mod.rs`.

mod corpus_common;

use corpus_common::round_trip;
use ocpi_types::v2_3_0::{
    Cdr, Credentials, FinancialAdviceConfirmation, Location, Session, Tariff, Terminal,
};

/// One assertion per 2.3.0 delta-surface spec example. `include_str!` embeds
/// each fixture at compile time, so a deleted or renamed corpus file breaks the
/// build rather than silently dropping coverage.
#[test]
fn conformance_corpus_2_3_0_round_trips() {
    // tariffs — North-American tax rework (specs/ocpi/2.3.0/mod_tariffs.asciidoc
    // §Simple Tariff with North American taxes): `tax_included`.
    round_trip::<Tariff>(
        "2.3.0 tariffs/tax_included",
        include_str!("../../../conformance/2.3.0/tariffs/tax_included.json"),
    );

    // payments — the new-in-2.3.0 module (specs/ocpi/2.3.0/mod_payments.asciidoc
    // §Terminal / §FinancialAdviceConfirmation).
    round_trip::<Terminal>(
        "2.3.0 payments/terminal",
        include_str!("../../../conformance/2.3.0/payments/terminal.json"),
    );
    round_trip::<FinancialAdviceConfirmation>(
        "2.3.0 payments/financial_advice",
        include_str!("../../../conformance/2.3.0/payments/financial_advice.json"),
    );

    // credentials — hub_party_id + hub role as a normal credentials role
    // (specs/ocpi/2.3.0/credentials.asciidoc §Credentials object).
    round_trip::<Credentials>(
        "2.3.0 credentials/hub_party_id",
        include_str!("../../../conformance/2.3.0/credentials/hub_party_id.json"),
    );

    // cdrs / sessions — the reworked Price (before_taxes + itemised TaxAmount)
    // (specs/ocpi/2.3.0/mod_cdrs.asciidoc / mod_sessions.asciidoc).
    round_trip::<Cdr>(
        "2.3.0 cdrs/na_taxed",
        include_str!("../../../conformance/2.3.0/cdrs/na_taxed.json"),
    );
    round_trip::<Session>(
        "2.3.0 sessions/na_taxed",
        include_str!("../../../conformance/2.3.0/sessions/na_taxed.json"),
    );

    // locations — the Parking object + help_phone Location fork
    // (specs/ocpi/2.3.0/mod_locations.asciidoc §Location object). This is the
    // Location/EVSE/Connector object class #226 left uncovered in every version.
    round_trip::<Location>(
        "2.3.0 locations/location",
        include_str!("../../../conformance/2.3.0/locations/location.json"),
    );
}

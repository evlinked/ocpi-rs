//! **Conformance corpus — OCPI 2.2** (milestone **M9**; issues #226 → #234).
//!
//! Extends the 2.2.1 conformance corpus (#226) to OCPI **2.2**. 2.2 is a mostly
//! wire-identical predecessor of 2.2.1; its genuine deltas are subtractive — the
//! `CdrToken` without 2.2.1's routing `country_code` / `party_id`, and the
//! `StartSession` without 2.2.1's `connector_id`. Each fixture under
//! [`conformance/2.2/<module>/<name>.json`](../../../conformance/2.2/) is a
//! spec-example payload transcribed from the 2.2 module unit tests; the invariant
//! is the same `T → JSON → T` serde round-trip the 2.2.1 harness asserts.
//!
//! Only the delta modules are covered — the wire-identical 2.2 surface is a set
//! of re-exports already exercised by the 2.2.1 corpus.

mod corpus_common;

use corpus_common::round_trip;
use ocpi_types::v2_2::{CdrToken, StartSession};

/// One assertion per 2.2 delta-surface spec example. `include_str!` embeds each
/// fixture at compile time, so a deleted or renamed corpus file breaks the build
/// rather than silently dropping coverage.
#[test]
fn conformance_corpus_2_2_round_trips() {
    // cdrs — the 2.2 CdrToken (no 2.2.1 country_code / party_id).
    round_trip::<CdrToken>(
        "2.2 cdrs/cdr_token",
        include_str!("../../../conformance/2.2/cdrs/cdr_token.json"),
    );

    // commands — the 2.2 StartSession (no 2.2.1 connector_id).
    round_trip::<StartSession>(
        "2.2 commands/start_session",
        include_str!("../../../conformance/2.2/commands/start_session.json"),
    );
}

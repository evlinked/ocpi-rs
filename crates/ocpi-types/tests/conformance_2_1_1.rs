//! **Conformance corpus — OCPI 2.1.1** (milestone **M9**; issues #226 → #234).
//!
//! Extends the 2.2.1 conformance corpus (#226) to OCPI **2.1.1**, the legacy
//! version whose wire shape differs most sharply from 2.2.1: the *flat*
//! `Cdr` / `Session` (an embedded full `location` object, `auth_id` instead of a
//! `cdr_token`, `stop_date_time` / `start_datetime` field names, bare numeric
//! cost fields) and the `auth_id`-keyed `Token`. Each fixture under
//! [`conformance/2.1.1/<module>/<name>.json`](../../../conformance/2.1.1/) is a
//! spec-example payload transcribed from `specs/ocpi/2.1.1` (via the per-module
//! unit tests that already carry it); the invariant is the same `T → JSON → T`
//! serde round-trip the 2.2.1 harness asserts.
//!
//! The `Cdr` / `Session` fixtures each embed a full 2.1.1 `Location` (with an
//! `Evse` + `Connector` in the CDR), so the Location object class is round-tripped
//! for 2.1.1 here rather than as a standalone fixture.

mod corpus_common;

use corpus_common::round_trip;
use ocpi_types::v2_1_1::{Cdr, Session, Token};

/// One assertion per 2.1.1 delta-surface spec example. `include_str!` embeds
/// each fixture at compile time, so a deleted or renamed corpus file breaks the
/// build rather than silently dropping coverage.
#[test]
fn conformance_corpus_2_1_1_round_trips() {
    // cdrs — the flat 2.1.1 CDR (embedded location, auth_id, stop_date_time,
    // bare numeric costs) — specs/ocpi/2.1.1 §CDR Object.
    round_trip::<Cdr>(
        "2.1.1 cdrs/cdr",
        include_str!("../../../conformance/2.1.1/cdrs/cdr.json"),
    );

    // sessions — the flat 2.1.1 Session (embedded location, auth_id,
    // start_datetime) — specs/ocpi/2.1.1 §Session Object.
    round_trip::<Session>(
        "2.1.1 sessions/session",
        include_str!("../../../conformance/2.1.1/sessions/session.json"),
    );

    // tokens — the auth_id-keyed 2.1.1 RFID Token — specs/ocpi/2.1.1 §Token Object.
    round_trip::<Token>(
        "2.1.1 tokens/rfid",
        include_str!("../../../conformance/2.1.1/tokens/rfid.json"),
    );
}

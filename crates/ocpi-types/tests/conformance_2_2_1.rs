//! **Conformance corpus — OCPI 2.2.1** (milestone **M9**, the *conformance*
//! roadmap item; issue #225).
//!
//! Every vendored spec-example payload for 2.2.1 lives as a standalone fixture
//! under [`conformance/2.2.1/<module>/<name>.json`](../../../conformance/) —
//! sourced from the example payloads in `specs/ocpi/2.2.1/*.asciidoc` (see
//! `conformance/README.md` for the per-fixture provenance map). Previously these
//! examples were transcribed inline, scattered across each module's own unit
//! tests; this harness consolidates them into one spec-organised corpus so a
//! missing or drifted example is caught centrally.
//!
//! The invariant every fixture asserts is a **serde round-trip**: the example
//! deserializes into the matching typed model, re-serializes, and deserializes
//! again to an *equal* value (`T → JSON → T` stability). This is faithful to the
//! wire without being brittle about field ordering or the unknown-field
//! tolerance the crate deliberately keeps (a raw JSON string-compare would fight
//! both). A fixture the crate cannot faithfully represent fails CI with a clear,
//! per-fixture message naming the file.
//!
//! Scope: **2.2.1** (the primary production target), per #225's "land 2.2.1
//! first" guard. The 2.1.1 / 2.2 / 2.3.0 corpora (#234) live in sibling
//! harnesses (`conformance_2_1_1.rs`, `conformance_2_2.rs`,
//! `conformance_2_3_0.rs`) and share the [`corpus_common::round_trip`] helper.

mod corpus_common;

use corpus_common::round_trip;
use ocpi_types::v2_2_1::{
    Cdr, CdrToken, ClientInfo, Credentials, Session, SetChargingProfile, StartSession, Tariff,
    Token,
};

/// One assertion per vendored 2.2.1 spec example. `include_str!` embeds each
/// fixture at compile time (no runtime CWD dependency), so a deleted or renamed
/// corpus file breaks the build rather than silently skipping coverage.
#[test]
fn conformance_corpus_2_2_1_round_trips() {
    // credentials — specs/ocpi/2.2.1/credentials.asciidoc §Credentials object
    round_trip::<Credentials>(
        "credentials/minimal_cpo",
        include_str!("../../../conformance/2.2.1/credentials/minimal_cpo.json"),
    );
    round_trip::<Credentials>(
        "credentials/multi_role",
        include_str!("../../../conformance/2.2.1/credentials/multi_role.json"),
    );

    // cdrs — specs/ocpi/2.2.1/mod_cdrs.asciidoc §CDR object / §CdrToken
    round_trip::<Cdr>(
        "cdrs/cdr",
        include_str!("../../../conformance/2.2.1/cdrs/cdr.json"),
    );
    round_trip::<CdrToken>(
        "cdrs/cdr_token",
        include_str!("../../../conformance/2.2.1/cdrs/cdr_token.json"),
    );

    // sessions — specs/ocpi/2.2.1/mod_sessions.asciidoc §Session object
    round_trip::<Session>(
        "sessions/simple_start",
        include_str!("../../../conformance/2.2.1/sessions/simple_start.json"),
    );

    // tariffs — specs/ocpi/2.2.1/mod_tariffs.asciidoc §Tariff object
    round_trip::<Tariff>(
        "tariffs/tariff",
        include_str!("../../../conformance/2.2.1/tariffs/tariff.json"),
    );

    // tokens — specs/ocpi/2.2.1/mod_tokens.asciidoc §Token object
    round_trip::<Token>(
        "tokens/app_user",
        include_str!("../../../conformance/2.2.1/tokens/app_user.json"),
    );
    round_trip::<Token>(
        "tokens/full_rfid",
        include_str!("../../../conformance/2.2.1/tokens/full_rfid.json"),
    );

    // commands — specs/ocpi/2.2.1/mod_commands.asciidoc §StartSession / §SetChargingProfile
    round_trip::<StartSession>(
        "commands/start_session",
        include_str!("../../../conformance/2.2.1/commands/start_session.json"),
    );
    round_trip::<SetChargingProfile>(
        "commands/set_charging_profile",
        include_str!("../../../conformance/2.2.1/commands/set_charging_profile.json"),
    );

    // hub_client_info — specs/ocpi/2.2.1/mod_hub_client_info.asciidoc §ClientInfo object
    round_trip::<ClientInfo>(
        "hub_client_info/client_info",
        include_str!("../../../conformance/2.2.1/hub_client_info/client_info.json"),
    );
}

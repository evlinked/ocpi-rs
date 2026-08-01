//! Shared harness helper for the multi-version conformance corpus (milestone
//! **M9**, issues #225/#226 → #234).
//!
//! Each per-version harness (`conformance_2_1_1.rs`, `conformance_2_2.rs`,
//! `conformance_2_2_1.rs`, `conformance_2_3_0.rs`) is its own integration-test
//! binary, so this module is compiled into each of them; it lives in a
//! subdirectory of `tests/` precisely so Cargo does *not* treat it as a
//! standalone test binary. Every harness `include_str!`s its own fixtures from
//! [`conformance/<version>/…`](../../../../conformance/) and drives them through
//! [`round_trip`].

use std::fmt::Debug;

use ocpi_types::serde::{de::DeserializeOwned, Serialize};
use ocpi_types::serde_json;

/// Deserialize `json` into `T`, re-serialize, and deserialize again — asserting
/// the two typed values are equal (`T → JSON → T` stability). `fixture` names
/// the corpus file so a failure points straight at the offending example.
///
/// This is faithful to the wire without being brittle about field ordering or
/// the unknown-field tolerance the crate deliberately keeps (2.3.0 `SHALL NOT
/// reject on unknown fields`, #184) — a raw JSON string-compare would fight
/// both.
pub fn round_trip<T>(fixture: &str, json: &str)
where
    T: DeserializeOwned + Serialize + PartialEq + Debug,
{
    let first: T = serde_json::from_str(json).unwrap_or_else(|e| {
        panic!(
            "[{fixture}] does not deserialize into {}: {e}",
            type_of::<T>()
        )
    });
    let reserialized = serde_json::to_string(&first)
        .unwrap_or_else(|e| panic!("[{fixture}] re-serialization failed: {e}"));
    let second: T = serde_json::from_str(&reserialized)
        .unwrap_or_else(|e| panic!("[{fixture}] re-deserialization failed: {e}"));
    assert_eq!(
        first,
        second,
        "[{fixture}] round-trip is not stable for {}",
        type_of::<T>()
    );
}

fn type_of<T>() -> &'static str {
    std::any::type_name::<T>()
}

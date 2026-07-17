#![no_main]
//! Fuzz the 2.2.1 `Token` deserialize boundary.
//!
//! Invariant: never panics — `Ok(_)` or a clean `Err(_)` only. See
//! `fuzz/README.md`.

use libfuzzer_sys::fuzz_target;
use ocpi_types::Token;

fuzz_target!(|data: &[u8]| {
    let _ = serde_json::from_slice::<Token>(data);
});

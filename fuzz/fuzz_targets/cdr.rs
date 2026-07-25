#![no_main]
//! Fuzz the 2.2.1 `Cdr` deserialize boundary — a billing record a Hub relays
//! between partners, so a malformed one must degrade cleanly, never crash.
//!
//! Invariant: never panics — `Ok(_)` or a clean `Err(_)` only. See
//! `fuzz/README.md`.

use libfuzzer_sys::fuzz_target;
use ocpi_types::Cdr;

fuzz_target!(|data: &[u8]| {
    let _ = serde_json::from_slice::<Cdr>(data);
});

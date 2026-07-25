#![no_main]
//! Fuzz the `/versions` catalogue — `Vec<Version>`.
//!
//! This is the exact surface the 3.0-recognition slice (#219) hardened: a
//! partner's version list is the first thing negotiation parses, and a single
//! malformed or forward-looking entry must never crash the deserializer.
//!
//! Invariant: never panics — `Ok(_)` or a clean `Err(_)` only. See
//! `fuzz/README.md`.

use libfuzzer_sys::fuzz_target;
use ocpi_types::Version;

fuzz_target!(|data: &[u8]| {
    let _ = serde_json::from_slice::<Vec<Version>>(data);
});

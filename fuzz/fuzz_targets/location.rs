#![no_main]
//! Fuzz the 2.2.1 `Location` deserialize boundary — the largest composite a
//! CPO pushes to an eMSP (nested EVSEs, connectors, geo-coordinates, hours).
//!
//! Invariant: deserializing untrusted JSON must **never panic** — `Ok(_)` or a
//! clean `Err(_)` only, however deeply nested or malformed. See
//! `fuzz/README.md`.

use libfuzzer_sys::fuzz_target;
use ocpi_types::Location;

fuzz_target!(|data: &[u8]| {
    let _ = serde_json::from_slice::<Location>(data);
});

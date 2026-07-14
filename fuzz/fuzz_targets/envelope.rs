#![no_main]
//! Fuzz the OCPI response envelope — the outermost deserialize entry point.
//!
//! Every OCPI endpoint wraps its payload in `{status_code, status_message,
//! timestamp, data}`. This target drives that envelope over an opaque `data`
//! payload so the fuzzer exercises the envelope's own parsing (the integer
//! `status_code`, the RFC 3339 `timestamp`) independent of any inner object.
//!
//! Invariant: deserializing untrusted JSON off the wire must **never panic**.
//! However malformed, truncated, or adversarial the bytes, `serde_json` must
//! resolve to `Ok(_)` or a clean `Err(_)` — never a crash, integer overflow,
//! or hang. See `fuzz/README.md`.

use libfuzzer_sys::fuzz_target;
use ocpi_types::OcpiResponse;

fuzz_target!(|data: &[u8]| {
    let _ = serde_json::from_slice::<OcpiResponse<serde_json::Value>>(data);
});

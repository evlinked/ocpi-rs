//! M8 (OCPI 2.3.0) forward-compatibility guard — **unknown JSON fields are
//! tolerated, never rejected** (issue #184, Part 1).
//!
//! `specs/ocpi/2.3.0/transport_and_format.asciidoc` ("Non-specified JSON
//! fields") makes this a hard normative requirement:
//!
//! > An OCPI Platform **SHALL NOT** reject request or response payloads based on
//! > the presence of JSON object field names that are not documented in this
//! > specification.
//!
//! `ocpi-types` already satisfies this: no type carries
//! `#[serde(deny_unknown_fields)]`, so serde ignores undocumented fields by
//! default. That tolerance is a *deliberate conformance property*, not an
//! accident — but nothing in the type layer structurally prevents a future
//! change from adding `deny_unknown_fields` to a struct and silently breaking
//! the `SHALL NOT`. This test is that regression fence: it fails CI the moment
//! any representative type starts rejecting an extra field.
//!
//! It is the trust-boundary companion to the crate's strict-rejection promise:
//! the unsupported case (a malformed value, a missing required field, an unknown
//! *enum* value) is still rejected explicitly, but an unknown *field* — which
//! the spec explicitly reserves room for so implementers can extend OCPI — is
//! preserved-by-ignoring, exactly as a roaming Hub relaying between partners on
//! different patch levels needs.

use ocpi_types::v2_3_0::Terminal;
use ocpi_types::{GeoLocation, OcpiResponse, OcpiStatusCode};

/// Inject an undocumented field into a JSON object body and assert the type
/// still deserializes to the *same* value it does without the extra field.
fn assert_tolerates_unknown_field<T>(clean: &str, with_extra: &str)
where
    T: serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let from_clean: T =
        serde_json::from_str(clean).expect("the documented-only body must deserialize");
    let from_extra: T = serde_json::from_str(with_extra)
        .expect("an extra undocumented field must NOT cause rejection (2.3.0 SHALL NOT)");
    assert_eq!(
        from_clean, from_extra,
        "the unknown field must be ignored, leaving the parsed value unchanged"
    );
}

#[test]
fn shared_common_type_ignores_unknown_field() {
    // `GeoLocation` stands in for the shared `common` types every module embeds.
    assert_tolerates_unknown_field::<GeoLocation>(
        r#"{"latitude":"50.770774","longitude":"-126.104965"}"#,
        r#"{"latitude":"50.770774","longitude":"-126.104965","altitude":"12.0"}"#,
    );
}

#[test]
fn v2_3_0_local_type_ignores_unknown_field() {
    // `Terminal` stands in for a `v2_3_0`-local delta type — the newest surface,
    // the one most likely to meet a forward-looking peer's extension field.
    assert_tolerates_unknown_field::<Terminal>(
        r#"{"terminal_id":"term-0001","last_updated":"2026-01-01T00:00:00Z"}"#,
        r#"{"terminal_id":"term-0001","last_updated":"2026-01-01T00:00:00Z","manufacturer":"ACME"}"#,
    );
}

#[test]
fn response_envelope_ignores_unknown_field_at_both_levels() {
    // The envelope is the outermost wrapper a hub parses; an extension can ride
    // either on the envelope itself or inside `data`. Both must be ignored.
    assert_tolerates_unknown_field::<OcpiResponse<GeoLocation>>(
        r#"{"data":{"latitude":"50.770774","longitude":"-126.104965"},"status_code":1000,"timestamp":"2026-01-01T00:00:00Z"}"#,
        r#"{"data":{"latitude":"50.770774","longitude":"-126.104965","altitude":"12.0"},"status_code":1000,"timestamp":"2026-01-01T00:00:00Z","x_vendor_trace":"abc123"}"#,
    );
}

#[test]
fn unknown_field_alongside_a_valid_status_code_still_parses() {
    // Belt-and-braces: the ignored field does not perturb the status code, so a
    // relayed error envelope keeps its meaning even when it carries an extension.
    let resp: OcpiResponse<GeoLocation> = serde_json::from_str(
        r#"{"status_code":2001,"timestamp":"2026-01-01T00:00:00Z","hint":"unknown to us"}"#,
    )
    .expect("an unknown field must not defeat envelope parsing");
    assert_eq!(resp.status_code, OcpiStatusCode::InvalidParameters);
    assert!(resp.data.is_none());
}

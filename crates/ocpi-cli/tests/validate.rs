//! Integration tests for the `ocpi` CLI.
//!
//! The `validate` subcommand is the one command that runs without a network
//! peer, so it is exercised here end-to-end by spawning the built `ocpi`
//! binary (`CARGO_BIN_EXE_ocpi`) against committed fixtures. This gives the
//! otherwise-untested `ocpi-cli` crate real coverage and pins the trust
//! boundary the charter cares about: malformed or non-envelope JSON is
//! rejected with a clean non-zero exit — never a panic.
//!
//! Fixtures are transcribed from `specs/ocpi/2.2.1`:
//! - `error_2001.json` is the verbatim error-envelope example from
//!   `transport_and_format.asciidoc` ("Response with an error").
//! - `versions_success.json` is a `GET /versions` success envelope shaped per
//!   `version_information_endpoint.asciidoc` (a list of `{version, url}`).

use std::path::PathBuf;
use std::process::{Command, Output};

/// A `Command` targeting the freshly-built `ocpi` binary.
fn ocpi() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ocpi"))
}

/// Absolute path to a committed fixture under `tests/fixtures/`.
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Run `ocpi validate <fixture>` and capture its output.
fn validate(name: &str) -> Output {
    ocpi()
        .arg("validate")
        .arg(fixture(name))
        .output()
        .expect("failed to spawn the ocpi binary")
}

#[test]
fn validate_accepts_success_versions_envelope() {
    let out = validate("versions_success.json");
    assert!(
        out.status.success(),
        "expected exit 0, got {:?}",
        out.status
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("valid OCPI envelope"), "stdout: {stdout}");
    assert!(stdout.contains("status_code=1000"), "stdout: {stdout}");
    assert!(stdout.contains("success=true"), "stdout: {stdout}");
}

#[test]
fn validate_accepts_error_envelope_but_reports_not_success() {
    // A well-formed *error* envelope is still a structurally valid envelope;
    // `validate` checks the shape, not the success bit — so it exits 0 and
    // reports `success=false` for a `2001` status code.
    let out = validate("error_2001.json");
    assert!(
        out.status.success(),
        "expected exit 0, got {:?}",
        out.status
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("status_code=2001"), "stdout: {stdout}");
    assert!(stdout.contains("success=false"), "stdout: {stdout}");
}

#[test]
fn validate_rejects_json_that_is_not_an_envelope() {
    // Valid JSON, but missing the required `status_code`/`timestamp` fields:
    // the command must fail cleanly (non-zero exit), never panic.
    let out = validate("not_an_envelope.json");
    assert!(!out.status.success(), "expected a non-zero exit");
}

#[test]
fn validate_rejects_malformed_json() {
    // Truncated / syntactically invalid JSON: a clean non-zero exit, no panic.
    let out = validate("malformed.json");
    assert!(!out.status.success(), "expected a non-zero exit");
}

#[test]
fn validate_rejects_missing_file() {
    let out = validate("does_not_exist.json");
    assert!(
        !out.status.success(),
        "expected a non-zero exit for a missing file"
    );
}

#[test]
fn cli_reports_its_version() {
    let out = ocpi()
        .arg("--version")
        .output()
        .expect("failed to spawn the ocpi binary");
    assert!(out.status.success(), "`--version` should exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("ocpi"), "stdout: {stdout}");
}

#[test]
fn invoking_without_a_subcommand_is_a_usage_error() {
    // clap requires a subcommand; with none it exits non-zero with usage help.
    let out = ocpi().output().expect("failed to spawn the ocpi binary");
    assert!(!out.status.success(), "expected a non-zero usage exit");
}

//! Contract test for the OCPI **2.1.1** credentials registration handshake
//! (the *Token A → B → C* exchange), driven through the `ocpi-types` crate.
//!
//! Mirrors `credentials_handshake_2_2_1.rs`, but for the **flat** 2.1.1
//! credentials object (no `roles` array, no `OCPI-to/from-*` routing headers).
//! This is what `evlinked/charge-hub` needs to roam with CPOs/eMSPs still on
//! 2.1.1.
//!
//! The 2.1.1 types are version-namespaced (`ocpi_types::v2_1_1::…`) rather than
//! root-exported, because the primary 2.2.1 `Credentials` already owns the
//! crate-root name. The shared envelope/status/common types come from the root.
//!
//! Spec: OCPI 2.1.1 — *Credentials* module / *Registration* use-case
//! (`specs/ocpi/2.1.1/OCPI_2.1.1.pdf`).

use ocpi_types::v2_1_1::Credentials;
use ocpi_types::{BusinessDetails, CiString2, CiString3, OcpiResponse, OcpiStatusCode, Url};

/// Build a flat 2.1.1 credentials object, as it appears on the wire during the
/// handshake.
fn credentials_for(token: &str, party: &str, country: &str, name: &str) -> Credentials {
    Credentials {
        token: token.to_owned(),
        url: Url::try_from("https://example.com/ocpi/versions").unwrap(),
        business_details: BusinessDetails {
            name: name.to_owned(),
            website: None,
            logo: None,
        },
        party_id: CiString3::try_from(party).unwrap(),
        country_code: CiString2::try_from(country).unwrap(),
    }
}

/// Step 1 — the new party (eMSP), authenticating with **Token A**, POSTs its
/// own flat credentials carrying **Token B**. We model the receiver parsing
/// that POST body off the wire.
#[test]
fn step_post_credentials_carries_token_b() {
    // Exactly the flat body an eMSP would POST to `…/credentials` in 2.1.1.
    let posted = r#"{
        "token": "TOKEN_B_emsp_to_cpo",
        "url": "https://emsp.example.com/ocpi/versions",
        "business_details": {"name": "Example Provider"},
        "party_id": "MSP",
        "country_code": "DE"
    }"#;

    let creds: Credentials = serde_json::from_str(posted).unwrap();
    assert_eq!(creds.token, "TOKEN_B_emsp_to_cpo");
    assert_eq!(creds.party_id.as_str(), "MSP");
    assert_eq!(creds.country_code.as_str(), "DE");
}

/// Step 2 — the receiver (CPO) responds with **its own** flat credentials
/// wrapped in the OCPI envelope, carrying **Token C** for the eMSP henceforth.
#[test]
fn step_response_carries_token_c_in_envelope() {
    let cpo = credentials_for("TOKEN_C_cpo_to_emsp", "EXA", "NL", "Example Operator");
    let envelope = OcpiResponse::success(cpo);

    let on_wire = serde_json::to_string(&envelope).unwrap();
    let parsed: OcpiResponse<Credentials> = serde_json::from_str(&on_wire).unwrap();

    assert!(parsed.is_success());
    assert_eq!(parsed.status_code, OcpiStatusCode::Success);
    let data = parsed
        .data
        .expect("success envelope must carry credentials");
    assert_eq!(data.token, "TOKEN_C_cpo_to_emsp");
    assert_eq!(data.party_id.as_str(), "EXA");
}

/// The handshake's security property: B (eMSP → CPO) and C (CPO → eMSP) are
/// distinct, and neither is the bootstrap Token A. After the exchange A is
/// discarded.
#[test]
fn token_a_b_c_are_distinct() {
    let token_a = "TOKEN_A_preshared";
    let emsp_post = credentials_for("TOKEN_B", "MSP", "DE", "Example Provider");
    let cpo_reply = credentials_for("TOKEN_C", "EXA", "NL", "Example Operator");

    assert_ne!(token_a, emsp_post.token, "B must differ from bootstrap A");
    assert_ne!(emsp_post.token, cpo_reply.token, "B and C must differ");
    assert_ne!(token_a, cpo_reply.token);
}

/// Step 3 — credentials **update** (PUT) rotates the token; party identity is
/// stable across the rotation.
#[test]
fn step_update_rotates_token() {
    let original = credentials_for("TOKEN_C", "EXA", "NL", "Example Operator");
    let rotated = credentials_for("TOKEN_C_v2", "EXA", "NL", "Example Operator");

    let json = serde_json::to_string(&rotated).unwrap();
    let back: Credentials = serde_json::from_str(&json).unwrap();
    assert_eq!(back, rotated);
    assert_ne!(original.token, rotated.token, "PUT must rotate the token");
    assert_eq!(original.party_id, rotated.party_id);
}

/// Step 4 — **unregister** (DELETE): the receiver acknowledges with a data-less
/// success envelope. `data` is omitted entirely, not `null`.
#[test]
fn step_delete_returns_empty_success_envelope() {
    let envelope: OcpiResponse<()> = OcpiResponse::success_empty();
    let on_wire = serde_json::to_string(&envelope).unwrap();
    assert!(
        !on_wire.contains("\"data\""),
        "empty envelope must omit data: {on_wire}"
    );

    let parsed: OcpiResponse<()> = serde_json::from_str(&on_wire).unwrap();
    assert!(parsed.is_success());
    assert_eq!(parsed.status_code, OcpiStatusCode::Success);
}

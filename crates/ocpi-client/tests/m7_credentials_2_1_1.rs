//! M7 end-to-end registration smoke test for **OCPI 2.1.1** (issue #112).
//!
//! Walks the flat 2.1.1 *Token A → B → C* credentials handshake over a real
//! loopback transport: an in-process `axum` server hosts the 2.1.1
//! `credentials_2_1_1_router`, driven entirely through the new
//! [`OcpiClient`] 2.1.1 sender methods (`register_2_1_1`,
//! `get_credentials_2_1_1`, `update_credentials_2_1_1`, and the
//! version-agnostic `delete_credentials`).
//!
//! Unlike the 2.2.1 flow ([`m2_registration`]), 2.1.1 does **no** fetch-back
//! in this slice, so there is a single party B and no `/versions` server on the
//! sender side. The credentials object is the *flat* 2.1.1 shape — no `roles`
//! array.
//!
//! This is the live transport counterpart to the data-contract test in
//! `ocpi-types/tests/credentials_handshake_2_1_1.rs`.
//!
//! Spec: OCPI 2.1.1 — *Credentials* module / *Registration* use-case.

use std::sync::Arc;

use ocpi_client::OcpiClient;
use ocpi_server::{http::credentials_2_1_1_router, Credentials2111Config};
use ocpi_types::v2_1_1::Credentials;
use ocpi_types::{BusinessDetails, CiString2, CiString3, Url as OcpiUrl};

/// The bootstrap token (Token A) the CPO pre-shares with the eMSP out of band.
const TOKEN_A: &str = "TOKEN_A_bootstrap";
/// The token the eMSP puts in its POST body for the CPO to call back with.
const TOKEN_B: &str = "TOKEN_B_cpo_calls_emsp";
/// The token the CPO issues to the eMSP in its registration response.
const TOKEN_C: &str = "TOKEN_C_emsp_calls_cpo";

/// Build a flat 2.1.1 credentials object, as it appears on the wire.
fn credentials(token: &str, versions_url: &str, party: &str, country: &str) -> Credentials {
    Credentials {
        token: token.to_owned(),
        url: OcpiUrl::try_from(versions_url).unwrap(),
        business_details: BusinessDetails {
            name: "Example Party".to_owned(),
            website: None,
            logo: None,
        },
        party_id: CiString3::try_from(party).unwrap(),
        country_code: CiString2::try_from(country).unwrap(),
    }
}

/// Bind an ephemeral loopback socket and return it with its origin.
async fn bind() -> (tokio::net::TcpListener, String) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    (listener, base)
}

/// Serve `router` on `listener` on a background task.
fn serve(listener: tokio::net::TcpListener, router: axum::Router) {
    tokio::spawn(async move {
        axum::serve(listener, router.into_make_service())
            .await
            .unwrap();
    });
}

#[tokio::test]
async fn m7_credentials_2_1_1_handshake_end_to_end() {
    // ── Party B (CPO, receiver): hosts the flat 2.1.1 credentials endpoint. ──
    let (b_listener, b_base) = bind().await;
    let b_credentials = credentials(TOKEN_C, &format!("{b_base}/versions"), "EXA", "NL");
    let cfg = Arc::new(Credentials2111Config::new(b_credentials.clone()));
    serve(b_listener, credentials_2_1_1_router(cfg));
    let credentials_url = format!("{b_base}/credentials");

    // ── Step 1: POST /credentials — register and exchange tokens. ───────────
    // A bootstraps with Token A and offers its own flat credentials (Token B).
    let client = OcpiClient::new(url::Url::parse(&format!("{b_base}/")).unwrap(), TOKEN_A);
    let a_credentials = credentials(
        TOKEN_B,
        "https://emsp.example.com/ocpi/versions",
        "MSP",
        "DE",
    );
    let exchanged = client
        .register_2_1_1(&credentials_url, &a_credentials)
        .await
        .expect("2.1.1 registration should succeed");

    assert_eq!(
        exchanged, b_credentials,
        "B returns its own flat credentials"
    );
    assert_eq!(exchanged.token, TOKEN_C, "B hands back Token C");
    assert_ne!(
        exchanged.token, TOKEN_A,
        "exchanged token differs from Token A"
    );
    assert_eq!(exchanged.party_id.as_str(), "EXA");
    assert_eq!(exchanged.country_code.as_str(), "NL");

    // ── Step 2: subsequent requests authenticate with Token C, not Token A. ──
    let client_c = OcpiClient::new(url::Url::parse(&format!("{b_base}/")).unwrap(), TOKEN_C);
    let fetched = client_c
        .get_credentials_2_1_1(&credentials_url)
        .await
        .expect("GET /credentials with Token C should return 200");
    assert_eq!(fetched, b_credentials, "GET echoes B's own credentials");

    // The bootstrap Token A is burned: a GET presenting it must be rejected.
    let stale = client.get_credentials_2_1_1(&credentials_url).await;
    assert!(
        stale.is_err(),
        "GET with the burned bootstrap Token A must be unauthorized (HTTP 401)"
    );

    // ── Step 3: PUT /credentials rotates under Token C. ─────────────────────
    let rotated_body = credentials(
        TOKEN_B,
        "https://emsp.example.com/ocpi/versions",
        "MSP",
        "DE",
    );
    let rotated = client_c
        .update_credentials_2_1_1(&credentials_url, &rotated_body)
        .await
        .expect("PUT /credentials with Token C should return 200");
    assert_eq!(rotated, b_credentials, "PUT echoes B's own credentials");

    // A second POST must be rejected — Token C is already registered (HTTP 405).
    let reregister = client
        .register_2_1_1(&credentials_url, &a_credentials)
        .await;
    assert!(
        reregister.is_err(),
        "re-POST after registration must fail (HTTP 405 already registered)"
    );

    // ── Step 4: DELETE /credentials unregisters (version-agnostic). ─────────
    client_c
        .delete_credentials(&credentials_url)
        .await
        .expect("DELETE /credentials with Token C should succeed");
    let after_delete = client_c.get_credentials_2_1_1(&credentials_url).await;
    assert!(
        after_delete.is_err(),
        "GET after DELETE must be unauthorized — the registration is gone"
    );
}

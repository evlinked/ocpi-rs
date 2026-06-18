//! M2 end-to-end registration smoke test (issue #23).
//!
//! Walks the full OCPI **2.2.1** bootstrap over a real loopback transport:
//! two in-process `axum` servers are stood up on ephemeral `127.0.0.1` ports
//! and driven entirely through [`OcpiClient`] (no raw `reqwest`).
//!
//! Topology — the two sides of the *Token A → B → C* handshake:
//!
//! - **Party B (CPO, receiver)** hosts `versions_router` + `credentials_router`.
//!   Its [`CredentialsConfig`] is built with [`OcpiVersionFetcher`] so a
//!   `POST /credentials` triggers the spec-mandated registration *fetch-back*.
//! - **Party A (eMSP, sender)** hosts only `versions_router`, so that B's
//!   fetch-back can `GET` A's `/versions` + version details and discover A's
//!   endpoint catalogue.
//!
//! The flow:
//!   1. `GET /versions` on B, then `GET /versions/2.2.1` (via
//!      [`OcpiClient::negotiate_version`]), selecting 2.2.1.
//!   2. `POST /credentials` on B carrying A's own credentials (Token B + A's
//!      `/versions` URL). B fetches back against A, registers, and returns its
//!      own credentials carrying Token C.
//!   3. A subsequent `GET /credentials` returns B's credentials with HTTP 200.
//!
//! This is the live transport counterpart to the data-contract test in
//! `ocpi-types/tests/credentials_handshake_2_2_1.rs`.
//!
//! Spec: `specs/ocpi/2.2.1/credentials.asciidoc` — *Registration* use-case.

use std::sync::Arc;

use ocpi_client::{OcpiClient, OcpiVersionFetcher};
use ocpi_server::{
    http::{credentials_router, versions_router},
    CredentialsConfig, VersionsConfig,
};
use ocpi_types::{
    BusinessDetails, CiString2, CiString3, Credentials, CredentialsRole, Endpoint, InterfaceRole,
    ModuleID, Role, Url as OcpiUrl, Version, VersionDetails, VersionNumber,
};

/// The bootstrap token (Token A) the CPO pre-shares with the eMSP out of band.
const TOKEN_A: &str = "TOKEN_A_bootstrap";
/// The token the eMSP puts in its POST body for the CPO to call back with.
const TOKEN_B: &str = "TOKEN_B_cpo_calls_emsp";
/// The token the CPO issues to the eMSP in its registration response.
const TOKEN_C: &str = "TOKEN_C_emsp_calls_cpo";

/// Build a single-role 2.2.1 credentials object, as it appears on the wire.
fn credentials(
    token: &str,
    versions_url: &str,
    role: Role,
    party: &str,
    country: &str,
) -> Credentials {
    Credentials {
        token: token.to_owned(),
        url: OcpiUrl::try_from(versions_url).unwrap(),
        roles: vec![CredentialsRole {
            role,
            business_details: BusinessDetails {
                name: "Example Party".to_owned(),
                website: None,
                logo: None,
            },
            party_id: CiString3::try_from(party).unwrap(),
            country_code: CiString2::try_from(country).unwrap(),
        }],
    }
}

/// A `VersionsConfig` advertising 2.2.1 with a single `credentials` endpoint,
/// using `base` (an `http://127.0.0.1:PORT` origin) to build absolute URLs.
fn versions_config(base: &str) -> VersionsConfig {
    let mut cfg = VersionsConfig::new();
    cfg.add_version(
        Version {
            version: VersionNumber::V2_2_1,
            url: OcpiUrl::try_from(format!("{base}/versions/2.2.1").as_str()).unwrap(),
        },
        VersionDetails {
            version: VersionNumber::V2_2_1,
            endpoints: vec![Endpoint {
                identifier: ModuleID::Credentials,
                role: InterfaceRole::Sender,
                url: OcpiUrl::try_from(format!("{base}/credentials").as_str()).unwrap(),
            }],
        },
    );
    cfg
}

/// Bind an ephemeral loopback socket and return it with its
/// `http://127.0.0.1:PORT` origin. Binding before building the router lets a
/// server reference its own origin in the URLs it advertises.
async fn bind() -> (tokio::net::TcpListener, String) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    (listener, base)
}

/// Serve `router` on `listener` on a background task. The socket is already
/// listening (from [`bind`]), so callers need no readiness delay.
fn serve(listener: tokio::net::TcpListener, router: axum::Router) {
    tokio::spawn(async move {
        axum::serve(listener, router.into_make_service())
            .await
            .unwrap();
    });
}

#[tokio::test]
async fn m2_registration_bootstrap_end_to_end() {
    // ── Party A (eMSP): answers B's fetch-back over its /versions. ──────────
    let (a_listener, a_base) = bind().await;
    serve(a_listener, versions_router(versions_config(&a_base)));

    // ── Party B (CPO): versions + credentials with fetch-back enabled. ──────
    let (b_listener, b_base) = bind().await;
    let b_credentials = credentials(
        TOKEN_C,
        &format!("{b_base}/versions"),
        Role::Cpo,
        "EXA",
        "NL",
    );
    let cred_cfg = Arc::new(CredentialsConfig::new_with_fetcher(
        b_credentials.clone(),
        vec![VersionNumber::V2_2_1],
        Arc::new(OcpiVersionFetcher::new()),
    ));
    let b_router = versions_router(versions_config(&b_base)).merge(credentials_router(cred_cfg));
    serve(b_listener, b_router);

    // ── Step 1: version negotiation (GET /versions, GET /versions/2.2.1). ───
    let client = OcpiClient::new(url::Url::parse(&format!("{b_base}/")).unwrap(), TOKEN_A);
    let details = client
        .negotiate_version(&[VersionNumber::V2_2_1])
        .await
        .expect("negotiation should pick a mutual version");
    assert_eq!(details.version, VersionNumber::V2_2_1, "must select 2.2.1");

    let credentials_url = details
        .endpoints
        .iter()
        .find(|e| e.identifier == ModuleID::Credentials)
        .map(|e| e.url.as_str().to_owned())
        .expect("version details must advertise a credentials endpoint");

    // ── Step 2: POST /credentials — register and exchange tokens. ───────────
    // A POSTs its own credentials (Token B for the callback + A's /versions
    // URL). B runs the fetch-back against A, then returns B's credentials.
    let a_credentials = credentials(
        TOKEN_B,
        &format!("{a_base}/versions"),
        Role::Emsp,
        "MSP",
        "DE",
    );
    let exchanged = client
        .register(&credentials_url, &a_credentials)
        .await
        .expect("registration should succeed after fetch-back");

    assert_eq!(exchanged, b_credentials, "B returns its own credentials");
    assert_eq!(exchanged.token, TOKEN_C, "B hands back Token C");
    assert_ne!(
        exchanged.token, TOKEN_A,
        "the exchanged token must differ from the bootstrap token A"
    );
    assert_eq!(exchanged.roles[0].role, Role::Cpo);

    // ── Step 3: a subsequent GET /credentials returns 200 with B's creds. ───
    // NOTE: the current server keys the registration by the POST *bearer*
    // (Token A) rather than the issued Token C, so the follow-up GET reuses
    // Token A. Per OCPI 2.2.1 the eMSP should switch to Token C and Token A
    // should be invalidated — tracked as a server follow-up, out of scope here.
    let fetched = client
        .get_credentials(&credentials_url)
        .await
        .expect("GET /credentials should return 200 for a registered party");
    assert_eq!(fetched, b_credentials, "GET echoes B's own credentials");

    // A second POST under the same bearer must be rejected (already registered).
    let reregister = client.register(&credentials_url, &a_credentials).await;
    assert!(
        reregister.is_err(),
        "re-POST under an already-registered token must fail (HTTP 405)"
    );
}

#[tokio::test]
async fn get_credentials_unauthorized_before_registration() {
    let (listener, base) = bind().await;
    let b_credentials = credentials(TOKEN_C, &format!("{base}/versions"), Role::Cpo, "EXA", "NL");
    serve(
        listener,
        credentials_router(Arc::new(CredentialsConfig::new(b_credentials))),
    );

    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN_A);
    let result = client.get_credentials(&format!("{base}/credentials")).await;
    assert!(
        result.is_err(),
        "GET /credentials before registration must be unauthorized (HTTP 401)"
    );
}

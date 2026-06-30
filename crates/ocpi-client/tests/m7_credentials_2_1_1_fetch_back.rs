//! M7 end-to-end registration **fetch-back** test for **OCPI 2.1.1** (issue #115).
//!
//! Walks the flat 2.1.1 *Token A → B → C* handshake over a real loopback
//! transport, this time with the registration **fetch-back** enabled — the
//! 2.1.1 counterpart to [`m2_registration`]:
//!
//! - **Party B (CPO, receiver)** hosts `credentials_2_1_1_router` built with a
//!   [`OcpiVersionFetcher`] as a [`LegacyVersionFetcher`], so a
//!   `POST /credentials` triggers the role-less 2.1.1 fetch-back.
//! - **Party A (eMSP, sender)** hosts only a role-less `/versions` +
//!   `/versions/2.1.1` (registered via `VersionsConfig::add_legacy_version`),
//!   so B's fetch-back can `GET` A's catalogue and discover A's endpoints.
//!
//! The 2.1.1 version details are **role-less** (`v2_1_1::Endpoint` has no
//! `role`); the [`LegacyVersionFetcher`] is what lets B parse them — the
//! role-bearing `VersionFetcher` would fail to deserialize.
//!
//! Spec: OCPI 2.1.1 — *Credentials* module / *Registration* use-case (fetch-back).

use std::sync::Arc;

use ocpi_client::{OcpiClient, OcpiVersionFetcher};
use ocpi_server::{
    http::{credentials_2_1_1_router, versions_router},
    Credentials2111Config, VersionsConfig,
};
use ocpi_types::v2_1_1::{Credentials, Endpoint as Endpoint2111, VersionDetails as Details2111};
use ocpi_types::{
    BusinessDetails, CiString2, CiString3, ModuleID, Url as OcpiUrl, Version, VersionNumber,
};

/// The bootstrap token (Token A) the CPO pre-shares with the eMSP out of band.
const TOKEN_A: &str = "TOKEN_A_bootstrap";
/// The token the eMSP puts in its POST body for the CPO to call back with.
const TOKEN_B: &str = "TOKEN_B_cpo_calls_emsp";
/// The token the CPO issues to the eMSP in its registration response.
const TOKEN_C: &str = "TOKEN_C_emsp_calls_cpo";

/// Build a flat 2.1.1 credentials object, as it appears on the wire. `url` is
/// the party's `/versions` endpoint (what the fetch-back `GET`s).
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

/// A `VersionsConfig` advertising **2.1.1** with a role-less endpoint catalogue
/// (Locations + Tokens), using `base` to build absolute URLs.
fn versions_config_2_1_1(base: &str) -> VersionsConfig {
    let mut cfg = VersionsConfig::new();
    cfg.add_legacy_version(
        Version {
            version: VersionNumber::V2_1_1,
            url: OcpiUrl::try_from(format!("{base}/versions/2.1.1").as_str()).unwrap(),
        },
        Details2111 {
            version: VersionNumber::V2_1_1,
            endpoints: vec![
                Endpoint2111 {
                    identifier: ModuleID::Locations,
                    url: OcpiUrl::try_from(format!("{base}/2.1.1/locations").as_str()).unwrap(),
                },
                Endpoint2111 {
                    identifier: ModuleID::Tokens,
                    url: OcpiUrl::try_from(format!("{base}/2.1.1/tokens").as_str()).unwrap(),
                },
            ],
        },
    );
    cfg
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
async fn m7_credentials_2_1_1_fetch_back_end_to_end() {
    // ── Party A (eMSP, sender): answers B's fetch-back over a role-less
    //    /versions + /versions/2.1.1. ────────────────────────────────────────
    let (a_listener, a_base) = bind().await;
    serve(a_listener, versions_router(versions_config_2_1_1(&a_base)));

    // ── Party B (CPO, receiver): flat 2.1.1 credentials with fetch-back. ─────
    let (b_listener, b_base) = bind().await;
    let b_credentials = credentials(TOKEN_C, &format!("{b_base}/versions"), "EXA", "NL");
    let cfg = Arc::new(Credentials2111Config::new_with_fetcher(
        b_credentials.clone(),
        vec![VersionNumber::V2_1_1],
        Arc::new(OcpiVersionFetcher::new()),
    ));
    serve(b_listener, credentials_2_1_1_router(cfg.clone()));
    let credentials_url = format!("{b_base}/credentials");

    // ── Step 1: POST /credentials — A registers, carrying its own /versions
    //    URL (Token B). B fetches back against A and stores A's endpoints. ────
    let client = OcpiClient::new(url::Url::parse(&format!("{b_base}/")).unwrap(), TOKEN_A);
    let a_credentials = credentials(TOKEN_B, &format!("{a_base}/versions"), "MSP", "DE");
    let exchanged = client
        .register_2_1_1(&credentials_url, &a_credentials)
        .await
        .expect("2.1.1 registration with fetch-back should succeed");
    assert_eq!(exchanged, b_credentials, "B returns its own credentials");
    assert_eq!(exchanged.token, TOKEN_C, "B hands back Token C");

    // The fetch-back ran: B stored A's role-less endpoint catalogue under the
    // issued Token C. This is the behaviour #115 adds over #112's no-fetch-back
    // path (which would leave `get_endpoints` returning `None`).
    let stored = cfg
        .get_endpoints(TOKEN_C)
        .expect("fetch-back must store A's endpoints under Token C");
    assert_eq!(stored.len(), 2, "A advertised two 2.1.1 endpoints");
    assert_eq!(stored[0].identifier, ModuleID::Locations);
    assert_eq!(stored[1].identifier, ModuleID::Tokens);
    assert_eq!(
        stored[0].url.as_str(),
        format!("{a_base}/2.1.1/locations"),
        "stored endpoint URL is A's advertised locations URL"
    );

    // ── Step 2: PUT /credentials re-runs the fetch-back under Token C. ──────
    let client_c = OcpiClient::new(url::Url::parse(&format!("{b_base}/")).unwrap(), TOKEN_C);
    let rotated_body = credentials(TOKEN_B, &format!("{a_base}/versions"), "MSP", "DE");
    let rotated = client_c
        .update_credentials_2_1_1(&credentials_url, &rotated_body)
        .await
        .expect("PUT /credentials with Token C should re-fetch and succeed");
    assert_eq!(rotated, b_credentials);
    assert!(
        cfg.get_endpoints(TOKEN_C).is_some(),
        "PUT fetch-back re-stores the endpoint catalogue"
    );
}

#[tokio::test]
async fn m7_credentials_2_1_1_fetch_back_unreachable_party_is_3001() {
    // Party B has fetch-back enabled but party A is never started, so the
    // outbound `GET /versions` fails at the transport layer. Per spec the
    // receiver maps this to OCPI status code 3001 (surfaced as a client error).
    let (b_listener, b_base) = bind().await;
    let b_credentials = credentials(TOKEN_C, &format!("{b_base}/versions"), "EXA", "NL");
    let cfg = Arc::new(Credentials2111Config::new_with_fetcher(
        b_credentials.clone(),
        vec![VersionNumber::V2_1_1],
        Arc::new(OcpiVersionFetcher::new()),
    ));
    serve(b_listener, credentials_2_1_1_router(cfg.clone()));
    let credentials_url = format!("{b_base}/credentials");

    // A's /versions URL points at a port with nothing listening.
    let client = OcpiClient::new(url::Url::parse(&format!("{b_base}/")).unwrap(), TOKEN_A);
    let dead = credentials(TOKEN_B, "http://127.0.0.1:1/versions", "MSP", "DE");
    let result = client.register_2_1_1(&credentials_url, &dead).await;
    assert!(
        result.is_err(),
        "registration must fail when the fetch-back can't reach the party (3001)"
    );
    // Nothing was registered — the POST was rejected before storing.
    assert!(
        !cfg.is_registered(TOKEN_C),
        "a failed fetch-back must not register the party"
    );
}

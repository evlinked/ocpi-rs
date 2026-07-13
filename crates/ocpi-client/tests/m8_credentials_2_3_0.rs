//! M8 end-to-end **OCPI 2.3.0** Credentials handshake test (issue #206).
//!
//! Walks the full Token A→B→C registration over a real loopback transport —
//! two in-process `axum` servers on ephemeral `127.0.0.1` ports, driven
//! entirely through [`OcpiClient`]'s `_2_3_0` methods — but over the
//! [`Credentials230`] fork (the `hub_party_id` addition, #179) rather than the
//! 2.2.1 shape. 2.3.0 reuses the 2.2.1 role-bearing Versions layer, so the
//! topology is identical to `m2_registration.rs`; the only thing under test is
//! that the Hub-identifying `hub_party_id` survives the exchange instead of
//! being dropped through the 2.2.1 `Credentials` struct.
//!
//! Topology — the two sides of the handshake:
//!
//! - **Party B (Hub, receiver)** hosts `versions_router` +
//!   `credentials_2_3_0_router`. Its [`Credentials230Config`] carries a
//!   `hub_party_id` (B *is* a Hub) and is built with [`OcpiVersionFetcher`] so a
//!   `POST /credentials` triggers the spec-mandated fetch-back.
//! - **Party A (eMSP, sender)** hosts only `versions_router`, so B's fetch-back
//!   can `GET` A's `/versions` and discover A's endpoint catalogue.
//!
//! Spec: `specs/ocpi/2.3.0/credentials.asciidoc` — Credentials / Registration.

use std::sync::Arc;

use ocpi_client::{OcpiClient, OcpiVersionFetcher};
use ocpi_server::{
    http::{credentials_2_3_0_router, versions_router},
    Credentials230Config, VersionsConfig,
};
use ocpi_types::{
    v2_3_0::Credentials as Credentials230, BusinessDetails, CiString2, CiString3, CiString5,
    CredentialsRole, Endpoint, InterfaceRole, ModuleID, Role, Url as OcpiUrl, Version,
    VersionDetails, VersionNumber,
};

const TOKEN_A: &str = "TOKEN_A_bootstrap";
const TOKEN_B: &str = "TOKEN_B_hub_calls_emsp";
const TOKEN_C: &str = "TOKEN_C_emsp_calls_hub";

/// Build a single-role 2.3.0 credentials object, optionally carrying the 2.3.0
/// `hub_party_id`. When `hub_party_id` is `None` the field is absent on the
/// wire, so the object is byte-for-byte a 2.2.1 credentials object.
fn credentials(
    token: &str,
    versions_url: &str,
    role: Role,
    party: &str,
    country: &str,
    hub_party_id: Option<&str>,
) -> Credentials230 {
    Credentials230 {
        token: token.to_owned(),
        url: OcpiUrl::try_from(versions_url).unwrap(),
        hub_party_id: hub_party_id.map(|s| CiString5::try_from(s.to_owned()).unwrap()),
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

/// A `VersionsConfig` advertising 2.3.0 with a single `credentials` endpoint.
fn versions_config(base: &str) -> VersionsConfig {
    let mut cfg = VersionsConfig::new();
    cfg.add_version(
        Version {
            version: VersionNumber::V2_3_0,
            url: OcpiUrl::try_from(format!("{base}/versions/2.3.0").as_str()).unwrap(),
        },
        VersionDetails {
            version: VersionNumber::V2_3_0,
            endpoints: vec![Endpoint {
                identifier: ModuleID::Credentials,
                role: InterfaceRole::Sender,
                url: OcpiUrl::try_from(format!("{base}/credentials").as_str()).unwrap(),
            }],
        },
    );
    cfg
}

async fn bind() -> (tokio::net::TcpListener, String) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    (listener, base)
}

fn serve(listener: tokio::net::TcpListener, router: axum::Router) {
    tokio::spawn(async move {
        axum::serve(listener, router.into_make_service())
            .await
            .unwrap();
    });
}

/// The core slice: a **Hub** registration carrying `hub_party_id` completes and
/// the field survives every leg of the 2.3.0 transport (POST response, GET,
/// PUT response), where the 2.2.1 shape would have dropped it on deserialize.
#[tokio::test]
async fn m8_credentials_2_3_0_hub_registration_preserves_hub_party_id() {
    // ── Party A (eMSP): answers B's fetch-back over its /versions. ──────────
    let (a_listener, a_base) = bind().await;
    serve(a_listener, versions_router(versions_config(&a_base)));

    // ── Party B (Hub): versions + 2.3.0 credentials, fetch-back enabled. ────
    // B's own credentials advertise it as a Hub via `hub_party_id`, and list
    // the Hub role plus a reachable party as normal `roles` entries (2.3.0).
    let (b_listener, b_base) = bind().await;
    let mut b_credentials = credentials(
        TOKEN_C,
        &format!("{b_base}/versions"),
        Role::Hub,
        "HUB",
        "NL",
        Some("NLHUB"),
    );
    b_credentials.roles.push(
        credentials(
            TOKEN_C,
            &format!("{b_base}/versions"),
            Role::Cpo,
            "RCP",
            "DE",
            None,
        )
        .roles[0]
            .clone(),
    );
    let cred_cfg = Arc::new(Credentials230Config::new_with_fetcher(
        b_credentials.clone(),
        vec![VersionNumber::V2_3_0],
        Arc::new(OcpiVersionFetcher::new()),
    ));
    let b_router =
        versions_router(versions_config(&b_base)).merge(credentials_2_3_0_router(cred_cfg));
    serve(b_listener, b_router);

    // ── Step 1: version negotiation selects 2.3.0. ──────────────────────────
    let client = OcpiClient::new(url::Url::parse(&format!("{b_base}/")).unwrap(), TOKEN_A);
    let details = client
        .negotiate_version(&[VersionNumber::V2_3_0])
        .await
        .expect("negotiation should pick a mutual version");
    assert_eq!(details.version, VersionNumber::V2_3_0, "must select 2.3.0");
    let credentials_url = details
        .endpoints
        .iter()
        .find(|e| e.identifier == ModuleID::Credentials)
        .map(|e| e.url.as_str().to_owned())
        .expect("version details must advertise a credentials endpoint");

    // ── Step 2: POST /credentials — A registers, B returns its Hub creds. ───
    let a_credentials = credentials(
        TOKEN_B,
        &format!("{a_base}/versions"),
        Role::Emsp,
        "MSP",
        "DE",
        None,
    );
    let exchanged = client
        .register_2_3_0(&credentials_url, &a_credentials)
        .await
        .expect("registration should succeed after fetch-back");
    assert_eq!(
        exchanged, b_credentials,
        "B returns its own Hub credentials"
    );
    assert_eq!(exchanged.token, TOKEN_C, "B hands back Token C");
    // The load-bearing assertion: `hub_party_id` survived the POST response.
    assert_eq!(
        exchanged.hub_party_id.as_ref().map(|s| s.as_str()),
        Some("NLHUB"),
        "the Hub's hub_party_id must survive registration, not be dropped"
    );
    // 2.3.0: a Hub lists reachable parties as normal roles.
    assert_eq!(exchanged.roles.len(), 2);
    assert_eq!(exchanged.roles[1].party_id.as_str(), "RCP");

    // ── Step 3: subsequent requests use Token C; hub_party_id still present. ─
    let client_c = OcpiClient::new(url::Url::parse(&format!("{b_base}/")).unwrap(), TOKEN_C);
    let fetched = client_c
        .get_credentials_2_3_0(&credentials_url)
        .await
        .expect("GET /credentials with Token C should return 200");
    assert_eq!(fetched, b_credentials, "GET echoes B's own credentials");
    assert_eq!(
        fetched.hub_party_id.as_ref().map(|s| s.as_str()),
        Some("NLHUB"),
        "GET must carry hub_party_id on the 2.3.0 wire"
    );

    // The burned bootstrap Token A is rejected.
    let stale = client.get_credentials_2_3_0(&credentials_url).await;
    assert!(stale.is_err(), "burned Token A must be unauthorized (401)");

    // ── Step 4: PUT rotates the registration; response still Hub-shaped. ────
    let rotated = client_c
        .update_credentials_2_3_0(&credentials_url, &a_credentials)
        .await
        .expect("PUT /credentials with Token C should succeed");
    assert_eq!(
        rotated.hub_party_id.as_ref().map(|s| s.as_str()),
        Some("NLHUB"),
        "PUT response must carry hub_party_id"
    );
}

/// A non-hub 2.3.0 party registers byte-identically to 2.2.1: its credentials
/// omit `hub_party_id`, so the field is absent on the wire and the exact same
/// bytes parse into the 2.2.1 `Credentials` type — proving the new transport
/// never destabilises the surface a 2.2.1 peer already runs.
#[tokio::test]
async fn m8_credentials_2_3_0_non_hub_stays_wire_identical_to_2_2_1() {
    let (listener, base) = bind().await;
    let b_credentials = credentials(
        TOKEN_C,
        &format!("{base}/versions"),
        Role::Cpo,
        "EXA",
        "NL",
        None,
    );
    // Register the CPO directly so a GET with Token C is authorized.
    let cfg = Credentials230Config::new(b_credentials.clone());
    cfg.register(TOKEN_C, b_credentials.clone()).unwrap();
    serve(listener, credentials_2_3_0_router(Arc::new(cfg)));

    let client_c = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN_C);
    let fetched = client_c
        .get_credentials_2_3_0(&format!("{base}/credentials"))
        .await
        .expect("GET with the registered Token C should return 200");
    assert!(fetched.hub_party_id.is_none());

    // The serialized bytes carry no `hub_party_id` and parse into 2.2.1.
    let wire = ocpi_types::serde_json::to_string(&fetched).unwrap();
    assert!(
        !wire.contains("hub_party_id"),
        "non-hub 2.3.0 credentials must omit hub_party_id on the wire: {wire}"
    );
    let via_2_2_1: ocpi_types::Credentials = ocpi_types::serde_json::from_str(&wire).unwrap();
    assert_eq!(via_2_2_1.token, fetched.token);
    assert_eq!(via_2_2_1.roles, fetched.roles);

    // An unregistered bearer is rejected before registration.
    let anon = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN_A);
    assert!(anon
        .get_credentials_2_3_0(&format!("{base}/credentials"))
        .await
        .is_err());
}

/// The receiver's trust boundary: an over-length `hub_party_id` (`CiString(5)`)
/// fails to deserialize, so the `Json<Credentials230>` extractor rejects the
/// body before it can reach the store — never truncated or accepted.
#[test]
fn over_length_hub_party_id_is_rejected_on_deserialize() {
    let bad = r#"{
        "token": "t",
        "url": "https://hub.example.com/ocpi/versions",
        "hub_party_id": "TOOLONG",
        "roles": [
            { "role": "HUB", "business_details": { "name": "H" },
              "party_id": "HUB", "country_code": "NL" }
        ]
    }"#;
    let err = ocpi_types::serde_json::from_str::<Credentials230>(bad).unwrap_err();
    assert!(
        err.to_string().to_lowercase().contains("5")
            || err.to_string().to_lowercase().contains("length")
            || err.to_string().to_lowercase().contains("long"),
        "error should reflect the CiString(5) length bound: {err}"
    );
}

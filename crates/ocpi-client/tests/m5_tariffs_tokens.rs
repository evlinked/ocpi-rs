//! M5 end-to-end Tariffs + Tokens smoke test (issue #72).
//!
//! Walks a full CPO↔eMSP **Tariffs** and **Tokens** exchange over a real
//! loopback transport: in-process `axum` servers are stood up on ephemeral
//! `127.0.0.1` ports and driven entirely through [`OcpiClient`] (no raw
//! `reqwest`). This is the M5 counterpart to the M2 registration smoke test in
//! `m2_registration.rs`, completing the per-milestone e2e pattern.
//!
//! Topology mirrors the OCPI roles:
//!
//! - **Tariffs** — the CPO is the *sender* of the tariff list; the eMSP is the
//!   *receiver* that stores tariffs via `PUT`/`DELETE`. Here a single server
//!   hosts [`tariffs_router`] and the client plays the peer issuing
//!   `GET`/`PUT`/`DELETE`.
//! - **Tokens** — the eMSP is the *sender* of the token list; the CPO is the
//!   *receiver* that caches tokens via `PUT`/`PATCH` and answers real-time
//!   `POST .../authorize`. A single server hosts [`tokens_router`].
//!
//! Routing headers: the typed sender methods authenticate with the credentials
//! token but do not yet emit per-call `OCPI-to/from-party-id/country-code`
//! headers (tracked by #64). The receiver routers *accept* requests regardless
//! (they are not enforced at this layer), which this test confirms by driving
//! every endpoint to a successful round-trip.
//!
//! Spec: `specs/ocpi/2.2.1/mod_tariffs.asciidoc`,
//! `specs/ocpi/2.2.1/mod_tokens.asciidoc` — §Sender/§Receiver Interfaces and
//! Tokens §Real-time authorization (`POST .../authorize`).

use std::sync::Arc;

use ocpi_client::OcpiClient;
use ocpi_server::{
    http::{tariffs_router, tokens_router},
    TariffsConfig, TokensConfig,
};
use ocpi_types::{
    chrono::TimeZone as _, serde_json::json, transport::PaginatedParams, AllowedType, CiString2,
    CiString3, CiString36, DateTime, PriceComponent, Tariff, TariffDimensionType, TariffElement,
    Token, TokenType, Utc, WhitelistType,
};

/// Build a minimal but spec-faithful 2.2.1 `Tariff` (a single energy price
/// component at 0.25 EUR/kWh — the spec's "Simple Tariff" shape).
fn sample_tariff(id: &str, ts: DateTime<Utc>) -> Tariff {
    Tariff {
        country_code: CiString2::try_from("NL").unwrap(),
        party_id: CiString3::try_from("CPO").unwrap(),
        id: CiString36::try_from(id).unwrap(),
        currency: "EUR".to_owned(),
        tariff_type: None,
        tariff_alt_text: vec![],
        tariff_alt_url: None,
        min_price: None,
        max_price: None,
        elements: vec![TariffElement {
            price_components: vec![PriceComponent {
                component_type: TariffDimensionType::Energy,
                price: 0.25,
                vat: Some(21.0),
                step_size: 1,
            }],
            restrictions: None,
        }],
        start_date_time: None,
        end_date_time: None,
        energy_mix: None,
        last_updated: ts,
    }
}

/// Build a spec-faithful 2.2.1 `Token` (RFID card owned by an eMSP).
fn sample_token(uid: &str, ts: DateTime<Utc>, valid: bool) -> Token {
    Token {
        country_code: CiString2::try_from("DE").unwrap(),
        party_id: CiString3::try_from("TNM").unwrap(),
        uid: CiString36::try_from(uid).unwrap(),
        token_type: TokenType::Rfid,
        contract_id: CiString36::try_from("DE8ACC12E46L89").unwrap(),
        visual_number: None,
        issuer: "TheNewMotion".to_owned(),
        group_id: None,
        valid,
        whitelist: WhitelistType::Allowed,
        language: None,
        default_profile_type: None,
        energy_contract: None,
        last_updated: ts,
    }
}

fn at(secs: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap() + ocpi_types::chrono::Duration::seconds(secs)
}

/// Bind an ephemeral loopback socket and return it with its origin.
async fn bind() -> (tokio::net::TcpListener, String) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    (listener, base)
}

/// Serve `router` on the already-listening `listener` on a background task.
fn serve(listener: tokio::net::TcpListener, router: axum::Router) {
    tokio::spawn(async move {
        axum::serve(listener, router.into_make_service())
            .await
            .unwrap();
    });
}

#[tokio::test]
async fn m5_tariffs_get_list_put_delete_round_trip() {
    // ── Stand up an eMSP receiver hosting the Tariffs module. ───────────────
    // Pre-seed three tariffs so the list endpoint exercises real pagination.
    let cfg = Arc::new(TariffsConfig::new());
    for i in 0..3 {
        let id = format!("TARIFF{i:03}");
        cfg.put("NL", "CPO", &id, sample_tariff(&id, at(i)));
    }
    let (listener, base) = bind().await;
    serve(listener, tariffs_router(cfg.clone()));

    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), "tok");
    let tariffs_url = format!("{base}/tariffs");

    // GET single tariff round-trips by composite key.
    let got = client
        .get_tariff(&tariffs_url, "NL", "CPO", "TARIFF001")
        .await
        .expect("GET tariff by id should succeed");
    assert_eq!(got.id.as_str(), "TARIFF001");
    assert_eq!(got.currency, "EUR");
    assert_eq!(got.elements[0].price_components[0].price, 0.25);

    // GET list with limit=2 — first page yields 2, total is 3, and the server
    // advertises a next page via the Link header.
    let (page, meta) = client
        .get_tariffs(
            &tariffs_url,
            PaginatedParams {
                limit: Some(2),
                ..Default::default()
            },
        )
        .await
        .expect("GET tariffs list should succeed");
    assert_eq!(page.len(), 2, "first page honors limit=2");
    assert_eq!(meta.total_count, 3, "X-Total-Count reflects all matches");
    assert_eq!(meta.limit, 2, "X-Limit echoes the requested limit");
    assert!(
        meta.next_url.is_some(),
        "Link: rel=next must be present while more pages remain"
    );

    // PUT a fresh tariff, then confirm it is retrievable through the client.
    let new = sample_tariff("TARIFF999", at(99));
    client
        .put_tariff(&tariffs_url, "NL", "CPO", "TARIFF999", &new)
        .await
        .expect("PUT tariff should succeed");
    assert!(
        cfg.get("NL", "CPO", "TARIFF999").is_some(),
        "store reflects the inserted tariff"
    );

    // DELETE it, then confirm both the store and a follow-up GET agree it's gone.
    client
        .delete_tariff(&tariffs_url, "NL", "CPO", "TARIFF999")
        .await
        .expect("DELETE tariff should succeed");
    assert!(
        cfg.get("NL", "CPO", "TARIFF999").is_none(),
        "store reflects the removed tariff"
    );
    let missing = client
        .get_tariff(&tariffs_url, "NL", "CPO", "TARIFF999")
        .await;
    assert!(missing.is_err(), "GET of a deleted tariff must 404");

    // DELETE of an unknown tariff is reported as NotFound, not silently ignored.
    let del_missing = client
        .delete_tariff(&tariffs_url, "NL", "CPO", "NOPE")
        .await;
    assert!(del_missing.is_err(), "DELETE of an unknown tariff must 404");
}

#[tokio::test]
async fn m5_tokens_list_put_patch_authorize_round_trip() {
    // ── Stand up a CPO receiver hosting the Tokens module. ──────────────────
    // Pre-seed three tokens; the last is invalid to exercise the blocked path.
    let cfg = Arc::new(TokensConfig::new());
    cfg.put(
        "DE",
        "TNM",
        "VALID001",
        TokenType::Rfid,
        sample_token("VALID001", at(0), true),
    );
    cfg.put(
        "DE",
        "TNM",
        "VALID002",
        TokenType::Rfid,
        sample_token("VALID002", at(1), true),
    );
    cfg.put(
        "DE",
        "TNM",
        "BLOCKED9",
        TokenType::Rfid,
        sample_token("BLOCKED9", at(2), false),
    );
    let (listener, base) = bind().await;
    serve(listener, tokens_router(cfg.clone()));

    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), "tok");
    let tokens_url = format!("{base}/tokens");

    // GET list with limit=2 — pagination headers mirror the tariffs case.
    let (page, meta) = client
        .get_tokens(
            &tokens_url,
            PaginatedParams {
                limit: Some(2),
                ..Default::default()
            },
        )
        .await
        .expect("GET tokens list should succeed");
    assert_eq!(page.len(), 2, "first page honors limit=2");
    assert_eq!(meta.total_count, 3, "X-Total-Count reflects all matches");
    assert_eq!(meta.limit, 2, "X-Limit echoes the requested limit");
    assert!(
        meta.next_url.is_some(),
        "Link: rel=next must be present while more pages remain"
    );

    // PUT a new token, then PATCH it (merge-patch flips `valid` false→via PUT).
    client
        .put_token(
            &tokens_url,
            "DE",
            "TNM",
            "NEW0001",
            TokenType::Rfid,
            &sample_token("NEW0001", at(10), true),
        )
        .await
        .expect("PUT token should succeed");
    assert!(
        cfg.get("DE", "TNM", "NEW0001", TokenType::Rfid)
            .is_some_and(|t| t.valid),
        "store reflects the inserted (valid) token"
    );

    // PATCH a single field — the receiver applies an RFC 7396 merge-patch.
    client
        .patch_token(
            &tokens_url,
            "DE",
            "TNM",
            "NEW0001",
            TokenType::Rfid,
            &json!({ "valid": false }),
        )
        .await
        .expect("PATCH token should succeed");
    assert!(
        cfg.get("DE", "TNM", "NEW0001", TokenType::Rfid)
            .is_some_and(|t| !t.valid),
        "PATCH flips the token to invalid in the store"
    );

    // Real-time authorization: a valid seeded token is ALLOWED…
    let allowed = client
        .authorize_token(&tokens_url, "VALID001", TokenType::Rfid, None)
        .await
        .expect("authorize of a known valid token should succeed");
    assert_eq!(allowed.allowed, AllowedType::Allowed);
    assert_eq!(allowed.token.uid.as_str(), "VALID001");

    // …an invalid token is refused (BLOCKED)…
    let blocked = client
        .authorize_token(&tokens_url, "BLOCKED9", TokenType::Rfid, None)
        .await
        .expect("authorize of a known invalid token still returns AuthorizationInfo");
    assert_eq!(blocked.allowed, AllowedType::Blocked);

    // …and an entirely unknown token yields OCPI 2004 (HTTP 404 → NotFound).
    let unknown = client
        .authorize_token(&tokens_url, "GHOST", TokenType::Rfid, None)
        .await;
    assert!(
        unknown.is_err(),
        "authorize of an unknown token must surface NotFound (OCPI 2004)"
    );
}

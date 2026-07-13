//! M8 OCPI **2.3.0** Tokens — *wire-identical module reuse*, verified at the
//! transport layer (issue #205, the 2.3.0 reuse-column close-out).
//!
//! Tokens is one of the **five** OCPI modules with **no** 2.3.0-vs-2.2.1 wire
//! delta (`specs/ocpi/2.3.0/changelog.asciidoc` — the entire 2.3.0-over-2.2.1
//! delta surface is Payments, the Locations Parking/15118/`accepted_emsps`
//! additions, the North-American tax rework on Tariffs/CDRs/Sessions, the
//! Credentials `hub_party_id`, and the "make OCPI extensible" serde policy).
//! Everything else — **Versions, Tokens, Commands, ChargingProfiles,
//! HubClientInfo** — is a genuine re-export of its 2.2.1 type, so
//! `ocpi_types::v2_3_0::Token` *is* `ocpi_types::v2_2_1::Token` (a compile-time
//! identity, ratified by the `reuse_types_stay_aliases_of_2_2_1` assertions in
//! `crates/ocpi-types/src/v2_3_0/mod.rs`).
//!
//! The design decision #205 ratifies — the direct M8 mirror of the 2.2-column
//! close-out (#171 / `m7_tariffs_reuse_2_2.rs`) — is therefore that **no
//! `_2_3_0` client/server methods are minted for the wire-identical modules**:
//! aliasing identical-typed calls would only imply a difference that does not
//! exist. A 2.3.0 party drives Tokens with the *existing* unqualified 2.2.1
//! surface (`OcpiClient::{get_tokens,put_token,authorize_token}` +
//! [`tokens_router`]).
//!
//! The type-level round-trips in `v2_3_0/mod.rs` prove reuse at the **type**
//! layer (a `v2_3_0::Token` serializes byte-for-byte like the 2.2.1 one). This
//! test proves the same claim one layer down — at the **transport** layer — for
//! Tokens: a `v2_3_0::Token` rides the existing 2.2.1 client + server end-to-end
//! (`PUT` → paginated `GET` list → real-time `authorize`) and round-trips
//! byte-for-byte. That is what makes the README matrix 2.3.0/Tokens cell honest
//! as ☑: reuse is *exercised*, not merely asserted.
//!
//! Spec: OCPI 2.3.0 — *Tokens* module (wire-identical to
//! `specs/ocpi/2.2.1/mod_tokens.asciidoc`): Receiver `PUT` at
//! `{country_code}/{party_id}/{token_uid}`, Sender `GET /tokens`, real-time
//! `POST /tokens/{token_uid}/authorize`.

use std::sync::Arc;

use ocpi_client::OcpiClient;
use ocpi_server::{http::tokens_router, TokensConfig};
// The 2.3.0 alias — `v2_3_0::Token` is the very same type as `v2_2_1::Token`, so
// the values it produces feed the unqualified 2.2.1 client/server surface
// directly. Importing it *through `v2_3_0`* is the point: this is a 2.3.0 party.
use ocpi_types::v2_3_0::{Token, TokenType};
// `AllowedType` (the authorize verdict) has no `v2_3_0` re-export of its own; it
// is the shared 2.2.1 enum, imported here to read the reuse result — importing
// it from `v2_2_1` is itself evidence the authorize surface is unforked.
use ocpi_types::v2_2_1::AllowedType;
use ocpi_types::{
    chrono::{Duration, TimeZone as _},
    serde_json::{self, json},
    transport::PaginatedParams,
    DateTime, Utc,
};

/// The bearer the peer presents on the Tokens exchange.
const TOKEN: &str = "TOKEN_C_tokens_reuse_2_3_0";

/// The spec's Token example (2.3.0 §Tokens — an RFID card owned by an eMSP).
/// The `country_code`/`party_id` owner fields, the `uid`/`type`/`contract_id`
/// shape, `valid`/`whitelist`, and `last_updated` are wire-identical between
/// 2.2.1 and 2.3.0 — which is exactly why Tokens needs no 2.3.0 override.
fn token_json() -> serde_json::Value {
    json!({
        "country_code": "DE",
        "party_id": "TNM",
        "uid": "12345678905880",
        "type": "RFID",
        "contract_id": "DE8ACC12E46L89",
        "issuer": "TheNewMotion",
        "valid": true,
        "whitelist": "ALLOWED",
        "last_updated": "2018-12-10T17:16:15Z"
    })
}

/// Build a 2.3.0 `Token` from the spec fixture, overriding the `uid`, the
/// `valid` flag, and the `last_updated` timestamp that drives deterministic
/// date filtering.
fn make_token(uid: &str, ts: DateTime<Utc>, valid: bool) -> Token {
    let mut token: Token = serde_json::from_value(token_json()).expect("valid 2.3.0 token");
    token.uid = uid.try_into().unwrap();
    token.valid = valid;
    token.last_updated = ts;
    token
}

/// Bind an ephemeral loopback socket and return it with its origin URL.
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

/// A 2.3.0 party drives the wire-identical Tokens module end-to-end over the
/// *existing* 2.2.1 client + server — no `_2_3_0` method is minted or needed.
/// The `v2_3_0::Token` round-trips byte-for-byte, which is the transport-layer
/// proof behind the README 2.3.0/Tokens ☑.
#[tokio::test]
async fn m8_tokens_2_3_0_reuse_put_list_authorize_round_trip() {
    // ── Receiver server: the unqualified 2.2.1 `tokens_router`. ─────────────
    let cfg = Arc::new(TokensConfig::new());
    let base_ts = Utc
        .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
        .single()
        .expect("valid timestamp");
    // Pre-seed two tokens so the list endpoint exercises real pagination; the
    // second is invalid to exercise the blocked authorize path.
    cfg.put(
        "DE",
        "TNM",
        "SEED0001",
        TokenType::Rfid,
        make_token("SEED0001", base_ts, true),
    );
    cfg.put(
        "DE",
        "TNM",
        "SEED0002",
        TokenType::Rfid,
        make_token("SEED0002", base_ts + Duration::seconds(1), false),
    );
    let (listener, base) = bind().await;
    serve(listener, tokens_router(cfg));

    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN);
    let tokens_url = format!("{base}/tokens");

    // ── PUT a new 2.3.0 token via the unqualified receiver method. ──────────
    let new = make_token("PUSH0003", base_ts + Duration::seconds(2), true);
    client
        .put_token(&tokens_url, "DE", "TNM", "PUSH0003", TokenType::Rfid, &new)
        .await
        .expect("PUT token should succeed");

    // ── GET paginated list — first page (limit 2 of 3). ─────────────────────
    let (page1, meta) = client
        .get_tokens(
            &tokens_url,
            PaginatedParams {
                limit: Some(2),
                ..Default::default()
            },
        )
        .await
        .expect("GET tokens list should return 200");
    assert_eq!(page1.len(), 2);
    assert_eq!(meta.total_count, 3);
    assert_eq!(meta.limit, 2);
    let next = meta.next_url.expect("a second page should be advertised");
    assert!(next.contains("offset=2"), "next link: {next}");

    // The just-pushed 2.3.0 token is on the second page and byte-for-byte
    // identical — the object is stored and served unmangled (the reuse is real,
    // not coerced through a distinct 2.3.0-only type, because there is none).
    let (page2, _) = client
        .get_tokens(
            &tokens_url,
            PaginatedParams {
                offset: Some(2),
                limit: Some(2),
                ..Default::default()
            },
        )
        .await
        .expect("GET tokens list page 2 should return 200");
    assert_eq!(page2.len(), 1);
    assert_eq!(page2[0], new);

    // ── Real-time authorization over the unqualified 2.2.1 method. ──────────
    // The valid seeded token is ALLOWED…
    let ok = client
        .authorize_token(&tokens_url, "SEED0001", TokenType::Rfid, None)
        .await
        .expect("authorize of a known valid token should succeed");
    assert!(matches!(ok.allowed, AllowedType::Allowed));
    assert_eq!(ok.token, make_token("SEED0001", base_ts, true));

    // …and the invalid seeded token is refused (BLOCKED) — the authorize
    // verdict logic is the shared 2.2.1 one, exercised through a 2.3.0 party.
    let blocked = client
        .authorize_token(&tokens_url, "SEED0002", TokenType::Rfid, None)
        .await
        .expect("authorize of a known invalid token still returns AuthorizationInfo");
    assert!(matches!(blocked.allowed, AllowedType::Blocked));
}

/// A 2.3.0 party's serialized `Token` is byte-for-byte a 2.2.1 `Token` — the
/// concrete evidence the module carries no wire delta and so is reused rather
/// than forked. (The compile-time identity is asserted in
/// `crates/ocpi-types/src/v2_3_0/mod.rs`; this checks the *wire* agrees.)
#[test]
fn token_2_3_0_wire_is_identical_to_2_2_1() {
    let ts = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).single().unwrap();
    let via_2_3_0: ocpi_types::v2_3_0::Token = make_token("12345678905880", ts, true);
    let via_2_2_1: ocpi_types::v2_2_1::Token = serde_json::from_value(token_json())
        .map(|mut t: ocpi_types::v2_2_1::Token| {
            t.uid = "12345678905880".try_into().unwrap();
            t.last_updated = ts;
            t
        })
        .unwrap();

    // Same type, same value.
    assert_eq!(via_2_3_0, via_2_2_1);
    // Same bytes on the wire — no field added, dropped, or renamed for 2.3.0.
    assert_eq!(
        serde_json::to_value(&via_2_3_0).unwrap(),
        serde_json::to_value(&via_2_2_1).unwrap(),
    );
}

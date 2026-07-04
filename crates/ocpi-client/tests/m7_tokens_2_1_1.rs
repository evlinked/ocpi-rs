//! M7 OCPI **2.1.1** Tokens — client + server round-trip smoke test
//! (issue #123, client slice; the server slice landed in #137).
//!
//! Stands up an in-process `axum` server hosting the real
//! [`tokens_2_1_1_router`] on an ephemeral `127.0.0.1` port and drives the CPO ↔
//! eMSP Tokens exchange entirely through [`OcpiClient`]'s 2.1.1 methods (no raw
//! `reqwest`). Mirrors the 2.1.1 Sessions smoke test (`m7_sessions_2_1_1.rs`).
//!
//! ## URL shapes (identical transport to 2.2.1)
//!
//! Per OCPI 2.1.1 §12.2.2 *"Token is a client owned object, so the end-points
//! need to contain the required extra fields: {party_id} and {country_code}"*,
//! so the receiver path carries the composite key — those URL segments predate
//! the 2.2 `OCPI-to/from-*` routing headers:
//!
//! - list:      `GET   {base}/tokens?…`                        (sender, §12.2.1)
//! - upsert:    `PUT   {base}/tokens/{cc}/{party}/{uid}?type=` (receiver, §12.2.2)
//! - patch:     `PATCH {base}/tokens/{cc}/{party}/{uid}?type=` (receiver)
//! - authorize: `POST  {base}/tokens/{uid}/authorize?type=`    (sender, §12.3)
//!
//! Only the payload is the 2.1.1 [`ocpi_types::v2_1_1::Token`] shape (`auth_id`
//! keying, `OTHER`/`RFID` only) and the slimmer [`AuthorizationInfo`] whose
//! [`LocationReferences`] keeps the 2.1.1-only `connector_ids`.
//!
//! Spec: OCPI 2.1.1 — *Tokens* module (§12), `specs/ocpi/2.1.1/OCPI_2.1.1.pdf`.

use std::sync::Arc;

use ocpi_client::{ClientError, OcpiClient};
use ocpi_server::{http::tokens_2_1_1_router, Tokens2111Config};
use ocpi_types::v2_1_1::{LocationReferences, Token, TokenType};
use ocpi_types::v2_2_1::{AllowedType, WhitelistType};
use ocpi_types::{
    chrono::{Duration, TimeZone as _},
    serde_json::{self, json},
    transport::PaginatedParams,
    DateTime, Utc,
};

/// The bearer the CPO presents on its Sender-interface (pull/authorize) calls.
const TOKEN: &str = "TOKEN_C_cpo_calls_emsp";

/// A spec-faithful OCPI 2.1.1 Token (§12.4.1): an RFID token. The shape
/// exercises the 2.1.1 quirks — bare `auth_id` (not `contract_id`), no
/// `country_code`/`party_id`, `OTHER`/`RFID` only.
fn token_json() -> serde_json::Value {
    json!({
        "uid": "12345678905880",
        "type": "RFID",
        "auth_id": "DE8ACC12E46L89",
        "visual_number": "DF000-2001-8999",
        "issuer": "TheNewMotion",
        "valid": true,
        "whitelist": "ALLOWED",
        "language": "it",
        "last_updated": "2018-12-10T17:25:10Z"
    })
}

/// Build a 2.1.1 Token from the spec fixture, overriding the fields that drive
/// deterministic pagination / authorize assertions.
fn make_token(uid: &str, valid: bool, ts: DateTime<Utc>) -> Token {
    let mut token: Token = serde_json::from_value(token_json()).expect("valid 2.1.1 token");
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

#[tokio::test]
async fn m7_tokens_2_1_1_list_put_patch_authorize_round_trip() {
    // ── eMSP server: tokens_2_1_1_router seeded with three valid Tokens. ────
    let store = Arc::new(Tokens2111Config::new());
    let base_ts = Utc
        .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
        .single()
        .expect("valid timestamp");
    for (i, uid) in ["TOKEN1", "TOKEN2", "TOKEN3"].iter().enumerate() {
        let ts = base_ts + Duration::seconds(i as i64);
        store.put("NL", "TNM", uid, TokenType::Rfid, make_token(uid, true, ts));
    }
    let (listener, base) = bind().await;
    serve(listener, tokens_2_1_1_router(store.clone()));

    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN);
    let tokens_url = format!("{base}/tokens");

    // ── GET paginated list — first page (limit 2 of 3). ────────────────────
    let (page1, meta) = client
        .get_tokens_2_1_1(
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
    // The 2.1.1 wire shape survives the client's deserialization.
    assert_eq!(page1[0].auth_id.as_str(), "DE8ACC12E46L89");
    assert_eq!(page1[0].token_type, TokenType::Rfid);
    let next = meta.next_url.expect("a second page should be advertised");
    assert!(next.contains("offset=2"), "next link: {next}");

    // ── Second page — the remaining Token. ─────────────────────────────────
    let (page2, _) = client
        .get_tokens_2_1_1(
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

    // ── PUT a new Token, then read it back from the store byte-for-byte. ───
    let new = make_token("TOKEN4", true, base_ts + Duration::seconds(10));
    client
        .put_token_2_1_1(&tokens_url, "NL", "TNM", "TOKEN4", TokenType::Rfid, &new)
        .await
        .expect("PUT Token should succeed");
    let stored = store
        .get("NL", "TNM", "TOKEN4", TokenType::Rfid)
        .expect("PUT Token should be in the store");
    assert_eq!(stored, new);

    // ── authorize the valid Token with a location scope — ALLOWED, and the
    //    2.1.1-only `connector_ids` survives the full client round-trip. ────
    let loc = LocationReferences {
        location_id: "LOC1".try_into().unwrap(),
        evse_uids: vec!["3256".try_into().unwrap()],
        connector_ids: vec!["1".try_into().unwrap(), "2".try_into().unwrap()],
    };
    let info = client
        .authorize_token_2_1_1(&tokens_url, "TOKEN4", TokenType::Rfid, Some(&loc))
        .await
        .expect("authorize of a valid token should return 200");
    assert_eq!(info.allowed, AllowedType::Allowed);
    let echoed = info
        .location
        .expect("ALLOWED echoes the requested location");
    assert_eq!(echoed.location_id.as_str(), "LOC1");
    assert_eq!(echoed.connector_ids.len(), 2);

    // ── PATCH the Token invalid, then authorize → BLOCKED, location cleared. ─
    client
        .patch_token_2_1_1(
            &tokens_url,
            "NL",
            "TNM",
            "TOKEN4",
            TokenType::Rfid,
            &json!({ "valid": false }),
        )
        .await
        .expect("PATCH Token should succeed");
    assert!(
        !store
            .get("NL", "TNM", "TOKEN4", TokenType::Rfid)
            .unwrap()
            .valid,
        "PATCH should have flipped `valid` to false"
    );
    let blocked = client
        .authorize_token_2_1_1(&tokens_url, "TOKEN4", TokenType::Rfid, Some(&loc))
        .await
        .expect("authorize of a known-but-invalid token still returns 200");
    assert_eq!(blocked.allowed, AllowedType::Blocked);
    assert!(
        blocked.location.is_none(),
        "a BLOCKED authorization must clear the location"
    );

    // ── 404 paths: authorize an unknown token, patch an unknown token. ─────
    let unknown = client
        .authorize_token_2_1_1(&tokens_url, "NOPE", TokenType::Rfid, None)
        .await;
    assert!(matches!(unknown, Err(ClientError::NotFound)));
    let patch_missing = client
        .patch_token_2_1_1(
            &tokens_url,
            "NL",
            "TNM",
            "NOPE",
            TokenType::Rfid,
            &json!({ "valid": false }),
        )
        .await;
    assert!(matches!(patch_missing, Err(ClientError::NotFound)));
}

/// The serialized 2.1.1 Token must carry `auth_id` and none of the 2.2+ fields,
/// and its whitelist enum is shared with 2.2.1.
#[test]
fn token_2_1_1_wire_has_auth_id_not_2_2_fields() {
    let ts = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).single().unwrap();
    let token = make_token("TOKEN1", true, ts);
    assert_eq!(token.whitelist, WhitelistType::Allowed);
    let obj = serde_json::to_value(&token).unwrap();
    let obj = obj.as_object().unwrap();

    assert_eq!(
        obj.get("auth_id").and_then(|v| v.as_str()),
        Some("DE8ACC12E46L89")
    );
    for absent in [
        "contract_id",
        "country_code",
        "party_id",
        "group_id",
        "default_profile_type",
        "energy_contract",
    ] {
        assert!(
            obj.get(absent).is_none(),
            "2.1.1 Token wire must not carry `{absent}`"
        );
    }
}

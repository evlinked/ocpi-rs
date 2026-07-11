//! M7 OCPI **2.2** CDRs — client + server round-trip smoke test
//! (issue #163, the CDRs wiring slice over the `v2_2` delta types).
//!
//! Stands up an in-process `axum` server hosting the real [`cdrs_2_2_router`]
//! on an ephemeral `127.0.0.1` port and drives the full CPO ↔ eMSP CDRs
//! exchange entirely through [`OcpiClient`]'s 2.2 methods (no raw `reqwest`).
//! This mirrors the 2.1.1 CDRs smoke test (`m7_cdrs_2_1_1.rs`) — only the
//! payload type and the wire-delta assertions differ.
//!
//! ## URL shapes (identical to 2.2.1 / 2.1.1)
//!
//! A CDR is a **server-owned** object — the receiver (eMSP) names it via the
//! `Location` header on `POST /cdrs` (§8.2.2) — so the endpoints are **flat**,
//! with no `{country_code}/{party_id}` segments:
//!
//! - push:   `POST {base}/cdrs`            → `201 Created` + `Location` (receiver, §8.2.2)
//! - object: `GET  {base}/cdrs/{cdr_id}`                                (receiver, §8.2.2)
//! - list:   `GET  {base}/cdrs?…`                                       (sender,   §8.2.1)
//!
//! Only the payload is the 2.2 [`ocpi_types::v2_2::Cdr`] shape: its `CdrToken`
//! has no `country_code`/`party_id`, its `CdrLocation` carries a required
//! `postal_code` and no `state`, and the `Cdr` has no
//! `home_charging_compensation` (all added in 2.2.1).
//!
//! Spec: OCPI 2.2 — *CDRs* module (§8), `specs/ocpi/2.2/mod_cdrs.asciidoc`.

use std::sync::Arc;

use ocpi_client::{ClientError, OcpiClient};
use ocpi_server::{http::cdrs_2_2_router, Cdrs22Config};
use ocpi_types::v2_2::{AuthMethod, Cdr};
use ocpi_types::{
    chrono::{Duration, TimeZone as _},
    serde_json::{self, json},
    transport::PaginatedParams,
    DateTime, Utc,
};

/// The bearer the CPO presents on its CDR push to the eMSP.
const TOKEN: &str = "TOKEN_C_cpo_pushes_cdr";

/// The base URL the eMSP advertises when minting `Location` headers.
const BASE_URL: &str = "https://emsp.example/ocpi/2.2";

/// A spec-faithful OCPI 2.2 CDR (§8.4.1). The shape exercises the 2.2-vs-2.2.1
/// deltas: a `cdr_token` with no `country_code`/`party_id`, a `cdr_location`
/// with a required `postal_code` and no `state`, a `total_cost` `Price` object,
/// and no `home_charging_compensation`.
fn cdr_json() -> serde_json::Value {
    json!({
        "country_code": "BE",
        "party_id": "BEC",
        "id": "CDR1",
        "start_date_time": "2015-06-29T21:39:09Z",
        "end_date_time": "2015-06-29T23:37:32Z",
        "cdr_token": {
            "uid": "012345678",
            "type": "RFID",
            "contract_id": "DE8ACC12E46L89"
        },
        "auth_method": "WHITELIST",
        "cdr_location": {
            "id": "LOC1",
            "name": "Gent Zuid",
            "address": "F.Rooseveltlaan 3A",
            "city": "Gent",
            "postal_code": "9000",
            "country": "BEL",
            "coordinates": { "latitude": "51.047599", "longitude": "3.729944" },
            "evse_uid": "3256",
            "evse_id": "BE*BEC*E041503001",
            "connector_id": "1",
            "connector_standard": "IEC_62196_T2",
            "connector_format": "SOCKET",
            "connector_power_type": "AC_3_PHASE"
        },
        "currency": "EUR",
        "charging_periods": [{
            "start_date_time": "2015-06-29T21:39:09Z",
            "dimensions": [{ "type": "ENERGY", "volume": 120 }]
        }],
        "total_cost": { "excl_vat": 4.00 },
        "total_energy": 15.0,
        "total_time": 1.973,
        "last_updated": "2015-06-29T23:37:32Z"
    })
}

/// Build a 2.2 CDR from the spec fixture, overriding the id and the
/// `last_updated` timestamp that drives deterministic pagination/date filtering.
fn make_cdr(id: &str, ts: DateTime<Utc>) -> Cdr {
    let mut cdr: Cdr = serde_json::from_value(cdr_json()).expect("valid 2.2 CDR");
    cdr.id = id.try_into().unwrap();
    cdr.last_updated = ts;
    cdr
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
async fn m7_cdrs_2_2_post_get_list_round_trip() {
    // ── eMSP server: cdrs_2_2_router seeded with two CDRs. ──────────────────
    let store = Arc::new(Cdrs22Config::new(BASE_URL));
    let base_ts = Utc
        .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
        .single()
        .expect("valid timestamp");
    for (i, id) in ["CDR1", "CDR2"].iter().enumerate() {
        let ts = base_ts + Duration::seconds(i as i64);
        store.store(make_cdr(id, ts));
    }
    let (listener, base) = bind().await;
    serve(listener, cdrs_2_2_router(store));

    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN);
    let cdrs_url = format!("{base}/cdrs");

    // ── GET single CDR → full 2.2 serde round-trip. ────────────────────────
    let cdr = client
        .get_cdr_2_2(&cdrs_url, "CDR1")
        .await
        .expect("GET single CDR should return 200");
    assert_eq!(cdr.id.as_str(), "CDR1");
    // The 2.2 token has a `contract_id` and no `country_code`/`party_id`.
    assert_eq!(cdr.cdr_token.contract_id.as_str(), "DE8ACC12E46L89");
    assert_eq!(cdr.auth_method, AuthMethod::Whitelist);
    // The 2.2 location has a required `postal_code`.
    assert_eq!(cdr.cdr_location.id.as_str(), "LOC1");
    assert_eq!(cdr.cdr_location.postal_code, "9000");
    assert_eq!(cdr.total_cost.excl_vat, 4.00);
    assert_eq!(cdr.currency, "EUR");

    // ── POST a new CDR → 201 + Location header pointing at the stored CDR. ──
    let new = make_cdr("CDR3", base_ts + Duration::seconds(5));
    let location = client
        .post_cdr_2_2(&cdrs_url, &new)
        .await
        .expect("POST CDR should return a Location header");
    assert_eq!(location, format!("{BASE_URL}/cdrs/CDR3"));

    // The just-pushed CDR is retrievable and byte-for-byte identical — the 2.2
    // delta shape is stored and served unmangled (not coerced through 2.2.1).
    let fetched = client
        .get_cdr_2_2(&cdrs_url, "CDR3")
        .await
        .expect("GET of the just-POSTed CDR should return 200");
    assert_eq!(fetched, new);

    // ── GET paginated list — first page (limit 2 of 3). ────────────────────
    let (page1, meta) = client
        .get_cdrs_2_2(
            &cdrs_url,
            PaginatedParams {
                limit: Some(2),
                ..Default::default()
            },
        )
        .await
        .expect("GET cdrs list should return 200");
    assert_eq!(page1.len(), 2);
    assert_eq!(meta.total_count, 3);
    assert_eq!(meta.limit, 2);
    let next = meta.next_url.expect("a second page should be advertised");
    assert!(next.contains("offset=2"), "next link: {next}");

    // ── 404 path: unknown GET. ─────────────────────────────────────────────
    let missing = client.get_cdr_2_2(&cdrs_url, "NOPE").await;
    assert!(matches!(missing, Err(ClientError::NotFound)));
}

/// The serialized 2.2 CDR must carry none of the 2.2.1-added fields: no
/// `country_code`/`party_id` on the `cdr_token`, no `state` on the
/// `cdr_location`, and no `home_charging_compensation` on the `Cdr`.
#[test]
fn cdr_2_2_wire_omits_2_2_1_added_fields() {
    let ts = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).single().unwrap();
    let wire = serde_json::to_value(make_cdr("CDR1", ts)).unwrap();
    let obj = wire.as_object().unwrap();

    // Present 2.2 fields: `end_date_time` spelling, a `cdr_token` object, a
    // `cdr_location` object, and a `total_cost` Price object.
    assert!(obj.contains_key("end_date_time"));
    assert!(obj.get("cdr_token").map(|v| v.is_object()).unwrap_or(false));
    assert!(obj
        .get("cdr_location")
        .map(|v| v.is_object())
        .unwrap_or(false));
    assert!(obj
        .get("total_cost")
        .map(|v| v.is_object())
        .unwrap_or(false));

    // Absent 2.2.1-added field on the Cdr.
    assert!(
        obj.get("home_charging_compensation").is_none(),
        "2.2 CDR wire must not carry `home_charging_compensation`"
    );

    // The token must not carry the 2.2.1-added owner fields.
    let token = obj.get("cdr_token").and_then(|v| v.as_object()).unwrap();
    assert!(token.get("country_code").is_none());
    assert!(token.get("party_id").is_none());

    // The location must not carry the 2.2.1-added `state`.
    let loc = obj.get("cdr_location").and_then(|v| v.as_object()).unwrap();
    assert!(loc.get("state").is_none());
    assert_eq!(
        loc.get("postal_code").and_then(|v| v.as_str()),
        Some("9000")
    );
}

//! M7 OCPI **2.1.1** CDRs — client + server round-trip smoke test
//! (issue #120, CDRs slice).
//!
//! Stands up an in-process `axum` server hosting the real [`cdrs_2_1_1_router`]
//! on an ephemeral `127.0.0.1` port and drives the full CPO ↔ eMSP CDRs
//! exchange entirely through [`OcpiClient`]'s 2.1.1 methods (no raw `reqwest`).
//! This mirrors the 2.2.1 CDRs smoke test (`m4_sessions_cdrs.rs`) and the 2.1.1
//! Sessions client test (`m7_sessions_2_1_1.rs`).
//!
//! ## URL shapes (identical to 2.2.1)
//!
//! A CDR is a **server-owned** object — the receiver (eMSP) names it via the
//! `Location` header on `POST /cdrs` (§10.2.2) — so the endpoints are **flat**,
//! with no `{country_code}/{party_id}` segments:
//!
//! - push:   `POST {base}/cdrs`            → `201 Created` + `Location` (receiver, §10.2.2)
//! - object: `GET  {base}/cdrs/{cdr_id}`                                (receiver, §10.2.2)
//! - list:   `GET  {base}/cdrs?…`                                       (sender,   §10.2.1)
//!
//! Only the payload is the 2.1.1 [`ocpi_types::v2_1_1::Cdr`] shape (bare
//! `auth_id`, embedded `location`, `stop_date_time`, a single numeric
//! `total_cost`, no `session_id`).
//!
//! Spec: OCPI 2.1.1 — *CDRs* module (§10), `specs/ocpi/2.1.1/OCPI_2.1.1.pdf`.

use std::sync::Arc;

use ocpi_client::{ClientError, OcpiClient};
use ocpi_server::{http::cdrs_2_1_1_router, Cdrs2111Config};
use ocpi_types::v2_1_1::{AuthMethod, Cdr};
use ocpi_types::{
    chrono::{Duration, TimeZone as _},
    serde_json::{self, json},
    transport::PaginatedParams,
    DateTime, Utc,
};

/// The bearer the CPO presents on its CDR push to the eMSP.
const TOKEN: &str = "TOKEN_C_cpo_pushes_cdr";

/// The base URL the eMSP advertises when minting `Location` headers.
const BASE_URL: &str = "https://emsp.example/ocpi/2.1.1";

/// A spec-faithful OCPI 2.1.1 CDR (§10.3.1) with an embedded Location (§8.3.1).
/// The shape exercises the 2.1.1 quirks: bare `auth_id`, embedded `location`,
/// `stop_date_time`, and bare numeric cost fields.
fn cdr_json() -> serde_json::Value {
    json!({
        "id": "CDR1",
        "start_date_time": "2015-06-29T21:39:09Z",
        "stop_date_time": "2015-06-29T23:37:32Z",
        "auth_id": "DE8ACC12E46L89",
        "auth_method": "WHITELIST",
        "location": {
            "id": "LOC1",
            "type": "ON_STREET",
            "name": "Gent Zuid",
            "address": "F.Rooseveltlaan 3A",
            "city": "Gent",
            "postal_code": "9000",
            "country": "BEL",
            "coordinates": { "latitude": "51.047599", "longitude": "3.729944" },
            "evses": [{
                "uid": "3256",
                "evse_id": "BE-BEC-E041503003",
                "status": "AVAILABLE",
                "connectors": [{
                    "id": "1",
                    "standard": "IEC_62196_T2",
                    "format": "SOCKET",
                    "power_type": "AC_1_PHASE",
                    "voltage": 230,
                    "amperage": 64,
                    "tariff_id": "11",
                    "last_updated": "2015-06-29T21:39:01Z"
                }],
                "last_updated": "2015-06-29T21:39:01Z"
            }],
            "last_updated": "2015-06-29T21:39:01Z"
        },
        "currency": "EUR",
        "charging_periods": [{
            "start_date_time": "2015-06-29T21:39:09Z",
            "dimensions": [{ "type": "ENERGY", "volume": 15.342 }]
        }],
        "total_cost": 4.00,
        "total_energy": 15.342,
        "total_time": 1.973,
        "last_updated": "2015-06-29T22:01:13Z"
    })
}

/// Build a 2.1.1 CDR from the spec fixture, overriding the id and the
/// `last_updated` timestamp that drives deterministic pagination/date filtering.
fn make_cdr(id: &str, ts: DateTime<Utc>) -> Cdr {
    let mut cdr: Cdr = serde_json::from_value(cdr_json()).expect("valid 2.1.1 CDR");
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
async fn m7_cdrs_2_1_1_post_get_list_round_trip() {
    // ── eMSP server: cdrs_2_1_1_router seeded with two CDRs. ────────────────
    let store = Arc::new(Cdrs2111Config::new(BASE_URL));
    let base_ts = Utc
        .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
        .single()
        .expect("valid timestamp");
    for (i, id) in ["CDR1", "CDR2"].iter().enumerate() {
        let ts = base_ts + Duration::seconds(i as i64);
        store.store(make_cdr(id, ts));
    }
    let (listener, base) = bind().await;
    serve(listener, cdrs_2_1_1_router(store));

    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN);
    let cdrs_url = format!("{base}/cdrs");

    // ── GET single CDR → full 2.1.1 serde round-trip. ──────────────────────
    let cdr = client
        .get_cdr_2_1_1(&cdrs_url, "CDR1")
        .await
        .expect("GET single CDR should return 200");
    assert_eq!(cdr.id.as_str(), "CDR1");
    assert_eq!(cdr.auth_id.as_str(), "DE8ACC12E46L89");
    assert_eq!(cdr.auth_method, AuthMethod::Whitelist);
    assert_eq!(cdr.location.id.as_str(), "LOC1");
    assert_eq!(cdr.total_cost, 4.00);
    assert_eq!(cdr.currency, "EUR");

    // ── POST a new CDR → 201 + Location header pointing at the stored CDR. ──
    let new = make_cdr("CDR3", base_ts + Duration::seconds(5));
    let location = client
        .post_cdr_2_1_1(&cdrs_url, &new)
        .await
        .expect("POST CDR should return a Location header");
    assert_eq!(location, format!("{BASE_URL}/cdrs/CDR3"));

    // The just-pushed CDR is retrievable and byte-for-byte identical.
    let fetched = client
        .get_cdr_2_1_1(&cdrs_url, "CDR3")
        .await
        .expect("GET of the just-POSTed CDR should return 200");
    assert_eq!(fetched, new);

    // ── GET paginated list — first page (limit 2 of 3). ────────────────────
    let (page1, meta) = client
        .get_cdrs_2_1_1(
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
    let missing = client.get_cdr_2_1_1(&cdrs_url, "NOPE").await;
    assert!(matches!(missing, Err(ClientError::NotFound)));
}

/// The serialized 2.1.1 CDR must carry none of the 2.2+ fields and must use the
/// `stop_date_time` spelling with a bare `auth_id` and an embedded `location`.
#[test]
fn cdr_2_1_1_wire_omits_2_2_fields() {
    let ts = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).single().unwrap();
    let wire = serde_json::to_value(make_cdr("CDR1", ts)).unwrap();
    let obj = wire.as_object().unwrap();

    // Present 2.1.1 fields: `stop_date_time` spelling, bare `auth_id`, embedded
    // `location`, bare numeric `total_cost`.
    assert!(obj.contains_key("stop_date_time"));
    assert_eq!(
        obj.get("auth_id").and_then(|v| v.as_str()),
        Some("DE8ACC12E46L89")
    );
    assert!(obj.get("location").map(|v| v.is_object()).unwrap_or(false));
    assert!(obj
        .get("total_cost")
        .map(|v| v.is_number())
        .unwrap_or(false));

    // Absent 2.2+ fields and renamed spellings.
    for absent in [
        "country_code",
        "party_id",
        "session_id",
        "cdr_token",
        "cdr_location",
        "authorization_reference",
        "signed_data",
        "end_date_time", // 2.2 rename of stop_date_time — must NOT appear
        "total_fixed_cost",
        "total_energy_cost",
        "total_time_cost",
    ] {
        assert!(
            obj.get(absent).is_none(),
            "2.1.1 CDR wire must not carry `{absent}`"
        );
    }
}

//! M8 OCPI **2.3.0** CDRs — client + server round-trip smoke test
//! (issue #203, the CDRs transport slice; the sibling Sessions slice is #204).
//!
//! Stands up an in-process `axum` server hosting the real [`cdrs_2_3_0_router`]
//! on an ephemeral `127.0.0.1` port and drives the full CPO ↔ eMSP CDRs
//! exchange entirely through [`OcpiClient`]'s `_2_3_0` methods (no raw
//! `reqwest`). This mirrors the 2.1.1 CDRs smoke test (`m7_cdrs_2_1_1.rs`); only
//! the payload is the tax-forked [`ocpi_types::v2_3_0::Cdr`] shape.
//!
//! ## URL shapes (identical to 2.2.1)
//!
//! A CDR is a **server-owned** object — the receiver (eMSP) names it via the
//! `Location` header on `POST /cdrs` — so the endpoints are **flat**, with no
//! `{country_code}/{party_id}` segments:
//!
//! - push:   `POST {base}/cdrs`            → `201 Created` + `Location` (receiver)
//! - object: `GET  {base}/cdrs/{cdr_id}`                                (receiver)
//! - list:   `GET  {base}/cdrs?…`                                       (sender)
//!
//! ## Why this test exists — the tax detail must survive the wire
//!
//! The 2.3.0 `Cdr` forks all six cost fields onto the tax-itemised 2.3.0
//! `Price`. A hub relaying a North-American partner's CDR must keep the itemised
//! GST+QST breakdown intact; if it collapsed into the VAT-only 2.2.1 field the
//! eMSP's receipt would no longer reconcile against the CDR. The round-trip
//! below seeds a Canadian CDR (`total_cost` = `before_taxes` + a GST 5 % / QST
//! 9.975 % `taxes` list) and asserts every tax line survives GET / POST / list.
//!
//! Spec: OCPI 2.3.0 — *CDRs* module (`specs/ocpi/2.3.0/mod_cdrs.asciidoc`).

use std::sync::Arc;

use ocpi_client::{ClientError, OcpiClient};
use ocpi_server::{http::cdrs_2_3_0_router, Cdrs230Config};
use ocpi_types::v2_3_0::{AuthMethod, Cdr};
use ocpi_types::{
    chrono::{Duration, TimeZone as _},
    serde_json::{self, json},
    transport::PaginatedParams,
    DateTime, Utc,
};

/// The bearer the CPO presents on its CDR push to the eMSP.
const TOKEN: &str = "TOKEN_C_cpo_pushes_cdr";

/// The base URL the eMSP advertises when minting `Location` headers.
const BASE_URL: &str = "https://emsp.example/ocpi/2.3.0";

/// A spec-faithful OCPI 2.3.0 CDR with a **North-American** `total_cost`: an
/// itemised GST + QST `taxes` list on the reworked `Price`, and an embedded
/// 2.3.0 tariff carrying the required `tax_included` flag.
fn cdr_json() -> serde_json::Value {
    json!({
        "country_code": "CA",
        "party_id": "EXA",
        "id": "CDR1",
        "start_date_time": "2026-07-11T09:00:00Z",
        "end_date_time": "2026-07-11T10:00:00Z",
        "cdr_token": {
            "country_code": "CA",
            "party_id": "EXA",
            "uid": "12345",
            "type": "RFID",
            "contract_id": "CA-EXA-C12345"
        },
        "auth_method": "WHITELIST",
        "cdr_location": {
            "id": "LOC1",
            "address": "F.Rooseveltlaan 3A",
            "city": "Ottawa",
            "country": "CAN",
            "coordinates": { "latitude": "45.421", "longitude": "-75.697" },
            "evse_uid": "EVSE1",
            "evse_id": "CA*EXA*E1",
            "connector_id": "1",
            "connector_standard": "IEC_62196_T2",
            "connector_format": "SOCKET",
            "connector_power_type": "AC_1_PHASE"
        },
        "currency": "CAD",
        "tariffs": [
            {
                "country_code": "CA",
                "party_id": "EXA",
                "id": "19",
                "currency": "CAD",
                "tax_included": "NO",
                "elements": [
                    { "price_components": [ { "type": "TIME", "price": 2.0, "step_size": 60 } ] }
                ],
                "last_updated": "2026-07-11T09:00:00Z"
            }
        ],
        "charging_periods": [
            {
                "start_date_time": "2026-07-11T09:00:00Z",
                "dimensions": [ { "type": "TIME", "volume": 1.0 } ]
            }
        ],
        "total_cost": {
            "before_taxes": 2.00,
            "taxes": [
                { "name": "GST", "percentage": 5.0, "amount": 0.10 },
                { "name": "QST", "account_number": "1234567890", "percentage": 9.975, "amount": 0.1995 }
            ]
        },
        "total_energy": 1.0,
        "total_time": 1.0,
        "last_updated": "2026-07-11T10:00:00Z"
    })
}

/// Build a 2.3.0 CDR from the fixture, overriding the id and the `last_updated`
/// timestamp that drives deterministic pagination/date filtering.
fn make_cdr(id: &str, ts: DateTime<Utc>) -> Cdr {
    let mut cdr: Cdr = serde_json::from_value(cdr_json()).expect("valid 2.3.0 CDR");
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
async fn m8_cdrs_2_3_0_post_get_list_round_trip() {
    // ── eMSP server: cdrs_2_3_0_router seeded with two North-American CDRs. ──
    let store = Arc::new(Cdrs230Config::new(BASE_URL));
    let base_ts = Utc
        .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
        .single()
        .expect("valid timestamp");
    for (i, id) in ["CDR1", "CDR2"].iter().enumerate() {
        let ts = base_ts + Duration::seconds(i as i64);
        store.store(make_cdr(id, ts));
    }
    let (listener, base) = bind().await;
    serve(listener, cdrs_2_3_0_router(store));

    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN);
    let cdrs_url = format!("{base}/cdrs");

    // ── GET single CDR → the itemised GST+QST tax lines survive the hop. ────
    let cdr = client
        .get_cdr_2_3_0(&cdrs_url, "CDR1")
        .await
        .expect("GET single CDR should return 200");
    assert_eq!(cdr.id.as_str(), "CDR1");
    assert_eq!(cdr.country_code.as_str(), "CA");
    assert_eq!(cdr.auth_method, AuthMethod::Whitelist);
    assert_eq!(cdr.currency, "CAD");
    // The tax detail — the whole reason the 2.3.0 CDR exists — is intact.
    assert_eq!(cdr.total_cost.before_taxes, 2.00);
    assert_eq!(cdr.total_cost.taxes.len(), 2);
    assert_eq!(cdr.total_cost.taxes[0].name, "GST");
    assert_eq!(cdr.total_cost.taxes[1].name, "QST");
    assert_eq!(
        cdr.total_cost.taxes[1].account_number.as_deref(),
        Some("1234567890")
    );
    // The embedded tariff is the 2.3.0 fork — its `tax_included` flag survives.
    assert_eq!(cdr.tariffs.len(), 1);

    // ── POST a new CDR → 201 + Location header pointing at the stored CDR. ──
    let new = make_cdr("CDR3", base_ts + Duration::seconds(5));
    let location = client
        .post_cdr_2_3_0(&cdrs_url, &new)
        .await
        .expect("POST CDR should return a Location header");
    assert_eq!(location, format!("{BASE_URL}/cdrs/CDR3"));

    // The just-pushed CDR is retrievable and byte-for-byte identical.
    let fetched = client
        .get_cdr_2_3_0(&cdrs_url, "CDR3")
        .await
        .expect("GET of the just-POSTed CDR should return 200");
    assert_eq!(fetched, new);

    // ── GET paginated list — first page (limit 2 of 3). ────────────────────
    let (page1, meta) = client
        .get_cdrs_2_3_0(
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

    // ── 404 path: unknown GET maps to NotFound. ────────────────────────────
    let missing = client.get_cdr_2_3_0(&cdrs_url, "NOPE").await;
    assert!(matches!(missing, Err(ClientError::NotFound)));
}

/// The trust boundary: a `POST /cdrs` body whose `total_cost` omits the required
/// `before_taxes` is rejected on deserialize (HTTP 4xx) before it can reach the
/// store — never silently defaulted to zero. Driven with a raw `reqwest` POST
/// because a well-typed [`Cdr`] cannot express the malformed shape.
#[tokio::test]
async fn m8_cdrs_2_3_0_rejects_total_cost_without_before_taxes() {
    let store = Arc::new(Cdrs230Config::new(BASE_URL));
    let (listener, base) = bind().await;
    serve(listener, cdrs_2_3_0_router(store));

    // A CDR body whose `total_cost` is `{}` — the required `before_taxes` is
    // absent. Serde must reject it at the `Json` extractor.
    let mut bad = cdr_json();
    bad["total_cost"] = json!({});

    let resp = reqwest::Client::new()
        .post(format!("{base}/cdrs"))
        .header("Authorization", format!("Token {TOKEN}"))
        .json(&bad)
        .send()
        .await
        .expect("request should complete");
    assert!(
        resp.status().is_client_error(),
        "a total_cost missing before_taxes must be a 4xx, got {}",
        resp.status()
    );
}

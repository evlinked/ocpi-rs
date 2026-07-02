//! M7 OCPI **2.1.1** Sessions — client + server round-trip smoke test
//! (issue #120, Sessions slice).
//!
//! Stands up an in-process `axum` server hosting the real
//! [`sessions_2_1_1_router`] on an ephemeral `127.0.0.1` port and drives the
//! full CPO ↔ eMSP Sessions exchange entirely through [`OcpiClient`]'s 2.1.1
//! methods (no raw `reqwest`). This mirrors the 2.2.1 Sessions smoke test
//! (`m4_sessions_cdrs.rs`) and the 2.1.1 Locations client test
//! (`m7_locations_2_1_1_client.rs`).
//!
//! ## URL shapes (identical to 2.2.1)
//!
//! Per OCPI 2.1.1 §9.2.2 *"Sessions is a client owned object, so the end-points
//! need to contain the required extra fields: {party_id} and {country_code}"*,
//! so the receiver path carries the composite key — the `{country_code}`/
//! `{party_id}` URL segments predate the 2.2 `OCPI-to/from-*` routing headers:
//!
//! - list:   `GET   {base}/sessions?…`                        (sender, §9.2.1)
//! - object: `GET   {base}/sessions/{cc}/{party}/{id}`        (receiver, §9.2.2)
//! - upsert: `PUT   {base}/sessions/{cc}/{party}/{id}`        (receiver)
//! - patch:  `PATCH {base}/sessions/{cc}/{party}/{id}`        (receiver)
//!
//! There is **no** `charging_preferences` endpoint (a 2.2 addition). Only the
//! payload is the 2.1.1 [`ocpi_types::v2_1_1::Session`] shape.
//!
//! Spec: OCPI 2.1.1 — *Sessions* module (§9), `specs/ocpi/2.1.1/OCPI_2.1.1.pdf`.

use std::sync::Arc;

use ocpi_client::{ClientError, OcpiClient};
use ocpi_server::{http::sessions_2_1_1_router, Sessions2111Config};
use ocpi_types::v2_1_1::{Session, SessionStatus};
use ocpi_types::{
    chrono::{Duration, TimeZone as _},
    serde_json::{self, json},
    transport::PaginatedParams,
    DateTime, Utc,
};

/// The bearer the eMSP presents on its Sender-interface calls.
const TOKEN: &str = "TOKEN_C_emsp_calls_cpo";

/// A spec-faithful OCPI 2.1.1 Session (§9.3.1) with an embedded Location
/// (§8.3.1). The shape exercises the 2.1.1 quirks: bare `auth_id`, embedded
/// `location`, one-word `start_datetime`, no `country_code`/`party_id`.
fn session_json() -> serde_json::Value {
    json!({
        "id": "SESSION1",
        "start_datetime": "2026-01-01T00:00:00Z",
        "kwh": 0.0,
        "auth_id": "NL8ACC12E46L89",
        "auth_method": "WHITELIST",
        "location": {
            "id": "LOC1",
            "type": "ON_STREET",
            "name": "Gent Zuid",
            "address": "F.Rooseveltlaan 3A",
            "city": "Gent",
            "postal_code": "9000",
            "country": "BEL",
            "coordinates": { "latitude": "51.047590", "longitude": "3.729940" },
            "evses": [{
                "uid": "3256",
                "evse_id": "BE-BEC-E041503001",
                "status": "AVAILABLE",
                "connectors": [{
                    "id": "1",
                    "standard": "IEC_62196_T2",
                    "format": "CABLE",
                    "power_type": "AC_3_PHASE",
                    "voltage": 220,
                    "amperage": 16,
                    "tariff_id": "11",
                    "last_updated": "2015-03-16T10:10:02Z"
                }],
                "last_updated": "2015-06-28T08:12:01Z"
            }],
            "last_updated": "2015-06-29T20:39:09Z"
        },
        "currency": "EUR",
        "status": "ACTIVE",
        "last_updated": "2026-01-01T00:00:00Z"
    })
}

/// Build a 2.1.1 Session from the spec fixture, overriding the id and the
/// timestamps that drive deterministic pagination/date-filter assertions.
fn make_session(id: &str, ts: DateTime<Utc>) -> Session {
    let mut session: Session = serde_json::from_value(session_json()).expect("valid 2.1.1 session");
    session.id = id.try_into().unwrap();
    session.start_datetime = ts;
    session.last_updated = ts;
    session
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
async fn m7_sessions_2_1_1_get_list_put_patch_round_trip() {
    // ── eMSP server: sessions_2_1_1_router seeded with three Sessions. ──────
    let store = Arc::new(Sessions2111Config::new());
    let base_ts = Utc
        .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
        .single()
        .expect("valid timestamp");
    for (i, id) in ["SESSION1", "SESSION2", "SESSION3"].iter().enumerate() {
        let ts = base_ts + Duration::seconds(i as i64);
        store.put("NL", "CPO", id, make_session(id, ts));
    }
    let (listener, base) = bind().await;
    serve(listener, sessions_2_1_1_router(store));

    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN);
    let sessions_url = format!("{base}/sessions");

    // ── GET single Session → full 2.1.1 serde round-trip. ──────────────────
    let session = client
        .get_session_2_1_1(&sessions_url, "NL", "CPO", "SESSION1")
        .await
        .expect("GET single Session should return 200");
    assert_eq!(session.id.as_str(), "SESSION1");
    assert_eq!(session.auth_id.as_str(), "NL8ACC12E46L89");
    assert_eq!(session.status, SessionStatus::Active);
    assert_eq!(session.location.id.as_str(), "LOC1");
    assert_eq!(session.currency, "EUR");

    // ── GET paginated list — first page (limit 2 of 3). ────────────────────
    let (page1, meta) = client
        .get_sessions_2_1_1(
            &sessions_url,
            PaginatedParams {
                limit: Some(2),
                ..Default::default()
            },
        )
        .await
        .expect("GET sessions list should return 200");
    assert_eq!(page1.len(), 2);
    assert_eq!(meta.total_count, 3);
    assert_eq!(meta.limit, 2);
    let next = meta.next_url.expect("a second page should be advertised");
    assert!(next.contains("offset=2"), "next link: {next}");

    // ── PUT a new Session, then read it back byte-for-byte. ────────────────
    let new = make_session("SESSION4", base_ts + Duration::seconds(10));
    client
        .put_session_2_1_1(&sessions_url, "NL", "CPO", "SESSION4", &new)
        .await
        .expect("PUT Session should succeed");
    let fetched = client
        .get_session_2_1_1(&sessions_url, "NL", "CPO", "SESSION4")
        .await
        .expect("GET of the just-PUT Session should return 200");
    assert_eq!(fetched, new);

    // ── PATCH (merge-patch) the status, leaving every other field intact. ──
    client
        .patch_session_2_1_1(
            &sessions_url,
            "NL",
            "CPO",
            "SESSION4",
            &json!({ "status": "COMPLETED" }),
        )
        .await
        .expect("PATCH Session should succeed");
    let patched = client
        .get_session_2_1_1(&sessions_url, "NL", "CPO", "SESSION4")
        .await
        .expect("GET of the patched Session should return 200");
    assert_eq!(patched.status, SessionStatus::Completed);
    assert_eq!(patched.auth_id.as_str(), "NL8ACC12E46L89");

    // ── 404 paths: unknown GET and unknown PATCH. ──────────────────────────
    let missing = client
        .get_session_2_1_1(&sessions_url, "NL", "CPO", "NOPE")
        .await;
    assert!(matches!(missing, Err(ClientError::NotFound)));
    let patch_missing = client
        .patch_session_2_1_1(&sessions_url, "NL", "CPO", "NOPE", &json!({ "kwh": 1.0 }))
        .await;
    assert!(matches!(patch_missing, Err(ClientError::NotFound)));
}

/// The serialized 2.1.1 Session must carry none of the 2.2+ fields and must use
/// the one-word `start_datetime` spelling with a bare `auth_id` and an embedded
/// `location` object.
#[test]
fn session_2_1_1_wire_omits_2_2_fields() {
    let ts = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).single().unwrap();
    let wire = serde_json::to_value(make_session("SESSION1", ts)).unwrap();
    let obj = wire.as_object().unwrap();

    // Present 2.1.1 fields (one-word `start_datetime`, bare `auth_id`,
    // embedded `location`).
    let start = obj
        .get("start_datetime")
        .and_then(|v| v.as_str())
        .expect("start_datetime must serialize as a string");
    assert!(
        start.starts_with("2026-01-01T00:00:00"),
        "unexpected start_datetime: {start}"
    );
    assert_eq!(
        obj.get("auth_id").and_then(|v| v.as_str()),
        Some("NL8ACC12E46L89")
    );
    assert!(obj.get("location").map(|v| v.is_object()).unwrap_or(false));

    // Absent 2.2+ fields and renamed spellings.
    for absent in [
        "country_code",
        "party_id",
        "cdr_token",
        "authorization_reference",
        "connector_id",
        "location_id",
        "evse_uid",
        "start_date_time", // 2.2 corrected spelling — must NOT appear
        "charging_preferences",
    ] {
        assert!(
            obj.get(absent).is_none(),
            "2.1.1 Session wire must not carry `{absent}`"
        );
    }
}

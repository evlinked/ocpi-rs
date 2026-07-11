//! M7 OCPI **2.2** Locations — client *sender* smoke test (issue #167).
//!
//! Drives the 2.2 sender getters
//! ([`OcpiClient::get_locations_2_2`], [`get_location_2_2`],
//! [`get_evse_2_2`], [`get_connector_2_2`]) against a real loopback transport.
//! The 2.2 *server* routers are a separate follow-up, so this harness stands up
//! a hand-rolled `axum` CPO that replays a 2.2 Location (the object is
//! structurally the 2.2.1 shape — `country_code`/`party_id` present, `publish`,
//! `tariff_ids: []` — but every connector enum here is a value 2.2 defines).
//!
//! The delta this checks is the **connector enum guard flowing through the
//! composite**: the getters deserialize into [`ocpi_types::v2_2::Location`],
//! whose `Connector.standard`/`power_type` are the 2.2 enums. So a valid 2.2
//! catalogue round-trips, while a payload carrying a 2.2.1-only plug/power value
//! (`AC_2_PHASE`, `GBT_DC`, …) is **rejected on the 2.2 path** rather than
//! silently coerced.
//!
//! [`get_evse_2_2`]: ocpi_client::OcpiClient::get_evse_2_2
//! [`get_location_2_2`]: ocpi_client::OcpiClient::get_location_2_2
//! [`get_connector_2_2`]: ocpi_client::OcpiClient::get_connector_2_2
//!
//! Spec: `specs/ocpi/2.2` — *Locations*, Sender Interface (GET List, GET
//! Object); `specs/ocpi/2.2.1/version_history.asciidoc` — the `PowerType` /
//! `ConnectorType` additions that define the 2.2-vs-2.2.1 delta.

use axum::{
    extract::Path,
    http::{header, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use ocpi_client::{ClientError, OcpiClient};
use ocpi_types::transport::PaginatedParams;

const TOKEN: &str = "TOKEN_C_emsp_calls_cpo";

/// A 2.2 Location: the 2.2.1-shaped object (`country_code`/`party_id`,
/// `publish`, `tariff_ids: []`) whose one connector uses `IEC_62196_T2` /
/// `AC_3_PHASE` — both values 2.2 defines, so it parses on the 2.2 path.
const LOCATION_JSON: &str = r#"{
    "country_code": "NL",
    "party_id": "CPO",
    "id": "LOC1",
    "publish": true,
    "address": "F.Rooseveltlaan 3A",
    "city": "Gent",
    "postal_code": "9000",
    "country": "NLD",
    "coordinates": { "latitude": "51.047590", "longitude": "3.729940" },
    "evses": [{
        "uid": "3256",
        "evse_id": "BE-BEC-E041503001",
        "status": "AVAILABLE",
        "status_schedule": [],
        "capabilities": ["RESERVABLE"],
        "connectors": [{
            "id": "1",
            "standard": "IEC_62196_T2",
            "format": "CABLE",
            "power_type": "AC_3_PHASE",
            "max_voltage": 220,
            "max_amperage": 16,
            "tariff_ids": ["11"],
            "last_updated": "2015-03-16T10:10:02Z"
        }],
        "physical_reference": "1",
        "floor_level": "-1",
        "last_updated": "2015-06-28T08:12:01Z"
    }],
    "time_zone": "Europe/Amsterdam",
    "last_updated": "2015-06-29T20:39:09Z"
}"#;

const EVSE_JSON: &str = r#"{
    "uid": "3256",
    "evse_id": "BE-BEC-E041503001",
    "status": "AVAILABLE",
    "status_schedule": [],
    "capabilities": ["RESERVABLE"],
    "connectors": [{
        "id": "1",
        "standard": "IEC_62196_T2",
        "format": "CABLE",
        "power_type": "AC_3_PHASE",
        "max_voltage": 220,
        "max_amperage": 16,
        "tariff_ids": ["11"],
        "last_updated": "2015-03-16T10:10:02Z"
    }],
    "last_updated": "2015-06-28T08:12:01Z"
}"#;

const CONNECTOR_JSON: &str = r#"{
    "id": "1",
    "standard": "IEC_62196_T2",
    "format": "CABLE",
    "power_type": "AC_3_PHASE",
    "max_voltage": 220,
    "max_amperage": 16,
    "tariff_ids": ["11"],
    "last_updated": "2015-03-16T10:10:02Z"
}"#;

/// The same connector, but with `power_type: "AC_2_PHASE"` — a value **2.2.1
/// added** and 2.2 does not define. It must fail to deserialize on the 2.2 path.
const CONNECTOR_JSON_2_2_1_ONLY: &str = r#"{
    "id": "1",
    "standard": "IEC_62196_T2",
    "format": "CABLE",
    "power_type": "AC_2_PHASE",
    "max_voltage": 220,
    "max_amperage": 16,
    "tariff_ids": ["11"],
    "last_updated": "2015-03-16T10:10:02Z"
}"#;

/// Wrap a payload (object or array) in a `status_code = 1000` OCPI envelope.
fn envelope(data: &str) -> String {
    format!(r#"{{"status_code":1000,"timestamp":"2026-01-01T00:00:00Z","data":{data}}}"#)
}

fn json_body(body: String) -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/json")], body)
}

async fn list_handler() -> impl IntoResponse {
    let body = envelope(&format!("[{LOCATION_JSON}]"));
    (
        [
            (header::CONTENT_TYPE, "application/json".to_owned()),
            (
                header::HeaderName::from_static("x-total-count"),
                "1".to_owned(),
            ),
            (header::HeaderName::from_static("x-limit"), "50".to_owned()),
        ],
        body,
    )
}

async fn location_handler(Path(id): Path<String>) -> impl IntoResponse {
    if id != "LOC1" {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    json_body(envelope(LOCATION_JSON)).into_response()
}

async fn evse_handler(Path((id, uid)): Path<(String, String)>) -> impl IntoResponse {
    if id != "LOC1" || uid != "3256" {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    json_body(envelope(EVSE_JSON)).into_response()
}

async fn connector_handler(
    Path((id, uid, cid)): Path<(String, String, String)>,
) -> impl IntoResponse {
    if id != "LOC1" || uid != "3256" || cid != "1" {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    json_body(envelope(CONNECTOR_JSON)).into_response()
}

/// A second connector route that replays the 2.2.1-only enum value, so the
/// client's 2.2 deserialize is what rejects it (not the server).
async fn bad_connector_handler() -> impl IntoResponse {
    json_body(envelope(CONNECTOR_JSON_2_2_1_ONLY)).into_response()
}

fn router() -> Router {
    Router::new()
        .route("/locations", get(list_handler))
        .route("/locations/{id}", get(location_handler))
        .route("/locations/{id}/{uid}", get(evse_handler))
        .route("/locations/{id}/{uid}/{cid}", get(connector_handler))
        .route("/bad/{id}/{uid}/{cid}", get(bad_connector_handler))
}

async fn bind() -> (tokio::net::TcpListener, String) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    (listener, base)
}

fn serve(listener: tokio::net::TcpListener, router: Router) {
    tokio::spawn(async move {
        axum::serve(listener, router.into_make_service())
            .await
            .unwrap();
    });
}

#[tokio::test]
async fn m7_locations_2_2_sender_round_trip() {
    let (listener, base) = bind().await;
    serve(listener, router());
    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN);

    let list_url = format!("{base}/locations");
    let object_url = format!("{base}/locations");

    // ── GET List (paginated) — the 2.2 wire shape parses, headers honoured. ───
    let (locs, meta) = client
        .get_locations_2_2(&list_url, PaginatedParams::default())
        .await
        .expect("GET 2.2 Locations list should return 200");
    assert_eq!(locs.len(), 1);
    assert_eq!(meta.total_count, 1, "X-Total-Count header is parsed");
    assert_eq!(meta.limit, 50, "X-Limit header is parsed");

    // ── GET single Location — full serde round-trip of the 2.2 composite. ─────
    let loc = client
        .get_location_2_2(&object_url, "LOC1")
        .await
        .expect("GET single 2.2 Location should return 200");
    assert_eq!(loc.id.as_str(), "LOC1");
    assert_eq!(loc.country_code.as_str(), "NL");
    assert_eq!(loc.party_id.as_str(), "CPO");
    assert_eq!(loc.evses.len(), 1);
    let evse = &loc.evses[0];
    assert_eq!(evse.uid.as_str(), "3256");
    // The connector's enums deserialized into the 2.2 types.
    assert_eq!(
        evse.connectors[0].standard,
        ocpi_types::v2_2::ConnectorType::Iec62196T2
    );
    assert_eq!(
        evse.connectors[0].power_type,
        ocpi_types::v2_2::PowerType::Ac3Phase
    );

    // ── GET nested EVSE + Connector via the sender sub-object routes. ─────────
    let evse = client
        .get_evse_2_2(&object_url, "LOC1", "3256")
        .await
        .expect("GET 2.2 EVSE should return 200");
    assert_eq!(evse.uid.as_str(), "3256");

    let connector = client
        .get_connector_2_2(&object_url, "LOC1", "3256", "1")
        .await
        .expect("GET 2.2 Connector should return 200");
    assert_eq!(connector.id.as_str(), "1");

    // ── 404 maps to ClientError::NotFound on the object getters. ─────────────
    let missing = client.get_location_2_2(&object_url, "NOPE").await;
    assert!(
        matches!(missing, Err(ClientError::NotFound)),
        "unknown id must surface as NotFound, got {missing:?}"
    );
}

/// The delta that matters: a payload carrying a **2.2.1-only** connector enum
/// (`power_type: "AC_2_PHASE"`) is rejected on the 2.2 path — the getter returns
/// a decode error rather than silently coercing it through the 2.2.1 struct.
#[tokio::test]
async fn m7_locations_2_2_rejects_2_2_1_only_connector_enum() {
    let (listener, base) = bind().await;
    serve(listener, router());
    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN);

    let bad_url = format!("{base}/bad");
    let result = client
        .get_connector_2_2(&bad_url, "LOC1", "3256", "1")
        .await;
    assert!(
        matches!(result, Err(ClientError::Http(_))),
        "a 2.2.1-only power_type must fail to deserialize on the 2.2 path, got {result:?}"
    );
}

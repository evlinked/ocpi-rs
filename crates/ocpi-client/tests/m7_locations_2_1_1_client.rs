//! M7 OCPI **2.1.1** Locations — client *sender* smoke test (issue #113).
//!
//! Drives the 2.1.1 sender getters
//! ([`OcpiClient::get_locations_2_1_1`], [`get_location_2_1_1`],
//! [`get_evse_2_1_1`], [`get_connector_2_1_1`]) against a real loopback
//! transport. The 2.1.1 *server receiver* router is a separate follow-up, so
//! this harness stands up a hand-rolled `axum` CPO that replays the OCPI 2.1.1
//! spec Location example (§8.3.1.1) verbatim. That makes the test a faithful
//! check of the **2.1.1 wire shape** the client must parse: `type` required,
//! **no** `country_code`/`party_id` on the Location, and a **singular**
//! `tariff_id` per connector (vs. the 2.2.1 `tariff_ids: []`).
//!
//! [`get_evse_2_1_1`]: ocpi_client::OcpiClient::get_evse_2_1_1
//! [`get_location_2_1_1`]: ocpi_client::OcpiClient::get_location_2_1_1
//! [`get_connector_2_1_1`]: ocpi_client::OcpiClient::get_connector_2_1_1
//!
//! Spec: `specs/ocpi/2.1.1` — *Locations*, Sender Interface (GET List, GET
//! Object).

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

/// The OCPI 2.1.1 spec Location example (§8.3.1.1), trimmed to one EVSE +
/// connector. Note: required `type`, no `country_code`/`party_id`, singular
/// `tariff_id`.
const LOCATION_JSON: &str = r#"{
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
        "status_schedule": [],
        "capabilities": ["RESERVABLE"],
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
        "physical_reference": "1",
        "floor_level": "-1",
        "last_updated": "2015-06-28T08:12:01Z"
    }],
    "operator": { "name": "BeCharged" },
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
        "voltage": 220,
        "amperage": 16,
        "tariff_id": "11",
        "last_updated": "2015-03-16T10:10:02Z"
    }],
    "last_updated": "2015-06-28T08:12:01Z"
}"#;

const CONNECTOR_JSON: &str = r#"{
    "id": "1",
    "standard": "IEC_62196_T2",
    "format": "CABLE",
    "power_type": "AC_3_PHASE",
    "voltage": 220,
    "amperage": 16,
    "tariff_id": "11",
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

fn router() -> Router {
    Router::new()
        .route("/locations", get(list_handler))
        .route("/locations/{id}", get(location_handler))
        .route("/locations/{id}/{uid}", get(evse_handler))
        .route("/locations/{id}/{uid}/{cid}", get(connector_handler))
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
async fn m7_locations_2_1_1_sender_round_trip() {
    let (listener, base) = bind().await;
    serve(listener, router());
    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN);

    let list_url = format!("{base}/locations");
    let object_url = format!("{base}/locations");

    // ── GET List (paginated) — the 2.1.1 wire shape parses, headers honoured. ─
    let (locs, meta) = client
        .get_locations_2_1_1(&list_url, PaginatedParams::default())
        .await
        .expect("GET 2.1.1 Locations list should return 200");
    assert_eq!(locs.len(), 1);
    assert_eq!(meta.total_count, 1, "X-Total-Count header is parsed");
    assert_eq!(meta.limit, 50, "X-Limit header is parsed");

    // ── GET single Location — full serde round-trip of the 2.1.1 shape. ───────
    let loc = client
        .get_location_2_1_1(&object_url, "LOC1")
        .await
        .expect("GET single 2.1.1 Location should return 200");
    assert_eq!(loc.id.as_str(), "LOC1");
    assert_eq!(loc.country, "BEL");
    assert_eq!(loc.evses.len(), 1);
    let evse = &loc.evses[0];
    assert_eq!(evse.uid.as_str(), "3256");
    // The 2.1.1 connector carries a *singular* tariff_id.
    assert_eq!(
        evse.connectors[0].tariff_id.as_ref().unwrap().as_str(),
        "11"
    );

    // ── GET nested EVSE + Connector via the sender sub-object routes. ─────────
    let evse = client
        .get_evse_2_1_1(&object_url, "LOC1", "3256")
        .await
        .expect("GET 2.1.1 EVSE should return 200");
    assert_eq!(evse.uid.as_str(), "3256");

    let connector = client
        .get_connector_2_1_1(&object_url, "LOC1", "3256", "1")
        .await
        .expect("GET 2.1.1 Connector should return 200");
    assert_eq!(connector.id.as_str(), "1");

    // ── 404 maps to ClientError::NotFound on the object getters. ─────────────
    let missing = client.get_location_2_1_1(&object_url, "NOPE").await;
    assert!(
        matches!(missing, Err(ClientError::NotFound)),
        "unknown id must surface as NotFound, got {missing:?}"
    );
}

//! M8 OCPI **2.3.0** Locations — client *sender* smoke test (slice of #177).
//!
//! Drives the 2.3.0 sender getters
//! ([`OcpiClient::get_locations_2_3_0`], [`get_location_2_3_0`],
//! [`get_evse_2_3_0`], [`get_connector_2_3_0`]) against a real loopback
//! transport. The 2.3.0 *server* routers are the remaining follow-up slice, so
//! this harness stands up a hand-rolled `axum` CPO that replays a 2.3.0 Location
//! carrying the **additive 2.3.0 fields**: `Location.{parking_places,
//! help_phone}`, `Evse.{parking, accepted_service_providers}`, and the ISO 15118
//! `Connector.capabilities`.
//!
//! The delta this checks is that those new fields **survive the round-trip on
//! the 2.3.0 path**: the getters deserialize into [`ocpi_types::v2_3_0::Location`],
//! so a 2.3.0 partner's parking/15118/AFIR data reaches the caller rather than
//! being silently dropped through the 2.2.1 struct. The negative test confirms
//! the crate's core promise still holds on the additive path — an undocumented
//! `ConnectorCapability` value is rejected on deserialize, never coerced.
//!
//! [`get_evse_2_3_0`]: ocpi_client::OcpiClient::get_evse_2_3_0
//! [`get_location_2_3_0`]: ocpi_client::OcpiClient::get_location_2_3_0
//! [`get_connector_2_3_0`]: ocpi_client::OcpiClient::get_connector_2_3_0
//!
//! Spec: `specs/ocpi/2.3.0/mod_locations.asciidoc` — Sender Interface (GET List,
//! GET Object); the Parking object, EVSE `parking`/`accepted_service_providers`,
//! and Connector ISO 15118 `capabilities` additions that define the delta.

use axum::{
    extract::Path,
    http::{header, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use ocpi_client::{ClientError, OcpiClient};
use ocpi_types::transport::PaginatedParams;
use ocpi_types::v2_3_0::{ConnectorCapability, ConnectorType, PowerType, VehicleType};

const TOKEN: &str = "TOKEN_C_emsp_calls_cpo";

/// A 2.3.0 Location: the 2.2.1-shaped object plus every additive 2.3.0 field —
/// a `parking_places` entry (with a `DISABLED` vehicle type), an `Evse.parking`
/// reference and `accepted_service_providers`, a `help_phone`, and an ISO 15118
/// `capabilities` flag on the connector.
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
    "parking_type": "ON_STREET",
    "parking_places": [{
        "id": "space-1",
        "vehicle_types": ["PERSONAL_VEHICLE", "DISABLED"],
        "restricted_to_type": true,
        "reservation_required": false,
        "direction": "PARALLEL"
    }],
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
            "capabilities": ["ISO_15118_20_PLUG_AND_CHARGE"],
            "last_updated": "2015-03-16T10:10:02Z"
        }],
        "parking": [{ "parking_id": "space-1", "evse_position": "LEFT" }],
        "accepted_service_providers": ["Contract Provider X", "Contract Provider Y"],
        "physical_reference": "1",
        "floor_level": "-1",
        "last_updated": "2015-06-28T08:12:01Z"
    }],
    "time_zone": "Europe/Amsterdam",
    "help_phone": "+31201234567",
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
        "capabilities": ["ISO_15118_20_PLUG_AND_CHARGE"],
        "last_updated": "2015-03-16T10:10:02Z"
    }],
    "parking": [{ "parking_id": "space-1", "evse_position": "LEFT" }],
    "accepted_service_providers": ["Contract Provider X"],
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
    "capabilities": ["ISO_15118_2_PLUG_AND_CHARGE", "ISO_15118_20_PLUG_AND_CHARGE"],
    "last_updated": "2015-03-16T10:10:02Z"
}"#;

/// The same connector, but with an undocumented `capabilities` value. It must
/// fail to deserialize — the additive 2.3.0 path stays strict on enum values.
const CONNECTOR_JSON_BAD_CAPABILITY: &str = r#"{
    "id": "1",
    "standard": "IEC_62196_T2",
    "format": "CABLE",
    "power_type": "AC_3_PHASE",
    "max_voltage": 220,
    "max_amperage": 16,
    "tariff_ids": ["11"],
    "capabilities": ["ISO_15118_99_TELEPORT"],
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

/// A second connector route replaying an undocumented `capabilities` value, so
/// the client's 2.3.0 deserialize is what rejects it (not the server).
async fn bad_connector_handler() -> impl IntoResponse {
    json_body(envelope(CONNECTOR_JSON_BAD_CAPABILITY)).into_response()
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
async fn m8_locations_2_3_0_sender_round_trip() {
    let (listener, base) = bind().await;
    serve(listener, router());
    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN);

    let list_url = format!("{base}/locations");
    let object_url = format!("{base}/locations");

    // ── GET List (paginated) — the 2.3.0 wire shape parses, headers honoured. ──
    let (locs, meta) = client
        .get_locations_2_3_0(&list_url, PaginatedParams::default())
        .await
        .expect("GET 2.3.0 Locations list should return 200");
    assert_eq!(locs.len(), 1);
    assert_eq!(meta.total_count, 1, "X-Total-Count header is parsed");
    assert_eq!(meta.limit, 50, "X-Limit header is parsed");

    // ── GET single Location — the additive 2.3.0 fields survive the round-trip. ─
    let loc = client
        .get_location_2_3_0(&object_url, "LOC1")
        .await
        .expect("GET single 2.3.0 Location should return 200");
    assert_eq!(loc.id.as_str(), "LOC1");
    // Location-level 2.3.0 additions.
    assert_eq!(loc.help_phone.as_ref().unwrap().as_str(), "+31201234567");
    assert_eq!(loc.parking_places.len(), 1);
    assert_eq!(
        loc.parking_places[0].vehicle_types,
        vec![VehicleType::PersonalVehicle, VehicleType::Disabled],
        "the parking-report vehicle types survive on the 2.3.0 path"
    );
    // EVSE-level 2.3.0 additions.
    assert_eq!(loc.evses.len(), 1);
    let evse = &loc.evses[0];
    assert_eq!(evse.parking.len(), 1);
    assert_eq!(evse.parking[0].parking_id.as_str(), "space-1");
    assert_eq!(
        evse.accepted_service_providers,
        vec!["Contract Provider X", "Contract Provider Y"],
        "the AFIR accepted-eMSP list reaches the caller"
    );
    // Connector-level 2.3.0 addition: the ISO 15118 flag, plus the base enums.
    let connector = &evse.connectors[0];
    assert_eq!(connector.standard, ConnectorType::Iec62196T2);
    assert_eq!(connector.power_type, PowerType::Ac3Phase);
    assert_eq!(
        connector.capabilities,
        vec![ConnectorCapability::Iso1511820PlugAndCharge],
        "the ISO 15118 Plug-and-Charge flag is not dropped"
    );

    // ── GET nested EVSE + Connector via the sender sub-object routes. ─────────
    let evse = client
        .get_evse_2_3_0(&object_url, "LOC1", "3256")
        .await
        .expect("GET 2.3.0 EVSE should return 200");
    assert_eq!(evse.uid.as_str(), "3256");
    assert_eq!(evse.parking[0].parking_id.as_str(), "space-1");

    let connector = client
        .get_connector_2_3_0(&object_url, "LOC1", "3256", "1")
        .await
        .expect("GET 2.3.0 Connector should return 200");
    assert_eq!(connector.id.as_str(), "1");
    assert_eq!(
        connector.capabilities,
        vec![
            ConnectorCapability::Iso1511802PlugAndCharge,
            ConnectorCapability::Iso1511820PlugAndCharge,
        ]
    );

    // ── 404 maps to ClientError::NotFound on the object getters. ─────────────
    let missing = client.get_location_2_3_0(&object_url, "NOPE").await;
    assert!(
        matches!(missing, Err(ClientError::NotFound)),
        "unknown id must surface as NotFound, got {missing:?}"
    );
}

/// The crate's core promise on the additive path: an undocumented
/// `ConnectorCapability` value is rejected on deserialize — the getter returns
/// a decode error rather than silently coercing it away.
#[tokio::test]
async fn m8_locations_2_3_0_rejects_unknown_connector_capability() {
    let (listener, base) = bind().await;
    serve(listener, router());
    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN);

    let bad_url = format!("{base}/bad");
    let result = client
        .get_connector_2_3_0(&bad_url, "LOC1", "3256", "1")
        .await;
    assert!(
        matches!(result, Err(ClientError::Http(_))),
        "an unknown ConnectorCapability must fail to deserialize, got {result:?}"
    );
}

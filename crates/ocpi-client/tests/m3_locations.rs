//! M3 Locations end-to-end smoke test (issue #32).
//!
//! Walks a full CPO → eMSP Locations exchange over a real loopback transport:
//! one in-process `axum` server (the CPO) hosting [`locations_router`] is stood
//! up on an ephemeral `127.0.0.1` port and driven entirely through
//! [`OcpiClient`] (no raw `reqwest`).
//!
//! ## URL shapes
//!
//! [`locations_router`] exposes the **list** route at the sender path
//! `GET /locations` and the per-object routes under the receiver path
//! `GET /locations/{country_code}/{party_id}/{location_id}[/{evse}/{connector}]`.
//! The [`OcpiClient`] object getters simply append path segments to the base
//! URL they are handed, so the test feeds them two bases:
//!
//! - list:   `{base}/locations`            → `GET /locations?…`
//! - object: `{base}/locations/NL/CPO`      → `GET /locations/NL/CPO/{id}[/…]`
//!
//! This composes the sender-style client getters onto the router's receiver
//! object routes, exercising the real client HTTP path-building, the real axum
//! routing, and a full serde round-trip of [`Location`]/[`Evse`]/[`Connector`].
//!
//! Routing headers (`OCPI-from/to-party-id/country-code`) are accepted but not
//! enforced by `locations_router`; uniform client-side propagation of those
//! headers is tracked separately by #64 and is out of scope here.
//!
//! Spec: `specs/ocpi/2.2.1/mod_locations.asciidoc` — §Sender + §Receiver
//! Interface (GET List, GET Object).

use std::sync::Arc;

use ocpi_client::{ClientError, OcpiClient};
use ocpi_server::{http::locations_router, LocationsConfig};
use ocpi_types::{
    chrono::{Duration, TimeZone as _},
    transport::PaginatedParams,
    CiString2, CiString3, CiString36, Connector, ConnectorFormat, ConnectorType, DateTime, Evse,
    GeoLocation, Location, PowerType, Status, Utc,
};

/// The bearer the eMSP presents on its Sender-interface GETs.
const TOKEN: &str = "TOKEN_C_emsp_calls_cpo";

/// A single AC Type-2 socket connector, mirroring the spec example.
fn make_connector(id: &str, ts: DateTime<Utc>) -> Connector {
    Connector {
        id: CiString36::try_from(id).unwrap(),
        standard: ConnectorType::Iec62196T2,
        format: ConnectorFormat::Socket,
        power_type: PowerType::Ac3Phase,
        max_voltage: 400,
        max_amperage: 16,
        max_electric_power: None,
        tariff_ids: Vec::new(),
        terms_and_conditions: None,
        last_updated: ts,
    }
}

/// An available EVSE carrying one connector.
fn make_evse(uid: &str, ts: DateTime<Utc>) -> Evse {
    Evse {
        uid: CiString36::try_from(uid).unwrap(),
        evse_id: None,
        status: Status::Available,
        status_schedule: Vec::new(),
        capabilities: Vec::new(),
        connectors: vec![make_connector("1", ts)],
        floor_level: None,
        coordinates: None,
        physical_reference: None,
        directions: Vec::new(),
        parking_restrictions: Vec::new(),
        images: Vec::new(),
        last_updated: ts,
    }
}

/// A Location owned by `NL/CPO`, carrying one EVSE + connector. `ts` drives
/// `last_updated` so the pagination/date-filter assertions are deterministic.
fn make_location(id: &str, ts: DateTime<Utc>) -> Location {
    Location {
        country_code: CiString2::try_from("NL").unwrap(),
        party_id: CiString3::try_from("CPO").unwrap(),
        id: CiString36::try_from(id).unwrap(),
        publish: true,
        publish_allowed_to: Vec::new(),
        name: None,
        address: "F.Rooseveltlaan 3A".into(),
        city: "Gent".into(),
        postal_code: Some("9000".into()),
        state: None,
        country: "BEL".into(),
        coordinates: GeoLocation {
            latitude: "51.047599".into(),
            longitude: "3.729944".into(),
        },
        related_locations: Vec::new(),
        parking_type: None,
        evses: vec![make_evse("EVSE1", ts)],
        directions: Vec::new(),
        operator: None,
        suboperator: None,
        owner: None,
        facilities: Vec::new(),
        time_zone: "Europe/Amsterdam".into(),
        opening_times: None,
        charging_when_closed: None,
        images: Vec::new(),
        energy_mix: None,
        last_updated: ts,
    }
}

/// Bind an ephemeral loopback socket and return it with its
/// `http://127.0.0.1:PORT` origin.
async fn bind() -> (tokio::net::TcpListener, String) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    (listener, base)
}

/// Serve `router` on `listener` on a background task. The socket is already
/// listening, so callers need no readiness delay.
fn serve(listener: tokio::net::TcpListener, router: axum::Router) {
    tokio::spawn(async move {
        axum::serve(listener, router.into_make_service())
            .await
            .unwrap();
    });
}

#[tokio::test]
async fn m3_locations_round_trip() {
    // ── CPO server: locations_router pre-seeded with three Locations. ───────
    let store = Arc::new(LocationsConfig::new());
    let base_ts = Utc
        .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
        .single()
        .expect("valid timestamp");
    for (i, id) in ["LOC1", "LOC2", "LOC3"].iter().enumerate() {
        let ts = base_ts + Duration::seconds(i as i64);
        store.put(make_location(id, ts));
    }
    let (listener, base) = bind().await;
    serve(listener, locations_router(store));

    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN);

    // The object getters append `{id}[/{evse}/{connector}]` to this base, which
    // lands on the router's receiver routes `/locations/{cc}/{party}/{id}/…`.
    let object_url = format!("{base}/locations/NL/CPO");
    // The list getter GETs this URL directly → the sender route `GET /locations`.
    let list_url = format!("{base}/locations");

    // ── GET single Location → full serde round-trip. ────────────────────────
    let loc = client
        .get_location(&object_url, "LOC1")
        .await
        .expect("GET single Location should return 200");
    assert_eq!(loc.id.as_str(), "LOC1");
    assert_eq!(loc.country_code.as_str(), "NL");
    assert_eq!(loc.party_id.as_str(), "CPO");
    assert_eq!(
        loc.evses.len(),
        1,
        "the nested EVSE survives the round-trip"
    );
    assert_eq!(loc.evses[0].uid.as_str(), "EVSE1");
    assert_eq!(loc.evses[0].connectors[0].id.as_str(), "1");

    // ── GET nested EVSE and Connector via the sub-object routes. ────────────
    let evse = client
        .get_evse(&object_url, "LOC1", "EVSE1")
        .await
        .expect("GET nested EVSE should return 200");
    assert_eq!(evse.uid.as_str(), "EVSE1");
    assert_eq!(evse.status, Status::Available);

    let connector = client
        .get_connector(&object_url, "LOC1", "EVSE1", "1")
        .await
        .expect("GET nested Connector should return 200");
    assert_eq!(connector.id.as_str(), "1");
    assert_eq!(connector.standard, ConnectorType::Iec62196T2);
    assert_eq!(connector.power_type, PowerType::Ac3Phase);

    // ── GET paginated list — first page (limit 2 of 3). ─────────────────────
    let (page1, meta) = client
        .get_locations(
            &list_url,
            PaginatedParams {
                date_from: None,
                date_to: None,
                offset: None,
                limit: Some(2),
            },
        )
        .await
        .expect("GET locations list should return 200");
    assert_eq!(meta.total_count, 3, "X-Total-Count reflects all three");
    assert_eq!(meta.limit, 2, "X-Limit echoes the requested page size");
    assert_eq!(page1.len(), 2, "first page is capped at the limit");
    assert_eq!(page1[0].id.as_str(), "LOC1");
    assert_eq!(page1[1].id.as_str(), "LOC2");
    let next_url = meta
        .next_url
        .expect("a Link: rel=next header must advertise the second page");
    // The server emits a relative Link target (RFC 8288 §3 — resolved against the
    // request URI); the client returns it verbatim, so the consumer resolves it
    // against the endpoint origin, exactly as a real eMSP would.
    assert!(
        next_url.starts_with('/'),
        "next link is a relative reference, got {next_url:?}"
    );
    let next_url = format!("{base}{next_url}");

    // ── Follow the next-page link to drain the remaining Location. ──────────
    let (page2, meta2) = client
        .get_locations(&next_url, PaginatedParams::default())
        .await
        .expect("GET next page should return 200");
    assert_eq!(page2.len(), 1, "second page holds the final Location");
    assert_eq!(page2[0].id.as_str(), "LOC3");
    assert_eq!(meta2.total_count, 3);
    assert!(
        meta2.next_url.is_none(),
        "the last page must not advertise a further next link"
    );

    // ── Unknown Location → OCPI 2003 / HTTP 404 → ClientError::NotFound. ─────
    let missing = client.get_location(&object_url, "NOPE").await;
    assert!(
        matches!(missing, Err(ClientError::NotFound)),
        "an unknown Location must surface as ClientError::NotFound, got {missing:?}"
    );
}

//! M4 Sessions + CDRs end-to-end smoke test (issue #71).
//!
//! Walks a full CPO ↔ eMSP Sessions + CDRs exchange over a real loopback
//! transport: an in-process `axum` server hosting [`sessions_router`] /
//! [`cdrs_router`] is stood up on an ephemeral `127.0.0.1` port and driven
//! entirely through [`OcpiClient`] (no raw `reqwest`). This mirrors the M2
//! registration smoke test (#23) and the M3 Locations smoke test (#32).
//!
//! ## URL shapes
//!
//! [`OcpiClient`] builds its session paths by appending segments to the base it
//! is handed, so the test feeds the session getters `{base}/sessions`:
//!
//! - list:    `GET  {base}/sessions?…`                         (sender)
//! - object:  `GET  {base}/sessions/{cc}/{party}/{id}`         (receiver)
//! - upsert:  `PUT  {base}/sessions/{cc}/{party}/{id}`         (receiver)
//! - patch:   `PATCH {base}/sessions/{cc}/{party}/{id}`        (receiver)
//! - prefs:   `PUT  {base}/sessions/{id}/charging_preferences` (sender)
//!
//! and the CDR getters `{base}/cdrs`:
//!
//! - list:    `GET  {base}/cdrs?…`        (sender)
//! - object:  `GET  {base}/cdrs/{id}`     (sender)
//! - create:  `POST {base}/cdrs`          (receiver) → `201` + `Location`
//!
//! Routing headers (`OCPI-from/to-party-id/country-code`) are accepted but not
//! enforced by the routers; uniform client-side propagation of those headers is
//! tracked separately by #64 and is out of scope here.
//!
//! Spec: `specs/ocpi/2.2.1/mod_sessions.asciidoc` and
//! `specs/ocpi/2.2.1/mod_cdrs.asciidoc` — §Sender + §Receiver Interface.

use std::sync::Arc;

use ocpi_client::{ClientError, OcpiClient};
use ocpi_server::{
    http::{cdrs_router, sessions_router},
    CdrsConfig, SessionsConfig,
};
use ocpi_types::{
    chrono::{Duration, TimeZone as _},
    serde_json::json,
    transport::PaginatedParams,
    AuthMethod, Cdr, CdrDimension, CdrDimensionType, CdrLocation, CdrToken, ChargingPeriod,
    ChargingPreferences, ChargingPreferencesResponse, CiString2, CiString3, CiString36, CiString39,
    CiString48, ConnectorFormat, ConnectorType, DateTime, GeoLocation, PowerType, Price,
    ProfileType, Session, SessionStatus, TokenType, Utc,
};

/// The bearer the eMSP presents on its Sender-interface calls.
const TOKEN: &str = "TOKEN_C_emsp_calls_cpo";

/// The CDR store's base URL — prepended to build the `Location` header on POST.
const CDR_BASE: &str = "https://cpo.example.com/ocpi/2.2.1";

fn make_cdr_token() -> CdrToken {
    CdrToken {
        country_code: CiString2::try_from("NL").unwrap(),
        party_id: CiString3::try_from("TNM").unwrap(),
        uid: CiString36::try_from("012345678").unwrap(),
        token_type: TokenType::Rfid,
        contract_id: CiString36::try_from("NL8ACC12E46L89").unwrap(),
    }
}

/// A Session owned by `NL/CPO`, mirroring the spec example. `ts` drives both
/// `start_date_time` and `last_updated` so the pagination/date-filter
/// assertions are deterministic.
fn make_session(id: &str, ts: DateTime<Utc>) -> Session {
    Session {
        country_code: CiString2::try_from("NL").unwrap(),
        party_id: CiString3::try_from("CPO").unwrap(),
        id: CiString36::try_from(id).unwrap(),
        start_date_time: ts,
        end_date_time: None,
        kwh: 0.0,
        cdr_token: make_cdr_token(),
        auth_method: AuthMethod::Whitelist,
        authorization_reference: None,
        location_id: CiString36::try_from("LOC1").unwrap(),
        evse_uid: CiString36::try_from("3256").unwrap(),
        connector_id: CiString36::try_from("1").unwrap(),
        meter_id: None,
        currency: "EUR".into(),
        charging_periods: vec![],
        total_cost: None,
        status: SessionStatus::Active,
        last_updated: ts,
    }
}

fn make_cdr_location() -> CdrLocation {
    CdrLocation {
        id: CiString36::try_from("LOC1").unwrap(),
        name: Some("Gent Zuid".into()),
        address: "F.Rooseveltlaan 3A".into(),
        city: "Gent".into(),
        postal_code: Some("9000".into()),
        state: None,
        country: "BEL".into(),
        coordinates: GeoLocation {
            latitude: "51.047599".into(),
            longitude: "3.729944".into(),
        },
        evse_uid: CiString36::try_from("3256").unwrap(),
        evse_id: CiString48::try_from("BE*BEC*E041503001").unwrap(),
        connector_id: CiString36::try_from("1").unwrap(),
        connector_standard: ConnectorType::Iec62196T2,
        connector_format: ConnectorFormat::Socket,
        connector_power_type: PowerType::Ac3Phase,
    }
}

/// A CDR owned by `NL/TNM`, mirroring the spec example. `ts` drives
/// `last_updated` for deterministic ordering/pagination.
fn make_cdr(id: &str, ts: DateTime<Utc>) -> Cdr {
    Cdr {
        country_code: CiString2::try_from("NL").unwrap(),
        party_id: CiString3::try_from("TNM").unwrap(),
        id: CiString39::try_from(id).unwrap(),
        start_date_time: ts,
        end_date_time: ts + Duration::hours(2),
        session_id: None,
        cdr_token: make_cdr_token(),
        auth_method: AuthMethod::Whitelist,
        authorization_reference: None,
        cdr_location: make_cdr_location(),
        meter_id: None,
        currency: "EUR".into(),
        tariffs: vec![],
        charging_periods: vec![ChargingPeriod {
            start_date_time: ts,
            dimensions: vec![CdrDimension {
                dimension_type: CdrDimensionType::Energy,
                volume: 120.0,
            }],
            tariff_id: None,
        }],
        signed_data: None,
        total_cost: Price {
            excl_vat: 4.00,
            incl_vat: None,
        },
        total_fixed_cost: None,
        total_energy: 120.0,
        total_energy_cost: None,
        total_time: 1.973,
        total_time_cost: None,
        total_parking_time: None,
        total_parking_cost: None,
        total_reservation_cost: None,
        remark: None,
        invoice_reference_id: None,
        credit: None,
        credit_reference_id: None,
        home_charging_compensation: None,
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
async fn m4_sessions_round_trip() {
    // ── CPO server: sessions_router pre-seeded with three Sessions. ─────────
    let store = Arc::new(SessionsConfig::new());
    let base_ts = Utc
        .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
        .single()
        .expect("valid timestamp");
    for (i, id) in ["SESSION1", "SESSION2", "SESSION3"].iter().enumerate() {
        let ts = base_ts + Duration::seconds(i as i64);
        store.put("NL", "CPO", id, make_session(id, ts));
    }
    let (listener, base) = bind().await;
    serve(listener, sessions_router(store));

    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN);
    let sessions_url = format!("{base}/sessions");

    // ── GET single Session → full serde round-trip. ─────────────────────────
    let session = client
        .get_session(&sessions_url, "NL", "CPO", "SESSION1")
        .await
        .expect("GET single Session should return 200");
    assert_eq!(session.id.as_str(), "SESSION1");
    assert_eq!(session.country_code.as_str(), "NL");
    assert_eq!(session.party_id.as_str(), "CPO");
    assert_eq!(session.status, SessionStatus::Active);
    assert_eq!(session.cdr_token.uid.as_str(), "012345678");

    // ── GET paginated list — first page (limit 2 of 3). ─────────────────────
    let (page1, meta) = client
        .get_sessions(
            &sessions_url,
            &PaginatedParams {
                date_from: None,
                date_to: None,
                offset: None,
                limit: Some(2),
            },
        )
        .await
        .expect("GET sessions list should return 200");
    assert_eq!(meta.total_count, 3, "X-Total-Count reflects all three");
    assert_eq!(meta.limit, 2, "X-Limit echoes the requested page size");
    assert_eq!(page1.len(), 2, "first page is capped at the limit");
    assert_eq!(page1[0].id.as_str(), "SESSION1");
    assert_eq!(page1[1].id.as_str(), "SESSION2");
    let next_url = meta
        .next_url
        .expect("a Link: rel=next header must advertise the second page");
    // The server emits a relative Link target (RFC 8288 §3); the client returns
    // it verbatim, so the consumer resolves it against the endpoint origin.
    assert!(
        next_url.starts_with('/'),
        "next link is a relative reference, got {next_url:?}"
    );
    let next_url = format!("{base}{next_url}");

    // ── Follow the next-page link to drain the remaining Session. ───────────
    let (page2, meta2) = client
        .get_sessions(&next_url, &PaginatedParams::default())
        .await
        .expect("GET next page should return 200");
    assert_eq!(page2.len(), 1, "second page holds the final Session");
    assert_eq!(page2[0].id.as_str(), "SESSION3");
    assert_eq!(meta2.total_count, 3);
    assert!(
        meta2.next_url.is_none(),
        "the last page must not advertise a further next link"
    );

    // ── PUT a brand-new Session → store reflects the insert. ────────────────
    let new_ts = base_ts + Duration::seconds(10);
    let mut new_session = make_session("SESSION9", new_ts);
    new_session.kwh = 7.5;
    client
        .put_session(&sessions_url, "NL", "CPO", "SESSION9", &new_session)
        .await
        .expect("PUT new Session should return 200");
    let fetched = client
        .get_session(&sessions_url, "NL", "CPO", "SESSION9")
        .await
        .expect("the PUT Session is now retrievable");
    assert_eq!(fetched.kwh, 7.5, "the PUT body survived the round-trip");

    // ── PATCH an existing Session (merge-patch kwh + status) → store updates. ─
    client
        .patch_session(
            &sessions_url,
            "NL",
            "CPO",
            "SESSION1",
            &json!({ "kwh": 42.0, "status": "COMPLETED" }),
        )
        .await
        .expect("PATCH Session should return 200");
    let patched = client
        .get_session(&sessions_url, "NL", "CPO", "SESSION1")
        .await
        .expect("the patched Session is retrievable");
    assert_eq!(patched.kwh, 42.0, "PATCH updated kwh");
    assert_eq!(
        patched.status,
        SessionStatus::Completed,
        "PATCH updated status"
    );
    assert_eq!(
        patched.cdr_token.uid.as_str(),
        "012345678",
        "merge-patch left untouched fields intact"
    );

    // ── PUT charging_preferences (Sender interface) → ChargingPreferencesResponse. ─
    // REGULAR needs no planning input.
    let resp = client
        .set_charging_preferences(
            &sessions_url,
            "SESSION1",
            &ChargingPreferences {
                profile_type: ProfileType::Regular,
                departure_time: None,
                energy_need: None,
                discharge_allowed: None,
            },
        )
        .await
        .expect("PUT charging_preferences should return 200");
    assert_eq!(resp, ChargingPreferencesResponse::Accepted);

    // A smart-charging profile with no departure_time → DepartureRequired.
    let resp = client
        .set_charging_preferences(
            &sessions_url,
            "SESSION1",
            &ChargingPreferences {
                profile_type: ProfileType::Cheap,
                departure_time: None,
                energy_need: None,
                discharge_allowed: None,
            },
        )
        .await
        .expect("PUT charging_preferences should return 200");
    assert_eq!(resp, ChargingPreferencesResponse::DepartureRequired);

    // CHEAP with departure + energy_need → Accepted.
    let resp = client
        .set_charging_preferences(
            &sessions_url,
            "SESSION1",
            &ChargingPreferences {
                profile_type: ProfileType::Cheap,
                departure_time: Some(base_ts + Duration::hours(6)),
                energy_need: Some(30.0),
                discharge_allowed: Some(false),
            },
        )
        .await
        .expect("PUT charging_preferences should return 200");
    assert_eq!(resp, ChargingPreferencesResponse::Accepted);

    // ── Unknown Session → OCPI 2003 / HTTP 404 → ClientError::NotFound. ──────
    let missing = client.get_session(&sessions_url, "NL", "CPO", "NOPE").await;
    assert!(
        matches!(missing, Err(ClientError::NotFound)),
        "an unknown Session must surface as ClientError::NotFound, got {missing:?}"
    );
}

#[tokio::test]
async fn m4_cdrs_round_trip() {
    // ── CPO server: cdrs_router pre-seeded with three CDRs. ─────────────────
    let store = Arc::new(CdrsConfig::new(CDR_BASE));
    let base_ts = Utc
        .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
        .single()
        .expect("valid timestamp");
    for (i, id) in ["CDR1", "CDR2", "CDR3"].iter().enumerate() {
        let ts = base_ts + Duration::seconds(i as i64);
        store.store(make_cdr(id, ts));
    }
    let (listener, base) = bind().await;
    serve(listener, cdrs_router(store));

    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN);
    let cdrs_url = format!("{base}/cdrs");

    // ── GET single CDR → full serde round-trip. ─────────────────────────────
    let cdr = client
        .get_cdr(&cdrs_url, "CDR1")
        .await
        .expect("GET single CDR should return 200");
    assert_eq!(cdr.id.as_str(), "CDR1");
    assert_eq!(cdr.total_energy, 120.0);
    assert_eq!(cdr.total_cost.excl_vat, 4.00);
    assert_eq!(cdr.cdr_location.evse_id.as_str(), "BE*BEC*E041503001");
    assert_eq!(cdr.charging_periods.len(), 1, "nested period round-trips");

    // ── GET paginated list — first page (limit 2 of 3). ─────────────────────
    let (page1, meta) = client
        .get_cdrs(
            &cdrs_url,
            PaginatedParams {
                date_from: None,
                date_to: None,
                offset: None,
                limit: Some(2),
            },
        )
        .await
        .expect("GET cdrs list should return 200");
    assert_eq!(meta.total_count, 3, "X-Total-Count reflects all three");
    assert_eq!(meta.limit, 2, "X-Limit echoes the requested page size");
    assert_eq!(page1.len(), 2, "first page is capped at the limit");
    assert_eq!(page1[0].id.as_str(), "CDR1");
    assert_eq!(page1[1].id.as_str(), "CDR2");
    // `get_cdrs` parses the `Link` header down to the bare next-page target via
    // `parse_next_link`, so `next_url` is already a relative reference.
    let next_url = meta
        .next_url
        .expect("a Link: rel=next header must advertise the second page");
    assert!(
        next_url.starts_with('/'),
        "next link is a relative reference, got {next_url:?}"
    );
    let next_url = format!("{base}{next_url}");

    // ── Follow the next-page link to drain the remaining CDR. ───────────────
    let (page2, meta2) = client
        .get_cdrs(&next_url, PaginatedParams::default())
        .await
        .expect("GET next page should return 200");
    assert_eq!(page2.len(), 1, "second page holds the final CDR");
    assert_eq!(page2[0].id.as_str(), "CDR3");
    assert_eq!(meta2.total_count, 3);

    // ── POST a new CDR (Receiver interface) → 201 + Location header URL. ─────
    let new_ts = base_ts + Duration::seconds(10);
    let location = client
        .post_cdr(&cdrs_url, &make_cdr("CDR9", new_ts))
        .await
        .expect("POST CDR should return 201 with a Location header");
    assert_eq!(
        location,
        format!("{CDR_BASE}/cdrs/CDR9"),
        "the Location header points at the newly-created CDR"
    );
    // The created CDR is now retrievable from the CPO store.
    let created = client
        .get_cdr(&cdrs_url, "CDR9")
        .await
        .expect("the POSTed CDR is now retrievable");
    assert_eq!(created.id.as_str(), "CDR9");

    // ── Unknown CDR → OCPI 2003 / HTTP 404 → ClientError::NotFound. ─────────
    let missing = client.get_cdr(&cdrs_url, "NOPE").await;
    assert!(
        matches!(missing, Err(ClientError::NotFound)),
        "an unknown CDR must surface as ClientError::NotFound, got {missing:?}"
    );
}

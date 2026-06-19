//! M6 HubClientInfo end-to-end round trip (issue #80).
//!
//! Exercises the new [`OcpiClient`] HubClientInfo sender methods against the
//! real [`hub_client_info_router`] over a loopback transport — the same
//! in-process-`axum` harness used by the M2/M3 smoke tests. One server stands
//! in for the Hub (and, for the PUT, a connected party's receiver), and the
//! client drives every method through actual HTTP + serde.
//!
//! ## Endpoints (2.2.1 §mod_hub_client_info)
//!
//! - `GET  /clientinfo`                          → Hub Sender list (paginated)
//! - `GET  /clientinfo/{country_code}/{party_id}` → Hub Sender single entry
//! - `PUT  /clientinfo/{country_code}/{party_id}` → party Receiver upsert
//!
//! HubClientInfo is a *Configuration Module*: the OCPI routing headers
//! (`OCPI-to/from-party-id/country-code`) are intentionally **not** sent — only
//! the `Authorization` token — so there is nothing routing-header-related to
//! assert here (cf. #64 for the modules that do carry them).
//!
//! Spec: `specs/ocpi/2.2.1/mod_hub_client_info.asciidoc`.

use std::sync::Arc;

use ocpi_client::{ClientError, OcpiClient};
use ocpi_server::{http::hub_client_info_router, HubClientInfoConfig};
use ocpi_types::{
    chrono::{Duration, TimeZone as _},
    transport::PaginatedParams,
    CiString2, CiString3, ClientInfo, ConnectionStatus, DateTime, Role, Utc,
};

/// The bearer a connected party presents on its calls to the Hub.
const TOKEN: &str = "TOKEN_C_party_calls_hub";

/// Build a `ClientInfo` mirroring the spec's example shape (a CPO party whose
/// connection status the Hub tracks). `ts` drives `last_updated` so the
/// pagination ordering is deterministic.
fn make_client_info(party_id: &str, status: ConnectionStatus, ts: DateTime<Utc>) -> ClientInfo {
    ClientInfo {
        party_id: CiString3::try_from(party_id).unwrap(),
        country_code: CiString2::try_from("NL").unwrap(),
        role: Role::Cpo,
        status,
        last_updated: ts,
    }
}

/// Bind an ephemeral loopback socket and return it with its origin.
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
async fn m6_hub_client_info_round_trip() {
    // ── Hub server: hub_client_info_router seeded with three parties. ───────
    let store = Arc::new(HubClientInfoConfig::new());
    let base_ts = Utc
        .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
        .single()
        .expect("valid timestamp");
    for (i, party) in ["AAA", "BBB", "CCC"].iter().enumerate() {
        let ts = base_ts + Duration::seconds(i as i64);
        store.put(
            "NL",
            party,
            make_client_info(party, ConnectionStatus::Connected, ts),
        );
    }
    let (listener, base) = bind().await;
    serve(listener, hub_client_info_router(Arc::clone(&store)));

    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN);

    // The single/PUT getters append `{country_code}/{party_id}` to this base.
    let object_url = format!("{base}/clientinfo");
    // The list getter GETs this URL directly → `GET /clientinfo`.
    let list_url = format!("{base}/clientinfo");

    // ── GET single ClientInfo → full serde round-trip. ──────────────────────
    let info = client
        .get_client_info(&object_url, "NL", "AAA")
        .await
        .expect("GET single ClientInfo should return 200");
    assert_eq!(info.party_id.as_str(), "AAA");
    assert_eq!(info.country_code.as_str(), "NL");
    assert_eq!(info.role, Role::Cpo);
    assert_eq!(info.status, ConnectionStatus::Connected);

    // ── GET paginated list — first page (limit 2 of 3). ─────────────────────
    let (page1, meta) = client
        .get_client_infos(
            &list_url,
            &PaginatedParams {
                date_from: None,
                date_to: None,
                offset: None,
                limit: Some(2),
            },
        )
        .await
        .expect("GET clientinfo list should return 200");
    assert_eq!(meta.total_count, 3, "X-Total-Count reflects all three");
    assert_eq!(meta.limit, 2, "X-Limit echoes the requested page size");
    assert_eq!(page1.len(), 2, "first page is capped at the limit");
    // Server sorts by last_updated, so AAA (earliest) precedes BBB.
    assert_eq!(page1[0].party_id.as_str(), "AAA");
    assert_eq!(page1[1].party_id.as_str(), "BBB");
    let next_url = meta
        .next_url
        .expect("a Link: rel=next header must advertise the second page");
    assert!(
        next_url.starts_with('/'),
        "next link is a relative reference, got {next_url:?}"
    );
    let next_url = format!("{base}{next_url}");

    // ── Follow the next-page link to drain the remaining party. ─────────────
    let (page2, meta2) = client
        .get_client_infos(&next_url, &PaginatedParams::default())
        .await
        .expect("GET next page should return 200");
    assert_eq!(page2.len(), 1, "second page holds the final party");
    assert_eq!(page2[0].party_id.as_str(), "CCC");
    assert_eq!(meta2.total_count, 3);
    assert!(
        meta2.next_url.is_none(),
        "the last page must not advertise a further next link"
    );

    // ── PUT a status change to a party's receiver, then read it back. ───────
    // The Hub pushes an updated ClientInfo (CCC went Offline) to the party.
    let updated_ts = base_ts + Duration::hours(1);
    let pushed = make_client_info("CCC", ConnectionStatus::Offline, updated_ts);
    client
        .put_client_info(&object_url, "NL", "CCC", &pushed)
        .await
        .expect("PUT ClientInfo should return 200");
    let readback = client
        .get_client_info(&object_url, "NL", "CCC")
        .await
        .expect("the upserted ClientInfo should be retrievable");
    assert_eq!(
        readback.status,
        ConnectionStatus::Offline,
        "the PUT must persist the changed status"
    );
    assert_eq!(readback.last_updated, updated_ts);

    // ── Unknown party → OCPI 2003 / HTTP 404 → ClientError::NotFound. ───────
    let missing = client.get_client_info(&object_url, "NL", "ZZZ").await;
    assert!(
        matches!(missing, Err(ClientError::NotFound)),
        "an unknown party must surface as ClientError::NotFound, got {missing:?}"
    );
}

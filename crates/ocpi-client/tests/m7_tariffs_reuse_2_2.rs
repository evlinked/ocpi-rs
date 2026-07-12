//! M7 OCPI **2.2** Tariffs — *wire-identical module reuse*, verified at the
//! transport layer (issue #171, the 2.2 back-coverage closeout).
//!
//! Tariffs is one of the **seven** OCPI modules with **no** 2.2-vs-2.2.1 wire
//! delta (`specs/ocpi/2.2.1/version_history.asciidoc`): the entire 2.2.1-over-2.2
//! delta surface is CDRs, Commands, and Locations, all of which are sliced into
//! `v2_2`-local overrides. Everything else — Sessions, **Tariffs**, Tokens,
//! ChargingProfiles, HubClientInfo, Versions, Credentials — is a genuine
//! re-export of its 2.2.1 type, so `ocpi_types::v2_2::Tariff` *is*
//! `ocpi_types::v2_2_1::Tariff` (a compile-time identity, ratified by the
//! `reuse` assertions in `crates/ocpi-types/src/v2_2/mod.rs`, #173).
//!
//! The design decision #171 ratifies is therefore that **no `_2_2` client/server
//! methods are minted for the wire-identical modules** — "aliasing identical
//! calls would only imply a difference that does not exist" (the precedent set
//! by the Commands wiring, #166). A 2.2 party drives Tariffs with the *existing*
//! unqualified 2.2.1 surface (`OcpiClient::{get_tariffs,get_tariff,put_tariff,
//! delete_tariff}` + [`tariffs_router`]).
//!
//! #173 proves that reuse at the **type** layer (a `v2_2::Session` serde
//! round-trip equals the 2.2.1 one). This test proves the same claim one layer
//! down — at the **transport** layer — for Tariffs: a `v2_2::Tariff` built from
//! the spec's "Simple Tariff" example rides the existing 2.2.1 client + server
//! end-to-end (`PUT` → `GET` → paginated list → `DELETE`) and round-trips
//! byte-for-byte. That is what makes the README matrix 2.2/Tariffs cell honest
//! as ☑: reuse is *exercised*, not merely asserted.
//!
//! Spec: OCPI 2.2 — *Tariffs* module, `specs/ocpi/2.2/mod_tariffs.asciidoc`
//! (Sender §Tariff object, Receiver `PUT`/`DELETE` at
//! `{country_code}/{party_id}/{tariff_id}`).

use std::sync::Arc;

use ocpi_client::{ClientError, OcpiClient};
use ocpi_server::{http::tariffs_router, TariffsConfig};
// The 2.2 alias — `v2_2::Tariff` is the very same type as `v2_2_1::Tariff`, so
// the values it produces feed the unqualified 2.2.1 client/server surface
// directly. Importing it *through `v2_2`* is the point: this is a 2.2 party.
use ocpi_types::v2_2::Tariff;
use ocpi_types::{
    chrono::{Duration, TimeZone as _},
    serde_json::{self, json},
    transport::PaginatedParams,
    DateTime, Utc,
};

/// The bearer the peer presents on the Tariffs exchange.
const TOKEN: &str = "TOKEN_C_tariffs_reuse";

/// The spec's "Simple Tariff" example (2.2 §Tariffs — a single energy price
/// component at 0.25 EUR/kWh). The `country_code`/`party_id` owner fields, the
/// `elements`/`price_components` shape, and `last_updated` are wire-identical
/// between 2.2 and 2.2.1 — which is exactly why Tariffs needs no 2.2 override.
fn tariff_json() -> serde_json::Value {
    json!({
        "country_code": "DE",
        "party_id": "ALL",
        "id": "16",
        "currency": "EUR",
        "elements": [{
            "price_components": [{
                "type": "ENERGY",
                "price": 0.25,
                "vat": 21.0,
                "step_size": 1
            }]
        }],
        "last_updated": "2018-12-17T11:16:55Z"
    })
}

/// Build a 2.2 `Tariff` from the spec fixture, overriding the id and the
/// `last_updated` timestamp that drives deterministic date filtering.
fn make_tariff(id: &str, ts: DateTime<Utc>) -> Tariff {
    let mut tariff: Tariff = serde_json::from_value(tariff_json()).expect("valid 2.2 tariff");
    tariff.id = id.try_into().unwrap();
    tariff.last_updated = ts;
    tariff
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

/// A 2.2 party drives the wire-identical Tariffs module end-to-end over the
/// *existing* 2.2.1 client + server — no `_2_2` method is minted or needed. The
/// `v2_2::Tariff` round-trips byte-for-byte, which is the transport-layer proof
/// behind the README 2.2/Tariffs ☑.
#[tokio::test]
async fn m7_tariffs_2_2_reuse_put_get_list_delete_round_trip() {
    // ── Receiver server: the unqualified 2.2.1 `tariffs_router`. ────────────
    let cfg = Arc::new(TariffsConfig::new());
    let base_ts = Utc
        .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
        .single()
        .expect("valid timestamp");
    // Pre-seed two tariffs so the list endpoint exercises real pagination.
    for (i, id) in ["16", "17"].iter().enumerate() {
        let ts = base_ts + Duration::seconds(i as i64);
        cfg.put("DE", "ALL", id, make_tariff(id, ts));
    }
    let (listener, base) = bind().await;
    serve(listener, tariffs_router(cfg));

    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN);
    let tariffs_url = format!("{base}/tariffs");

    // ── GET one tariff → full 2.2 serde round-trip through the 2.2.1 method. ─
    let tariff = client
        .get_tariff(&tariffs_url, "DE", "ALL", "16")
        .await
        .expect("GET single tariff should return 200");
    assert_eq!(tariff.id.as_str(), "16");
    assert_eq!(tariff.currency, "EUR");
    assert_eq!(tariff.elements.len(), 1);
    assert_eq!(tariff.elements[0].price_components[0].price, 0.25);

    // ── PUT a new 2.2 tariff via the unqualified receiver method. ───────────
    let new = make_tariff("18", base_ts + Duration::seconds(5));
    client
        .put_tariff(&tariffs_url, "DE", "ALL", "18", &new)
        .await
        .expect("PUT tariff should succeed");

    // The just-pushed 2.2 tariff is retrievable and byte-for-byte identical —
    // the object is stored and served unmangled (the reuse is real, not coerced
    // through a distinct 2.2-only type, because there is none).
    let fetched = client
        .get_tariff(&tariffs_url, "DE", "ALL", "18")
        .await
        .expect("GET of the just-PUT tariff should return 200");
    assert_eq!(fetched, new);

    // ── GET paginated list — first page (limit 2 of 3). ─────────────────────
    let (page1, meta) = client
        .get_tariffs(
            &tariffs_url,
            PaginatedParams {
                limit: Some(2),
                ..Default::default()
            },
        )
        .await
        .expect("GET tariffs list should return 200");
    assert_eq!(page1.len(), 2);
    assert_eq!(meta.total_count, 3);
    assert_eq!(meta.limit, 2);
    let next = meta.next_url.expect("a second page should be advertised");
    assert!(next.contains("offset=2"), "next link: {next}");

    // ── DELETE the pushed tariff, then confirm it is gone (404). ────────────
    client
        .delete_tariff(&tariffs_url, "DE", "ALL", "18")
        .await
        .expect("DELETE tariff should succeed");
    let missing = client.get_tariff(&tariffs_url, "DE", "ALL", "18").await;
    assert!(matches!(missing, Err(ClientError::NotFound)));
}

/// A 2.2 party's serialized `Tariff` is byte-for-byte a 2.2.1 `Tariff` — the
/// concrete evidence the module carries no wire delta and so is reused rather
/// than forked. (The compile-time identity is asserted in
/// `crates/ocpi-types/src/v2_2/mod.rs`; this checks the *wire* agrees.)
#[test]
fn tariff_2_2_wire_is_identical_to_2_2_1() {
    let ts = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).single().unwrap();
    let via_2_2: ocpi_types::v2_2::Tariff = make_tariff("16", ts);
    let via_2_2_1: ocpi_types::v2_2_1::Tariff = serde_json::from_value(tariff_json())
        .map(|mut t: ocpi_types::v2_2_1::Tariff| {
            t.id = "16".try_into().unwrap();
            t.last_updated = ts;
            t
        })
        .unwrap();

    // Same type, same value.
    assert_eq!(via_2_2, via_2_2_1);
    // Same bytes on the wire — no field added, dropped, or renamed for 2.2.
    assert_eq!(
        serde_json::to_value(&via_2_2).unwrap(),
        serde_json::to_value(&via_2_2_1).unwrap(),
    );
}

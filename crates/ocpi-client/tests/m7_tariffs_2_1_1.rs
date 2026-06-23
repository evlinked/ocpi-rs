//! M7 end-to-end **OCPI 2.1.1 Tariffs** smoke test (issue #122).
//!
//! Walks a full CPO↔eMSP 2.1.1 Tariffs exchange over a real loopback
//! transport: an in-process `axum` server is stood up on an ephemeral
//! `127.0.0.1` port and driven entirely through [`OcpiClient`]'s `_2_1_1`
//! getters/setters (no raw `reqwest`). This is the 2.1.1 counterpart to the
//! 2.2.1 Tariffs leg of `m5_tariffs_tokens.rs`.
//!
//! ## 2.1.1 transport fidelity
//!
//! Per OCPI 2.1.1 §11.2, the Tariffs transport paths are **identical to
//! 2.2.1** — the Sender (CPO) interface is flat (`GET /tariffs`, §11.2.1) and
//! the Receiver (eMSP) interface is a client-owned object keyed by
//! `{country_code}/{party_id}/{tariff_id}` (§11.2.2). Only the `Tariff` object
//! shape differs from 2.2.1: no `country_code`/`party_id`/`type`/`min_price`/
//! `max_price`, validity lives only inside `TariffRestrictions`, and
//! `PriceComponent` has no `vat`.
//!
//! Spec: OCPI 2.1.1 — *Tariffs* module (`specs/ocpi/2.1.1/OCPI_2.1.1.pdf`, §11).

use std::sync::Arc;

use ocpi_client::OcpiClient;
use ocpi_server::{http::tariffs_2_1_1_router, Tariffs2111Config};
use ocpi_types::{
    chrono::TimeZone as _,
    transport::PaginatedParams,
    v2_1_1::{PriceComponent, Tariff, TariffDimensionType, TariffElement},
    CiString36, DateTime, Utc,
};

/// Build a minimal but spec-faithful 2.1.1 `Tariff` — a single energy price
/// component at 0.25 EUR/kWh (the spec's "Simple Tariff" shape, §11.3.1).
fn sample_tariff(id: &str, ts: DateTime<Utc>) -> Tariff {
    Tariff {
        id: CiString36::try_from(id).unwrap(),
        currency: "EUR".to_owned(),
        tariff_alt_text: vec![],
        tariff_alt_url: None,
        elements: vec![TariffElement {
            price_components: vec![PriceComponent {
                component_type: TariffDimensionType::Energy,
                price: 0.25,
                step_size: 1,
            }],
            restrictions: None,
        }],
        energy_mix: None,
        last_updated: ts,
    }
}

fn at(secs: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap() + ocpi_types::chrono::Duration::seconds(secs)
}

/// Bind an ephemeral loopback socket and return it with its origin.
async fn bind() -> (tokio::net::TcpListener, String) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    (listener, base)
}

/// Serve `router` on the already-listening `listener` on a background task.
fn serve(listener: tokio::net::TcpListener, router: axum::Router) {
    tokio::spawn(async move {
        axum::serve(listener, router.into_make_service())
            .await
            .unwrap();
    });
}

#[tokio::test]
async fn m7_tariffs_2_1_1_get_list_put_delete_round_trip() {
    // ── Stand up an eMSP receiver hosting the 2.1.1 Tariffs module. ─────────
    // Pre-seed three tariffs so the list endpoint exercises real pagination.
    let cfg = Arc::new(Tariffs2111Config::new());
    for i in 0..3 {
        let id = format!("TARIFF{i:03}");
        cfg.put("NL", "CPO", &id, sample_tariff(&id, at(i)));
    }
    let (listener, base) = bind().await;
    serve(listener, tariffs_2_1_1_router(cfg.clone()));

    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), "tok");
    let tariffs_url = format!("{base}/tariffs");

    // GET single tariff round-trips by composite key.
    let got = client
        .get_tariff_2_1_1(&tariffs_url, "NL", "CPO", "TARIFF001")
        .await
        .expect("GET 2.1.1 tariff by id should succeed");
    assert_eq!(got.id.as_str(), "TARIFF001");
    assert_eq!(got.currency, "EUR");
    assert_eq!(got.elements[0].price_components[0].price, 0.25);

    // GET list with limit=2 — first page yields 2, total is 3, and the server
    // advertises a next page via the Link header.
    let (page, meta) = client
        .get_tariffs_2_1_1(
            &tariffs_url,
            PaginatedParams {
                limit: Some(2),
                ..Default::default()
            },
        )
        .await
        .expect("GET 2.1.1 tariffs list should succeed");
    assert_eq!(page.len(), 2, "first page honors limit=2");
    assert_eq!(meta.total_count, 3, "X-Total-Count reflects all matches");
    assert_eq!(meta.limit, 2, "X-Limit echoes the requested limit");
    assert!(
        meta.next_url.is_some(),
        "Link: rel=next must be present while more pages remain"
    );

    // PUT a fresh tariff, then confirm it is retrievable through the client.
    let new = sample_tariff("TARIFF999", at(99));
    client
        .put_tariff_2_1_1(&tariffs_url, "NL", "CPO", "TARIFF999", &new)
        .await
        .expect("PUT 2.1.1 tariff should succeed");
    let fetched = client
        .get_tariff_2_1_1(&tariffs_url, "NL", "CPO", "TARIFF999")
        .await
        .expect("PUT tariff should be retrievable");
    assert_eq!(fetched, new, "round-tripped tariff is byte-identical");

    // DELETE it, then confirm both the store and a follow-up GET agree it's gone.
    client
        .delete_tariff_2_1_1(&tariffs_url, "NL", "CPO", "TARIFF999")
        .await
        .expect("DELETE 2.1.1 tariff should succeed");
    assert!(
        cfg.get("NL", "CPO", "TARIFF999").is_none(),
        "store reflects the removed tariff"
    );
    let missing = client
        .get_tariff_2_1_1(&tariffs_url, "NL", "CPO", "TARIFF999")
        .await;
    assert!(missing.is_err(), "GET of a deleted 2.1.1 tariff must 404");

    // DELETE of an unknown tariff is reported as NotFound, not silently ignored.
    let del_missing = client
        .delete_tariff_2_1_1(&tariffs_url, "NL", "CPO", "NOPE")
        .await;
    assert!(
        del_missing.is_err(),
        "DELETE of an unknown 2.1.1 tariff must 404"
    );
}

/// Lock in the 2.1.1 wire shape: the serialized `Tariff` must NOT carry any of
/// the fields 2.2 / 2.2.1 added, and `PriceComponent` must have no `vat`.
#[test]
fn tariff_2_1_1_wire_omits_2_2_fields() {
    let json = ocpi_types::serde_json::to_value(sample_tariff("T1", at(0))).unwrap();
    let obj = json.as_object().expect("Tariff serializes to an object");

    for absent in [
        "country_code",
        "party_id",
        "type",
        "min_price",
        "max_price",
        "start_date_time",
        "end_date_time",
    ] {
        assert!(
            !obj.contains_key(absent),
            "2.1.1 Tariff must not emit `{absent}` (a 2.2+ field)"
        );
    }
    // Required 2.1.1 fields are present.
    for present in ["id", "currency", "elements", "last_updated"] {
        assert!(
            obj.contains_key(present),
            "2.1.1 Tariff must emit `{present}`"
        );
    }

    // PriceComponent has no `vat` in 2.1.1 (added in 2.2.1).
    let pc = &obj["elements"][0]["price_components"][0];
    assert!(
        pc.as_object().unwrap().get("vat").is_none(),
        "2.1.1 PriceComponent must not emit `vat`"
    );
}

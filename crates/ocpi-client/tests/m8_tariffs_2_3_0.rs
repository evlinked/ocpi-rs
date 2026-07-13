//! M8 end-to-end **OCPI 2.3.0 Tariffs** smoke test (issue #178).
//!
//! Walks a full CPO↔eMSP 2.3.0 Tariffs exchange over a real loopback
//! transport: an in-process `axum` server built from
//! [`tariffs_2_3_0_router`](ocpi_server::http::tariffs_2_3_0_router) is stood up
//! on an ephemeral `127.0.0.1` port and driven entirely through [`OcpiClient`]'s
//! `_2_3_0` getters/setters (no raw `reqwest`). This is the 2.3.0 counterpart to
//! the 2.1.1 leg in `m7_tariffs_2_1_1.rs`.
//!
//! ## 2.3.0 transport fidelity
//!
//! Per `specs/ocpi/2.3.0/mod_tariffs.asciidoc`, the Tariffs transport paths are
//! **identical to 2.2.1** — the Sender (CPO) interface is flat (`GET /tariffs`)
//! and the Receiver (eMSP) interface is a client-owned object keyed by
//! `{country_code}/{party_id}/{tariff_id}`. Only the `Tariff` object shape
//! differs: it carries the North-American tax fork — a required `tax_included`
//! flag, tax-aware `PriceLimit` `min_price`/`max_price`, and `preauthorize_amount`.
//! The test asserts those delta fields survive the round-trip on the wire
//! instead of being coerced through the VAT-only 2.2.1 shape.

use std::sync::Arc;

use ocpi_client::OcpiClient;
use ocpi_server::{http::tariffs_2_3_0_router, Tariffs230Config};
use ocpi_types::{
    chrono::TimeZone as _,
    transport::PaginatedParams,
    v2_2_1::{PriceComponent, TariffDimensionType, TariffElement},
    v2_3_0::{PriceLimit, Tariff, TaxIncluded},
    CiString2, CiString3, CiString36, DateTime, Utc,
};

/// Build a spec-faithful North-American 2.3.0 `Tariff`: taxes **not** included
/// (added on top afterward), a tax-aware `min_price`/`max_price`, and a
/// `preauthorize_amount` a Payment Terminal Provider should reserve. The
/// per-component `vat` is deliberately empty — the North-American convention
/// signals tax handling with the top-level `tax_included` flag, not per line.
fn na_tariff(id: &str, ts: DateTime<Utc>) -> Tariff {
    Tariff {
        country_code: CiString2::try_from("CA").unwrap(),
        party_id: CiString3::try_from("EXA").unwrap(),
        id: CiString36::try_from(id).unwrap(),
        currency: "CAD".to_owned(),
        tariff_type: None,
        tariff_alt_text: vec![],
        tariff_alt_url: None,
        min_price: Some(PriceLimit {
            before_taxes: 0.5,
            after_taxes: Some(0.55),
        }),
        max_price: Some(PriceLimit {
            before_taxes: 10.0,
            after_taxes: None,
        }),
        preauthorize_amount: Some(25.0),
        elements: vec![TariffElement {
            price_components: vec![PriceComponent {
                component_type: TariffDimensionType::Energy,
                price: 0.25,
                vat: None,
                step_size: 1,
            }],
            restrictions: None,
        }],
        tax_included: TaxIncluded::No,
        start_date_time: None,
        end_date_time: None,
        energy_mix: None,
        last_updated: ts,
    }
}

fn at(secs: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 6, 1, 0, 0, 0).unwrap() + ocpi_types::chrono::Duration::seconds(secs)
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
async fn m8_tariffs_2_3_0_get_list_put_delete_round_trip() {
    // ── Stand up an eMSP receiver hosting the 2.3.0 Tariffs module. ──────────
    // Pre-seed three tariffs so the list endpoint exercises real pagination.
    let cfg = Arc::new(Tariffs230Config::new());
    for i in 0..3 {
        let id = format!("TARIFF{i:03}");
        cfg.put("CA", "EXA", &id, na_tariff(&id, at(i)));
    }
    let (listener, base) = bind().await;
    serve(listener, tariffs_2_3_0_router(cfg.clone()));

    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), "tok");
    let tariffs_url = format!("{base}/tariffs");

    // GET single tariff round-trips by composite key — the North-American tax
    // delta fields survive the wire hop intact.
    let got = client
        .get_tariff_2_3_0(&tariffs_url, "CA", "EXA", "TARIFF001")
        .await
        .expect("GET 2.3.0 tariff by id should succeed");
    assert_eq!(got.id.as_str(), "TARIFF001");
    assert_eq!(got.currency, "CAD");
    assert_eq!(
        got.tax_included,
        TaxIncluded::No,
        "tax_included stance must survive the round-trip"
    );
    assert_eq!(got.min_price.as_ref().unwrap().before_taxes, 0.5);
    assert_eq!(got.min_price.as_ref().unwrap().after_taxes, Some(0.55));
    assert_eq!(got.max_price.as_ref().unwrap().before_taxes, 10.0);
    assert_eq!(got.max_price.as_ref().unwrap().after_taxes, None);
    assert_eq!(got.preauthorize_amount, Some(25.0));
    // North-American convention leaves the per-component VAT empty.
    assert!(got.elements[0].price_components[0].vat.is_none());

    // GET list with limit=2 — first page yields 2, total is 3, and the server
    // advertises a next page via the Link header.
    let (page, meta) = client
        .get_tariffs_2_3_0(
            &tariffs_url,
            PaginatedParams {
                limit: Some(2),
                ..Default::default()
            },
        )
        .await
        .expect("GET 2.3.0 tariffs list should succeed");
    assert_eq!(page.len(), 2, "first page honors limit=2");
    assert_eq!(meta.total_count, 3, "X-Total-Count reflects all matches");
    assert_eq!(meta.limit, 2, "X-Limit echoes the requested limit");
    assert!(
        meta.next_url.is_some(),
        "Link: rel=next must be present while more pages remain"
    );

    // PUT a fresh tariff, then confirm it is retrievable through the client.
    let new = na_tariff("TARIFF999", at(99));
    client
        .put_tariff_2_3_0(&tariffs_url, "CA", "EXA", "TARIFF999", &new)
        .await
        .expect("PUT 2.3.0 tariff should succeed");
    let fetched = client
        .get_tariff_2_3_0(&tariffs_url, "CA", "EXA", "TARIFF999")
        .await
        .expect("PUT tariff should be retrievable");
    assert_eq!(fetched, new, "round-tripped tariff is byte-identical");

    // DELETE it, then confirm both the store and a follow-up GET agree it's gone.
    client
        .delete_tariff_2_3_0(&tariffs_url, "CA", "EXA", "TARIFF999")
        .await
        .expect("DELETE 2.3.0 tariff should succeed");
    assert!(
        cfg.get("CA", "EXA", "TARIFF999").is_none(),
        "store reflects the removed tariff"
    );
    let missing = client
        .get_tariff_2_3_0(&tariffs_url, "CA", "EXA", "TARIFF999")
        .await;
    assert!(missing.is_err(), "GET of a deleted 2.3.0 tariff must 404");

    // DELETE of an unknown tariff is reported as NotFound, not silently ignored.
    let del_missing = client
        .delete_tariff_2_3_0(&tariffs_url, "CA", "EXA", "NOPE")
        .await;
    assert!(
        del_missing.is_err(),
        "DELETE of an unknown 2.3.0 tariff must 404"
    );
}

/// Lock in the 2.3.0 wire delta: a `Tariff` must emit the North-American tax
/// fields, and a body missing the required `tax_included` must be rejected on
/// deserialize (never silently defaulted) — the server-side trust boundary.
#[test]
fn tariff_2_3_0_wire_carries_tax_delta_and_rejects_missing_tax_included() {
    let json = ocpi_types::serde_json::to_value(na_tariff("T1", at(0))).unwrap();
    let obj = json.as_object().expect("Tariff serializes to an object");

    for present in [
        "country_code",
        "party_id",
        "tax_included",
        "min_price",
        "max_price",
        "preauthorize_amount",
    ] {
        assert!(
            obj.contains_key(present),
            "2.3.0 Tariff must emit `{present}`"
        );
    }
    assert_eq!(obj["tax_included"], "NO");
    assert_eq!(obj["min_price"]["after_taxes"], 0.55);

    // `tax_included` is required (card. 1): dropping it fails to deserialize.
    let mut without = obj.clone();
    without.remove("tax_included");
    let err = ocpi_types::serde_json::from_value::<Tariff>(without.into()).unwrap_err();
    assert!(
        err.to_string().contains("tax_included"),
        "error should name the missing field: {err}"
    );
}

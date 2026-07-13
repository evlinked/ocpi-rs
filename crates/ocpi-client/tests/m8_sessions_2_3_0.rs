//! M8 OCPI **2.3.0** Sessions — client + server round-trip smoke test
//! (issue #203, Sessions slice).
//!
//! Stands up an in-process `axum` server hosting the real
//! [`sessions_2_3_0_router`] on an ephemeral `127.0.0.1` port and drives the
//! full CPO ↔ eMSP Sessions exchange entirely through [`OcpiClient`]'s 2.3.0
//! methods (no raw `reqwest`). Mirrors the 2.1.1 Sessions smoke test
//! (`m7_sessions_2_1_1.rs`); the only wire difference is the payload type.
//!
//! ## The 2.3.0 delta under test
//!
//! Sessions is a client-owned object, so the receiver path is identical to
//! 2.2.1/2.1.1 (`{cc}/{party}/{id}`). Only the payload differs: the 2.3.0
//! [`Session`](ocpi_types::v2_3_0::Session) reworks `total_cost` onto the
//! tax-itemised 2.3.0 [`Price`](ocpi_types::v2_3_0::Price) (`before_taxes` + an
//! itemised [`TaxAmount`](ocpi_types::v2_3_0::TaxAmount) list). The fixture is a
//! **North-American** session whose `total_cost` carries a federal GST + a
//! provincial QST line; the test asserts that itemised breakdown survives every
//! verb intact instead of collapsing into the VAT-only 2.2.1 field.
//!
//! Spec: `specs/ocpi/2.3.0/mod_sessions.asciidoc` — *Sessions* module.

use std::sync::Arc;

use ocpi_client::{ClientError, OcpiClient};
use ocpi_server::{http::sessions_2_3_0_router, Sessions230Config};
use ocpi_types::v2_2_1::{AuthMethod, CdrToken, SessionStatus};
use ocpi_types::v2_3_0::{Price, Session, TaxAmount};
use ocpi_types::{
    chrono::{Duration, TimeZone as _},
    serde_json::{self, json},
    transport::PaginatedParams,
    CiString2, CiString3, CiString36, DateTime, TokenType, Utc,
};

/// The bearer the eMSP presents on its Sender-interface calls.
const TOKEN: &str = "TOKEN_C_emsp_calls_cpo";

fn make_cdr_token() -> CdrToken {
    CdrToken {
        country_code: CiString2::try_from("CA").unwrap(),
        party_id: CiString3::try_from("TNM").unwrap(),
        uid: CiString36::try_from("012345678").unwrap(),
        token_type: TokenType::Rfid,
        contract_id: CiString36::try_from("CA8ACC12E46L89").unwrap(),
    }
}

/// A North-American `total_cost`: a `before_taxes` base plus two itemised tax
/// lines (Canadian federal GST 5 % + Québec QST 9.975 %) — the exact shape the
/// VAT-only 2.2.1 `Price` cannot represent, and the whole reason the 2.3.0
/// `Session` forks `total_cost` onto the reworked `Price`.
fn make_total_cost() -> Price {
    Price {
        before_taxes: 10.0,
        taxes: vec![
            TaxAmount {
                name: "GST".into(),
                account_number: Some("123456789RT0001".into()),
                percentage: Some(5.0),
                amount: 0.50,
            },
            TaxAmount {
                name: "QST".into(),
                account_number: None,
                percentage: Some(9.975),
                amount: 0.9975,
            },
        ],
    }
}

/// A 2.3.0 Session owned by `CA/CPO` carrying the itemised North-American
/// `total_cost`. `ts` drives both `start_date_time` and `last_updated` so the
/// pagination/date-filter assertions are deterministic.
fn make_session(id: &str, ts: DateTime<Utc>) -> Session {
    Session {
        country_code: CiString2::try_from("CA").unwrap(),
        party_id: CiString3::try_from("CPO").unwrap(),
        id: CiString36::try_from(id).unwrap(),
        start_date_time: ts,
        end_date_time: None,
        kwh: 42.0,
        cdr_token: make_cdr_token(),
        auth_method: AuthMethod::Whitelist,
        authorization_reference: None,
        location_id: CiString36::try_from("LOC1").unwrap(),
        evse_uid: CiString36::try_from("3256").unwrap(),
        connector_id: CiString36::try_from("1").unwrap(),
        meter_id: None,
        currency: "CAD".into(),
        charging_periods: vec![],
        total_cost: Some(make_total_cost()),
        status: SessionStatus::Active,
        last_updated: ts,
    }
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

/// Assert a `total_cost` still carries the itemised GST+QST breakdown intact.
fn assert_north_american_tax(price: &Price) {
    assert_eq!(price.before_taxes, 10.0);
    assert_eq!(price.taxes.len(), 2, "both tax lines survive");
    assert_eq!(price.taxes[0].name, "GST");
    assert_eq!(
        price.taxes[0].account_number.as_deref(),
        Some("123456789RT0001")
    );
    assert_eq!(price.taxes[0].percentage, Some(5.0));
    assert_eq!(price.taxes[1].name, "QST");
    assert_eq!(price.taxes[1].percentage, Some(9.975));
}

#[tokio::test]
async fn m8_sessions_2_3_0_get_list_put_patch_round_trip() {
    // ── eMSP server: sessions_2_3_0_router seeded with three Sessions. ─────────
    let store = Arc::new(Sessions230Config::new());
    let base_ts = Utc
        .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
        .single()
        .expect("valid timestamp");
    for (i, id) in ["SESSION1", "SESSION2", "SESSION3"].iter().enumerate() {
        let ts = base_ts + Duration::seconds(i as i64);
        store.put("CA", "CPO", id, make_session(id, ts));
    }
    let (listener, base) = bind().await;
    serve(listener, sessions_2_3_0_router(store));

    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN);
    let sessions_url = format!("{base}/sessions");

    // ── GET single Session → the itemised tax breakdown survives the hop. ──────
    let session = client
        .get_session_2_3_0(&sessions_url, "CA", "CPO", "SESSION1")
        .await
        .expect("GET single Session should return 200");
    assert_eq!(session.id.as_str(), "SESSION1");
    assert_eq!(session.status, SessionStatus::Active);
    assert_eq!(session.currency, "CAD");
    assert_north_american_tax(
        session
            .total_cost
            .as_ref()
            .expect("total_cost must round-trip"),
    );

    // ── GET paginated list — first page (limit 2 of 3), headers honoured. ──────
    let (page1, meta) = client
        .get_sessions_2_3_0(
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

    // ── PUT a new Session, then read it back byte-for-byte. ────────────────────
    let new = make_session("SESSION4", base_ts + Duration::seconds(10));
    client
        .put_session_2_3_0(&sessions_url, "CA", "CPO", "SESSION4", &new)
        .await
        .expect("PUT Session should succeed");
    let fetched = client
        .get_session_2_3_0(&sessions_url, "CA", "CPO", "SESSION4")
        .await
        .expect("GET of the just-PUT Session should return 200");
    assert_eq!(fetched, new);

    // ── PATCH (merge-patch) the status, leaving total_cost + taxes intact. ─────
    client
        .patch_session_2_3_0(
            &sessions_url,
            "CA",
            "CPO",
            "SESSION4",
            &json!({ "status": "COMPLETED" }),
        )
        .await
        .expect("PATCH Session should succeed");
    let patched = client
        .get_session_2_3_0(&sessions_url, "CA", "CPO", "SESSION4")
        .await
        .expect("GET of the patched Session should return 200");
    assert_eq!(patched.status, SessionStatus::Completed);
    assert_north_american_tax(
        patched
            .total_cost
            .as_ref()
            .expect("total_cost survives the merge-patch"),
    );

    // ── 404 paths: unknown GET and unknown PATCH. ──────────────────────────────
    let missing = client
        .get_session_2_3_0(&sessions_url, "CA", "CPO", "NOPE")
        .await;
    assert!(matches!(missing, Err(ClientError::NotFound)));
    let patch_missing = client
        .patch_session_2_3_0(&sessions_url, "CA", "CPO", "NOPE", &json!({ "kwh": 1.0 }))
        .await;
    assert!(matches!(patch_missing, Err(ClientError::NotFound)));
}

/// The serialized 2.3.0 Session must carry the itemised `taxes` list on
/// `total_cost` — the field that is the whole reason the 2.3.0 `Session` forks.
#[test]
fn session_2_3_0_wire_carries_itemised_taxes() {
    let ts = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).single().unwrap();
    let wire = serde_json::to_value(make_session("SESSION1", ts)).unwrap();
    let taxes = wire
        .get("total_cost")
        .and_then(|c| c.get("taxes"))
        .and_then(|t| t.as_array())
        .expect("total_cost.taxes must serialize as an array");
    assert_eq!(taxes.len(), 2);
    assert_eq!(taxes[0].get("name").and_then(|v| v.as_str()), Some("GST"));
    assert_eq!(taxes[1].get("name").and_then(|v| v.as_str()), Some("QST"));
}

/// The trust boundary: a `total_cost` present but omitting the required
/// `before_taxes` is **rejected on deserialize**, never silently defaulted — the
/// same guard the receiver's `PUT`/`PATCH` path relies on before it stores.
#[test]
fn session_2_3_0_total_cost_requires_before_taxes() {
    let mut wire = serde_json::to_value(make_session(
        "SESSION1",
        Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).single().unwrap(),
    ))
    .unwrap();
    // Drop the required `before_taxes` from the embedded Price.
    wire.get_mut("total_cost")
        .and_then(|c| c.as_object_mut())
        .expect("total_cost object")
        .remove("before_taxes");
    let parsed: Result<Session, _> = serde_json::from_value(wire);
    assert!(
        parsed.is_err(),
        "a total_cost missing before_taxes must be rejected on deserialize"
    );
}

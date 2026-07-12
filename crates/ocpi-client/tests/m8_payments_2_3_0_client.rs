//! M8 OCPI **2.3.0** Payments — PTP-Sender client round-trip smoke test
//! (issue #185, the PTP client-sender slice over the `v2_3_0::payments` types
//! landed in #176/#180; the CPO-receiver router is the complementary slice in
//! #193).
//!
//! Payments is asymmetric: the **PTP** (Payment Terminal Provider) is the
//! *Sender*, the **CPO** is the *Receiver*. This test drives the CPO→PTP
//! **Sender** getters — the read side a hub uses to pull a PTP's `Terminal`
//! catalogue and its `FinancialAdviceConfirmation`s. Because the PTP Sender
//! interface has no crate-provided router (only the CPO *Receiver* router
//! exists, in a separate slice), the test stands up a **minimal in-process
//! `axum` stub** that speaks the PTP Sender wire shape — a real loopback
//! transport driven entirely through [`OcpiClient`]'s `_2_3_0` getters (no raw
//! `reqwest`).
//!
//! ## Sender endpoints exercised (§82)
//!
//! Read side (`..._ptp_sender_round_trip`):
//! - list:   `GET {base}/payments/terminals?…`                        (paginated)
//! - object: `GET {base}/payments/terminals/{terminal_id}`
//! - list:   `GET {base}/payments/financial-advice-confirmations?…`   (paginated)
//! - object: `GET {base}/payments/financial-advice-confirmations/{id}`
//!
//! Mutating side (`..._ptp_sender_terminal_mutations`, the #185 follow-up slice):
//! - activate:   `POST {base}/payments/terminals/activate` (PTP assigns the id)
//! - update:     `PUT {base}/payments/terminals/{terminal_id}`
//! - assign:     `PATCH {base}/payments/terminals/{terminal_id}`
//! - deactivate: `POST {base}/payments/terminals/{terminal_id}/deactivate`
//!
//! Spec: OCPI 2.3.0 — *Payments* module (`specs/ocpi/2.3.0/mod_payments.asciidoc`).

use axum::{
    extract::Path,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};

use ocpi_client::{ClientError, OcpiClient};
use ocpi_types::{
    serde_json::{self, json},
    transport::PaginatedParams,
    v2_3_0::{CaptureStatusCode, FinancialAdviceConfirmation, Terminal},
    OcpiResponse,
};

/// The bearer the CPO (or a hub acting for it) presents to the PTP's Sender.
const TOKEN: &str = "TOKEN_C_cpo_calls_ptp";

/// A spec-faithful 2.3.0 `Terminal` (§Terminal object). Carries the optional
/// address/coordinate/invoice fields plus the `location_ids`/`evse_uids` maps
/// so the round-trip proves the full shape survives the wire unmangled.
fn terminal_json(terminal_id: &str) -> serde_json::Value {
    json!({
        "terminal_id": terminal_id,
        "customer_reference": "CUST-42",
        "party_id": "PTP",
        "country_code": "NL",
        "address": "F.Rooseveltlaan 3A",
        "city": "Gent",
        "postal_code": "9000",
        "country": "BEL",
        "coordinates": { "latitude": "51.047599", "longitude": "3.729944" },
        "invoice_base_url": "https://ptp.example/invoices",
        "invoice_creator": "CPO",
        "location_ids": ["LOC1"],
        "evse_uids": ["3256"],
        "last_updated": "2026-01-01T00:00:00Z"
    })
}

/// A spec-faithful 2.3.0 `FinancialAdviceConfirmation` (§FinancialAdvice object)
/// with the given id and capture status.
fn confirmation_json(id: &str, capture_status: &str) -> serde_json::Value {
    json!({
        "id": id,
        "authorization_reference": "AUTH-REF-1",
        "total_costs": { "excl_vat": 8.50, "incl_vat": 10.29 },
        "currency": "EUR",
        "eft_data": ["**** **** **** 1234"],
        "capture_status_code": capture_status,
        "last_updated": "2026-01-01T00:00:00Z"
    })
}

fn terminal(terminal_id: &str) -> Terminal {
    serde_json::from_value(terminal_json(terminal_id)).expect("valid 2.3.0 Terminal")
}

fn confirmation(id: &str, capture_status: &str) -> FinancialAdviceConfirmation {
    serde_json::from_value(confirmation_json(id, capture_status))
        .expect("valid 2.3.0 FinancialAdviceConfirmation")
}

// ── PTP-Sender stub handlers ────────────────────────────────────────────────

/// `GET /payments/terminals` — returns two terminals plus a first-page
/// `Link`/`X-Total-Count`/`X-Limit` set advertising a (nonexistent) second page,
/// so the client's pagination-header parsing is exercised end-to-end.
async fn list_terminals() -> impl IntoResponse {
    let body = OcpiResponse::success(vec![terminal("TERM0001"), terminal("TERM0002")]);
    let headers = [
        ("X-Total-Count", "3"),
        ("X-Limit", "2"),
        (
            "Link",
            r#"</payments/terminals?offset=2&limit=2>; rel="next""#,
        ),
    ];
    (headers, Json(body))
}

/// `GET /payments/terminals/{terminal_id}` — one terminal, or 404 for an
/// unknown id (mirrors the receiver's not-found path).
async fn get_terminal(Path(terminal_id): Path<String>) -> axum::response::Response {
    if terminal_id == "TERM0001" {
        Json(OcpiResponse::success(terminal("TERM0001"))).into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

/// `GET /payments/financial-advice-confirmations` — a `SUCCESS` and a `FAILED`
/// confirmation on a single terminal page (no further `Link`).
async fn list_confirmations() -> impl IntoResponse {
    let body = OcpiResponse::success(vec![
        confirmation("FAC0001", "SUCCESS"),
        confirmation("FAC0002", "FAILED"),
    ]);
    let headers = [("X-Total-Count", "2"), ("X-Limit", "50")];
    (headers, Json(body))
}

/// `GET /payments/financial-advice-confirmations/{id}` — one confirmation, or
/// 404 for an unknown id.
async fn get_confirmation(Path(id): Path<String>) -> axum::response::Response {
    if id == "FAC0001" {
        Json(OcpiResponse::success(confirmation("FAC0001", "SUCCESS"))).into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

// ── PTP-Sender mutating stub handlers (§82) ─────────────────────────────────

/// `POST /payments/terminals/activate` — the PTP assigns the real `terminal_id`
/// (the request body may carry only a placeholder) and echoes back the created
/// terminal in the envelope `data`, mirroring the spec's "PTP sets the id" note.
async fn activate_terminal(Json(body): Json<Terminal>) -> impl IntoResponse {
    // The PTP ignores the client-supplied placeholder id and mints its own.
    let mut created = body;
    created.terminal_id = "PTP-ASSIGNED-0001".try_into().unwrap();
    Json(OcpiResponse::success(created))
}

/// `POST /payments/terminals/{terminal_id}/deactivate` — a status-only
/// acknowledgement for a known id, or 404 for an unknown one.
async fn deactivate_terminal(Path(terminal_id): Path<String>) -> axum::response::Response {
    if terminal_id == "TERM0001" {
        Json(OcpiResponse::<()>::success_empty()).into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

/// `PUT /payments/terminals/{terminal_id}` — accept a full terminal replace for
/// a known id (status-only ack), 404 otherwise.
async fn put_terminal(
    Path(terminal_id): Path<String>,
    Json(_body): Json<Terminal>,
) -> axum::response::Response {
    if terminal_id == "TERM0001" {
        Json(OcpiResponse::<()>::success_empty()).into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

/// `PATCH /payments/terminals/{terminal_id}` — accept a Location/EVSE-assignment
/// merge-patch for a known id (status-only ack), 404 otherwise.
async fn patch_terminal(
    Path(terminal_id): Path<String>,
    Json(_body): Json<serde_json::Value>,
) -> axum::response::Response {
    if terminal_id == "TERM0001" {
        Json(OcpiResponse::<()>::success_empty()).into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

/// The minimal PTP Sender interface the getters call.
fn ptp_sender_router() -> Router {
    Router::new()
        .route("/payments/terminals", get(list_terminals))
        .route("/payments/terminals/activate", post(activate_terminal))
        .route(
            "/payments/terminals/{terminal_id}/deactivate",
            post(deactivate_terminal),
        )
        .route(
            "/payments/terminals/{terminal_id}",
            get(get_terminal).put(put_terminal).patch(patch_terminal),
        )
        .route(
            "/payments/financial-advice-confirmations",
            get(list_confirmations),
        )
        .route(
            "/payments/financial-advice-confirmations/{id}",
            get(get_confirmation),
        )
}

/// Bind an ephemeral loopback socket and return it with its origin URL.
async fn bind() -> (tokio::net::TcpListener, String) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    (listener, base)
}

/// Serve `router` on `listener` on a background task.
fn serve(listener: tokio::net::TcpListener, router: Router) {
    tokio::spawn(async move {
        axum::serve(listener, router.into_make_service())
            .await
            .unwrap();
    });
}

#[tokio::test]
async fn m8_payments_2_3_0_ptp_sender_round_trip() {
    let (listener, base) = bind().await;
    serve(listener, ptp_sender_router());

    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN);
    let terminals_url = format!("{base}/payments/terminals");
    let confirmations_url = format!("{base}/payments/financial-advice-confirmations");

    // ── GET single Terminal → full 2.3.0 serde round-trip. ─────────────────
    let term = client
        .get_terminal_2_3_0(&terminals_url, "TERM0001")
        .await
        .expect("GET single terminal should return 200");
    assert_eq!(term.terminal_id.as_str(), "TERM0001");
    assert_eq!(term.party_id.as_ref().map(|s| s.as_str()), Some("PTP"));
    assert_eq!(term.location_ids.len(), 1);
    assert_eq!(term.evse_uids.first().map(|s| s.as_str()), Some("3256"));
    // The just-fetched object is byte-for-byte the stub fixture — the 2.3.0
    // shape is served unmangled (not coerced through a 2.2.1 surface).
    assert_eq!(term, terminal("TERM0001"));

    // ── GET paginated Terminal list → header-driven PaginationMeta. ────────
    let (terminals, meta) = client
        .get_terminals_2_3_0(
            &terminals_url,
            PaginatedParams {
                limit: Some(2),
                ..Default::default()
            },
        )
        .await
        .expect("GET terminals list should return 200");
    assert_eq!(terminals.len(), 2);
    assert_eq!(meta.total_count, 3, "X-Total-Count reflects the full set");
    assert_eq!(meta.limit, 2, "X-Limit echoes the page size");
    let next = meta
        .next_url
        .expect("a Link: rel=next must advertise page 2");
    assert!(next.contains("offset=2"), "next link: {next}");

    // ── GET single FinancialAdviceConfirmation → round-trip. ───────────────
    let fac = client
        .get_financial_advice_confirmation_2_3_0(&confirmations_url, "FAC0001")
        .await
        .expect("GET single confirmation should return 200");
    assert_eq!(fac.id.as_str(), "FAC0001");
    assert_eq!(fac.authorization_reference.as_str(), "AUTH-REF-1");
    assert_eq!(fac.capture_status_code, CaptureStatusCode::Success);
    assert_eq!(fac.currency.as_str(), "EUR");
    assert_eq!(fac.eft_data.len(), 1);

    // ── GET FinancialAdviceConfirmation list → both capture outcomes. ──────
    let (confirmations, meta2) = client
        .get_financial_advice_confirmations_2_3_0(&confirmations_url, PaginatedParams::default())
        .await
        .expect("GET confirmations list should return 200");
    assert_eq!(confirmations.len(), 2);
    assert_eq!(meta2.total_count, 2);
    assert!(
        meta2.next_url.is_none(),
        "the single confirmation page must not advertise a next link"
    );
    // A SUCCESS and a FAILED confirmation both survive the wire — the two
    // capture outcomes a settlement path must distinguish.
    assert_eq!(
        confirmations[0].capture_status_code,
        CaptureStatusCode::Success
    );
    assert_eq!(
        confirmations[1].capture_status_code,
        CaptureStatusCode::Failed
    );

    // ── 404 paths: unknown ids map to ClientError::NotFound. ───────────────
    let missing_term = client.get_terminal_2_3_0(&terminals_url, "NOPE").await;
    assert!(matches!(missing_term, Err(ClientError::NotFound)));
    let missing_fac = client
        .get_financial_advice_confirmation_2_3_0(&confirmations_url, "NOPE")
        .await;
    assert!(matches!(missing_fac, Err(ClientError::NotFound)));
}

#[tokio::test]
async fn m8_payments_2_3_0_ptp_sender_terminal_mutations() {
    let (listener, base) = bind().await;
    serve(listener, ptp_sender_router());

    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN);
    let terminals_url = format!("{base}/payments/terminals");

    // ── POST /activate → the PTP assigns the terminal_id and returns it. ───
    // The CPO sends a Terminal carrying only mapping data; its own placeholder
    // id is discarded and the PTP-minted id comes back in the envelope.
    let activated = client
        .activate_terminal_2_3_0(&terminals_url, &terminal("PLACEHOLDER"))
        .await
        .expect("activate should return 200");
    let activated = activated.expect("activation echoes the created terminal");
    assert_eq!(
        activated.terminal_id.as_str(),
        "PTP-ASSIGNED-0001",
        "the PTP assigns the terminal_id, not the caller"
    );
    // The mapping data the CPO supplied survives the round-trip unmangled.
    assert_eq!(
        activated.location_ids.first().map(|s| s.as_str()),
        Some("LOC1")
    );

    // ── PUT /{id} → full replace of a known terminal (status-only ack). ────
    client
        .put_terminal_2_3_0(&terminals_url, "TERM0001", &terminal("TERM0001"))
        .await
        .expect("PUT of a known terminal should return 200");

    // ── PATCH /{id} → assign location_ids/evse_uids (merge-patch). ─────────
    let assignment = json!({ "location_ids": ["LOC1", "LOC2"], "evse_uids": ["3256"] });
    client
        .patch_terminal_2_3_0(&terminals_url, "TERM0001", &assignment)
        .await
        .expect("PATCH of a known terminal should return 200");

    // ── POST /{id}/deactivate → status-only ack for a known terminal. ─────
    client
        .deactivate_terminal_2_3_0(&terminals_url, "TERM0001")
        .await
        .expect("deactivate of a known terminal should return 200");

    // ── 404 paths: an unknown terminal id maps to ClientError::NotFound on
    //    every mutating verb that addresses a specific terminal. ───────────
    assert!(matches!(
        client
            .put_terminal_2_3_0(&terminals_url, "NOPE", &terminal("NOPE"))
            .await,
        Err(ClientError::NotFound)
    ));
    assert!(matches!(
        client
            .patch_terminal_2_3_0(&terminals_url, "NOPE", &json!({ "location_ids": [] }))
            .await,
        Err(ClientError::NotFound)
    ));
    assert!(matches!(
        client
            .deactivate_terminal_2_3_0(&terminals_url, "NOPE")
            .await,
        Err(ClientError::NotFound)
    ));
}

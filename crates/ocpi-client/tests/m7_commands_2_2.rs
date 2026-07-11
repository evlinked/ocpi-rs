//! M7 OCPI **2.2** Commands — client + server round-trip smoke test (issue #165).
//!
//! Stands up an in-process `axum` server hosting the real [`commands_2_2_router`]
//! on an ephemeral `127.0.0.1` port and drives the 2.2 Commands exchange through
//! [`OcpiClient`]. The delta that matters: `POST /commands/START_SESSION`
//! deserializes into the **2.2** [`ocpi_types::v2_2::StartSession`] — no
//! `connector_id` — so a Sender-pinned connector cannot be carried into the
//! session. Every other command body and the async `CommandResult` callback are
//! wire-identical to 2.2.1, so this test drives the callback with the existing
//! [`OcpiClient::post_command_result`] against the same 2.2 router.
//!
//! Like the 2.1.1 Commands wiring (#124), the default [`Commands22Config`] is a
//! stateless placeholder that acks `NOT_SUPPORTED`; this test therefore exercises
//! the full HTTP path (client → axum → 2.2 deserialize → response envelope →
//! client parse) end-to-end and asserts the placeholder's envelope. Handler-level
//! behaviour and the `connector_id`-drop guarantee are covered by the
//! `commands_22_tests` unit module in `ocpi-server` and by `v2_2::commands`.
//!
//! ## URL shapes (identical to 2.2.1)
//!
//! Commands is a verb-style RPC keyed by the Sender-supplied `response_url`, so
//! the paths are flat, with no `{country_code}/{party_id}` segments:
//!
//! - receiver: `POST {base}/commands/START_SESSION`      → `CommandResponse` ack
//! - callback: `POST {base}/commands/{command}/result`   → `CommandResult` sink
//!
//! Spec: `specs/ocpi/2.2/mod_commands.asciidoc`.

use std::sync::Arc;

use ocpi_client::OcpiClient;
use ocpi_server::{http::commands_2_2_router, Commands22Config};
use ocpi_types::serde_json;
use ocpi_types::v2_2::StartSession;
use ocpi_types::v2_2_1::{CommandResponseType, CommandResult, CommandResultType};

/// The bearer the eMSP presents when POSTing a command to the CPO.
const TOKEN: &str = "TOKEN_C_emsp_sends_command";

/// A spec-shaped 2.2 `START_SESSION` (no `connector_id`).
fn start_session(response_url: &str) -> StartSession {
    let json = serde_json::json!({
        "response_url": response_url,
        "token": {
            "country_code": "DE",
            "party_id": "TNM",
            "uid": "12345678905880",
            "type": "RFID",
            "contract_id": "DE8ACC12E46L89",
            "issuer": "TheNewMotion",
            "valid": true,
            "whitelist": "ALWAYS",
            "last_updated": "2018-12-10T17:25:10Z"
        },
        "location_id": "LOC1",
        "evse_uid": "EVSE1"
    });
    serde_json::from_value(json).expect("valid 2.2 StartSession")
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

#[tokio::test]
async fn m7_commands_2_2_start_session_and_result_callback() {
    let (listener, base) = bind().await;
    serve(
        listener,
        commands_2_2_router(Arc::new(Commands22Config::new())),
    );

    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN);
    let commands_url = format!("{base}/commands");

    // ── START_SESSION → the 2.2 body is deserialized on the wire and the CPO
    // returns a valid CommandResponse envelope (the placeholder acks
    // NOT_SUPPORTED with the 2.2.1-shaped `timeout`/`message` fields). ────────
    let response_url = format!("{base}/emsp/commands/START_SESSION/42");
    let ack = client
        .start_session_2_2(&commands_url, start_session(&response_url))
        .await
        .expect("START_SESSION should return a CommandResponse");
    assert_eq!(ack.result, CommandResponseType::NotSupported);
    assert_eq!(ack.timeout, 30);
    assert!(ack.message.is_empty());

    // ── Async result callback (wire-identical to 2.2.1) → the CommandResult
    // sink accepts it and the router returns a success envelope. ─────────────
    let result = CommandResult {
        result: CommandResultType::Accepted,
        message: vec![],
    };
    client
        .post_command_result(&format!("{commands_url}/START_SESSION/result"), result)
        .await
        .expect("posting the async result should succeed");
}

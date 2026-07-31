//! M8 OCPI **2.3.0** — *wire-identical module reuse*, verified at the transport
//! layer for the three modules #205 did not cover (issue #228).
//!
//! OCPI **2.3.0** builds *additively* on 2.2.1. Per
//! `specs/ocpi/2.3.0/changelog.asciidoc` ("Changes between 2.2.1-d2 and 2.3.0")
//! the entire 2.3.0-over-2.2.1 delta surface is Payments, the Locations
//! Parking/15118/`accepted_service_providers` additions, the North-American tax
//! rework (Tariffs/CDRs/Sessions), the Credentials `hub_party_id`, and the
//! "make OCPI extensible" serde policy. **Five** modules carry **no** wire
//! delta — Versions, Tokens, **Commands**, **ChargingProfiles**,
//! **HubClientInfo** — so `ocpi_types::v2_3_0::X` *is* `ocpi_types::v2_2_1::X`
//! (a compile-time identity, ratified by the `reuse_types_stay_aliases_of_2_2_1`
//! assertions in `crates/ocpi-types/src/v2_3_0/mod.rs`).
//!
//! **Tokens** already has its reuse *exercised* end-to-end over the unqualified
//! 2.2.1 transport (`m8_tokens_reuse_2_3_0.rs`, #205). This file closes the same
//! gap for the remaining three wire-identical modules: a `v2_3_0` value rides
//! the *existing* 2.2.1 client + server through real HTTP + serde and round-trips
//! unmangled — the design decision #205 ratified (**no `_2_3_0` client/server
//! method is minted for a wire-identical module**; aliasing identical-typed calls
//! would only imply a difference that does not exist), now proven at the
//! transport layer rather than only asserted at the type layer.
//!
//! Importing the driving types *through `ocpi_types::v2_3_0`* is the point of the
//! exercise: every call below is made by a 2.3.0 party.
//!
//! Spec: OCPI 2.3.0 — the Commands / Charging Profiles / Hub Client Info modules,
//! each wire-identical to `specs/ocpi/2.2.1/mod_{commands,charging_profiles,
//! hub_client_info}.asciidoc`.

use std::sync::Arc;

use ocpi_client::OcpiClient;
use ocpi_server::{
    http::{
        charging_profiles_router, charging_profiles_sender_router, commands_router,
        hub_client_info_router,
    },
    ChargingProfilesConfig, CommandsConfig, HubClientInfoConfig,
};
// The Commands / HubClientInfo driving types reached **through `v2_3_0`** — each
// is the very same type as its 2.2.1 counterpart (asserted at compile time in
// `v2_3_0/mod.rs`), so a 2.3.0 party's values feed the unqualified 2.2.1
// client/server surface directly.
use ocpi_types::v2_3_0::{
    ClientInfo, CommandResponseType, CommandResult, CommandResultType, ConnectionStatus,
    StartSession,
};
use ocpi_types::{
    chrono::{Duration, TimeZone as _},
    serde_json,
    transport::PaginatedParams,
    CiString2, CiString3, DateTime, Role, Utc,
};

/// The bearer the peer presents on these exchanges.
const TOKEN: &str = "TOKEN_C_reuse_2_3_0";

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

// ── Commands ──────────────────────────────────────────────────────────────────

/// A spec-shaped 2.3.0 `START_SESSION` body (wire-identical to 2.2.1: it carries
/// the full `Token`, and the connector fields stay optional).
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
    serde_json::from_value(json).expect("valid 2.3.0 StartSession")
}

/// A 2.3.0 party drives the wire-identical **Commands** module end-to-end over
/// the *existing* 2.2.1 `commands_router` — no `_2_3_0` method is minted or
/// needed. `START_SESSION` is deserialized on the wire into `v2_3_0::StartSession`
/// (== `v2_2_1::StartSession`), the CPO returns a valid `CommandResponse`, and the
/// async `CommandResult` callback rides the same router.
#[tokio::test]
async fn m8_commands_2_3_0_reuse_start_session_and_result_callback() {
    let (listener, base) = bind().await;
    serve(listener, commands_router(Arc::new(CommandsConfig::new())));

    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN);
    let commands_url = format!("{base}/commands");

    // ── START_SESSION → the 2.3.0 body deserializes on the wire and the CPO
    // returns a valid CommandResponse envelope (the default config acks
    // NOT_SUPPORTED with the 2.2.1-shaped `timeout`/`message` fields). ────────
    let response_url = format!("{base}/emsp/commands/START_SESSION/42");
    let ack = client
        .start_session(&commands_url, start_session(&response_url))
        .await
        .expect("START_SESSION should return a CommandResponse");
    assert_eq!(ack.result, CommandResponseType::NotSupported);
    assert_eq!(ack.timeout, 30);

    // ── Async result callback (wire-identical to 2.2.1) → the CommandResult
    // sink accepts the 2.3.0 party's result and the router returns success. ───
    let result = CommandResult {
        result: CommandResultType::Accepted,
        message: vec![],
    };
    client
        .post_command_result(&format!("{commands_url}/START_SESSION/result"), result)
        .await
        .expect("posting the async result should succeed");
}

// ── ChargingProfiles ──────────────────────────────────────────────────────────

/// A minimal `ActiveChargingProfile` as a 2.3.0 party (CPO) would push it.
///
/// `ActiveChargingProfile` has no `v2_3_0` re-export of its own — like
/// `AllowedType` in `m8_tokens_reuse_2_3_0.rs`, importing it from `v2_2_1` is
/// itself evidence the surface is unforked. Its **inner** `ChargingProfile` is
/// reached through `v2_3_0`, so the composite a 2.3.0 party builds is, nominally
/// and on the wire, the 2.2.1 one.
fn active_profile() -> ocpi_types::v2_2_1::ActiveChargingProfile {
    use ocpi_types::v2_3_0::{ChargingProfile, ChargingProfilePeriod, ChargingRateUnit};
    ocpi_types::v2_2_1::ActiveChargingProfile {
        start_date_time: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
        charging_profile: ChargingProfile {
            start_date_time: None,
            duration: Some(900),
            charging_rate_unit: ChargingRateUnit::W,
            min_charging_rate: None,
            charging_profile_period: vec![ChargingProfilePeriod {
                start_period: 0,
                limit: 11_000.0,
            }],
        },
    }
}

/// A 2.3.0 party drives the wire-identical **ChargingProfiles** module over the
/// *existing* 2.2.1 routers: the Sender PUT (`ActiveChargingProfile`) round-trips
/// against `charging_profiles_sender_router`, and the *same* body is rejected by
/// the Receiver `charging_profiles_router` (the two interfaces share the
/// `PUT /chargingprofiles/{session_id}` path but carry different bodies). This is
/// the 2.3.0 reuse mirror of `m6_chargingprofiles_sender_put.rs`.
#[tokio::test]
async fn m8_charging_profiles_2_3_0_reuse_sender_put_round_trips() {
    let (listener, base) = bind().await;
    serve(
        listener,
        charging_profiles_sender_router(Arc::new(ChargingProfilesConfig::new())),
    );

    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN);
    let ok = client
        .put_active_charging_profile(
            &format!("{base}/chargingprofiles"),
            "SESSION-2-3-0",
            active_profile(),
        )
        .await;
    assert!(
        ok.is_ok(),
        "a 2.3.0 party's Sender PUT of an ActiveChargingProfile must succeed \
         against the unqualified 2.2.1 sender router: {ok:?}"
    );
}

/// The Receiver (CPO) router's PUT expects a `SetChargingProfile`; a 2.3.0
/// party's `ActiveChargingProfile` body lacks `response_url`, so deserialization
/// fails — the same interface boundary that motivates the separate sender router,
/// still enforced when the caller is a 2.3.0 party.
#[tokio::test]
async fn m8_charging_profiles_2_3_0_reuse_sender_body_rejected_by_receiver() {
    let (listener, base) = bind().await;
    serve(
        listener,
        charging_profiles_router(Arc::new(ChargingProfilesConfig::new())),
    );

    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN);
    let err = client
        .put_active_charging_profile(
            &format!("{base}/chargingprofiles"),
            "SESSION-2-3-0",
            active_profile(),
        )
        .await;
    assert!(
        err.is_err(),
        "pushing an ActiveChargingProfile to the Receiver PUT must fail — the two \
         interfaces share a path but not a body, hence the separate sender router"
    );
}

// ── HubClientInfo ─────────────────────────────────────────────────────────────

/// Build a `ClientInfo` (reached through `v2_3_0`) mirroring the spec's example
/// shape. `ts` drives `last_updated` so pagination ordering is deterministic.
fn make_client_info(party_id: &str, status: ConnectionStatus, ts: DateTime<Utc>) -> ClientInfo {
    ClientInfo {
        party_id: CiString3::try_from(party_id).unwrap(),
        country_code: CiString2::try_from("NL").unwrap(),
        role: Role::Cpo,
        status,
        last_updated: ts,
    }
}

/// A 2.3.0 party drives the wire-identical **HubClientInfo** module end-to-end
/// over the *existing* 2.2.1 `hub_client_info_router`: it PUTs a `v2_3_0`
/// `ClientInfo` (Receiver upsert), reads it back via the Sender single-GET
/// byte-for-byte, and pages the Sender list — all with no `_2_3_0` method.
#[tokio::test]
async fn m8_hub_client_info_2_3_0_reuse_put_get_list_round_trip() {
    let store = Arc::new(HubClientInfoConfig::new());
    let base_ts = Utc
        .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
        .single()
        .expect("valid timestamp");
    // Pre-seed two parties so the list endpoint exercises real pagination.
    for (i, party) in ["AAA", "BBB"].iter().enumerate() {
        store.put(
            "NL",
            party,
            make_client_info(
                party,
                ConnectionStatus::Connected,
                base_ts + Duration::seconds(i as i64),
            ),
        );
    }
    let (listener, base) = bind().await;
    serve(listener, hub_client_info_router(Arc::clone(&store)));

    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN);
    let clientinfo_url = format!("{base}/clientinfo");

    // ── Receiver PUT: a 2.3.0 party upserts a third entry over the wire. ─────
    let pushed = make_client_info(
        "CCC",
        ConnectionStatus::Connected,
        base_ts + Duration::seconds(2),
    );
    client
        .put_client_info(&clientinfo_url, "NL", "CCC", &pushed)
        .await
        .expect("PUT ClientInfo should succeed");

    // ── Sender single GET: the just-pushed 2.3.0 entry round-trips unmangled. ─
    let got = client
        .get_client_info(&clientinfo_url, "NL", "CCC")
        .await
        .expect("GET single ClientInfo should return 200");
    assert_eq!(got, pushed);

    // ── Sender paginated list — first page (limit 2 of 3). ───────────────────
    let (page1, meta) = client
        .get_client_infos(
            &clientinfo_url,
            &PaginatedParams {
                limit: Some(2),
                ..Default::default()
            },
        )
        .await
        .expect("GET clientinfo list should return 200");
    assert_eq!(
        meta.total_count, 3,
        "X-Total-Count reflects all three parties"
    );
    assert_eq!(meta.limit, 2);
    assert_eq!(page1.len(), 2);
    // Server sorts by last_updated, so AAA (earliest) precedes BBB.
    assert_eq!(page1[0].party_id.as_str(), "AAA");
    assert_eq!(page1[1].party_id.as_str(), "BBB");
    let next = meta
        .next_url
        .expect("a Link: rel=next header must advertise the second page");
    let next = format!("{base}{next}");

    // ── Follow the next-page link to drain the pushed 2.3.0 party. ───────────
    let (page2, meta2) = client
        .get_client_infos(&next, &PaginatedParams::default())
        .await
        .expect("GET next page should return 200");
    assert_eq!(page2.len(), 1);
    assert_eq!(page2[0], pushed);
    assert!(
        meta2.next_url.is_none(),
        "last page advertises no further link"
    );
}

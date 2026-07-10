//! M7 OCPI **2.1.1** cross-module end-to-end smoke test (issue #141).
//!
//! Stands up **one** in-process axum app assembling the real 2.1.1 routers —
//! [`versions_router`] (advertising a role-less 2.1.1 catalogue via
//! [`VersionsConfig::add_legacy_version`]), [`locations_2_1_1_router`], and
//! [`commands_2_1_1_router`] — on an ephemeral loopback port, then drives it
//! through [`OcpiClient`] exactly as a 2.2.1-capable eMSP discovering a
//! 2.1.1-only CPO would.
//!
//! ## Why these modules
//!
//! The per-module 2.1.1 client↔router HTTP round-trips already exist for
//! Sessions, CDRs, Tariffs, Tokens and Credentials (incl. the registration
//! fetch-back) in this directory. This test covers the transport surfaces
//! nothing exercises end-to-end yet:
//!
//! 1. **Version negotiation against a live legacy server** — `GET /versions`,
//!    highest-mutual-version selection falling back to 2.1.1, then the
//!    role-less `GET /versions/2.1.1` details fetch (endpoints without `role`)
//!    and endpoint discovery from the returned catalogue.
//! 2. **Locations receiver router driven by the real client** — the existing
//!    `m7_locations_2_1_1_client` test predates `locations_2_1_1_router` (#125)
//!    and replays JSON from a hand-rolled server; here the real router serves
//!    a store-seeded Location at all three object levels, plus the
//!    unknown-Location failure path (HTTP 404 carrying OCPI `2003`,
//!    surfaced by the client as [`ClientError::NotFound`]).
//! 3. **Commands over HTTP** — zero client↔router coverage existed. The
//!    client POSTs a `START_SESSION` (full 2.1.1 `Token` object on the wire,
//!    §13.3.3), gets the placeholder config's documented synchronous
//!    `NOT_SUPPORTED` ack back through the envelope, and then exercises the
//!    async-result callback route (`POST …/commands/START_SESSION/result`)
//!    that 2.1.1 serves with the *same* `CommandResponse` object (§13.2.2.1).
//!
//! All URLs the client hits after `GET /versions` are discovered from the
//! advertised catalogue — nothing is hard-coded past the bootstrap origin,
//! so the test fails if the routers and the advertised endpoints drift apart.
//!
//! Spec: OCPI 2.1.1 (`specs/ocpi/2.1.1/OCPI_2.1.1.pdf`) — §6 Versions,
//! §8 Locations (eMSP/receiver interface), §13 Commands.

use std::sync::Arc;

use ocpi_client::{negotiate_version, ClientError, OcpiClient, OcpiVersionFetcher};
use ocpi_server::{
    http::{commands_2_1_1_router, locations_2_1_1_router, versions_router},
    Commands2111Config, LegacyVersionFetcher, Locations2111Config, VersionsConfig,
};
use ocpi_types::{
    v2_1_1::{
        CommandResponse as CommandResponse2111, CommandResponseType as CommandResponseType2111,
        Endpoint as Endpoint2111, Location as Location2111, StartSession as StartSession2111,
        Token as Token2111, TokenType as TokenType2111, VersionDetails as Details2111,
    },
    CiString36, CiString64, ModuleID, Url as OcpiUrl, Version, VersionNumber, WhitelistType,
};

/// The bearer the eMSP presents (Token C). Sent raw — 2.1.1 predates the
/// 2.2.1 Base64 rule, hence `with_compat_raw_token(true)` on the client. The
/// routers under test don't enforce auth, so the mode is exercised, not
/// asserted.
const TOKEN: &str = "TOKEN_C_emsp_calls_cpo";

/// The OCPI 2.1.1 spec Location example (§8.3.1.1), trimmed to one EVSE +
/// connector — the same fixture `m7_locations_2_1_1_client.rs` replays.
/// 2.1.1 wire shape: required `type`, no `country_code`/`party_id` on the
/// object, singular `tariff_id` on the connector.
const LOCATION_JSON: &str = r#"{
    "id": "LOC1",
    "type": "ON_STREET",
    "name": "Gent Zuid",
    "address": "F.Rooseveltlaan 3A",
    "city": "Gent",
    "postal_code": "9000",
    "country": "BEL",
    "coordinates": { "latitude": "51.047590", "longitude": "3.729940" },
    "evses": [{
        "uid": "3256",
        "evse_id": "BE-BEC-E041503001",
        "status": "AVAILABLE",
        "capabilities": ["RESERVABLE"],
        "connectors": [{
            "id": "1",
            "standard": "IEC_62196_T2",
            "format": "CABLE",
            "power_type": "AC_3_PHASE",
            "voltage": 220,
            "amperage": 16,
            "tariff_id": "11",
            "last_updated": "2015-03-16T10:10:02Z"
        }],
        "physical_reference": "1",
        "floor_level": "-1",
        "last_updated": "2015-06-28T08:12:01Z"
    }],
    "last_updated": "2015-06-29T20:39:09Z"
}"#;

/// A `VersionsConfig` advertising **only 2.1.1**, with a role-less catalogue
/// pointing at the Locations + Commands routers this test mounts on `base`.
fn versions_config(base: &str) -> VersionsConfig {
    let mut cfg = VersionsConfig::new();
    cfg.add_legacy_version(
        Version {
            version: VersionNumber::V2_1_1,
            url: OcpiUrl::try_from(format!("{base}/versions/2.1.1").as_str()).unwrap(),
        },
        Details2111 {
            version: VersionNumber::V2_1_1,
            endpoints: vec![
                Endpoint2111 {
                    identifier: ModuleID::Locations,
                    url: OcpiUrl::try_from(format!("{base}/locations").as_str()).unwrap(),
                },
                Endpoint2111 {
                    identifier: ModuleID::Commands,
                    url: OcpiUrl::try_from(format!("{base}/commands").as_str()).unwrap(),
                },
            ],
        },
    );
    cfg
}

/// The whitelisted RFID token the eMSP asks the CPO to start a session for —
/// 2.1.1 shape: `auth_id` (not the 2.2.1 `contract_id`), no `country_code`/
/// `party_id`.
fn make_token() -> Token2111 {
    Token2111 {
        uid: CiString36::try_from("012345678").unwrap(),
        token_type: TokenType2111::Rfid,
        auth_id: CiString36::try_from("NL8ACC12E46L89").unwrap(),
        visual_number: None,
        issuer: CiString64::try_from("TheNewMotion").unwrap(),
        valid: true,
        whitelist: WhitelistType::Allowed,
        language: None,
        last_updated: "2015-06-29T22:39:09Z".parse().unwrap(),
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
async fn m7_e2e_2_1_1_versions_locations_commands_round_trip() {
    // ── CPO app: versions + Locations receiver + Commands, one router. ──────
    let (listener, base) = bind().await;
    let locations = Arc::new(Locations2111Config::new());
    locations.put(
        "NL",
        "CPO",
        "LOC1",
        ocpi_types::serde_json::from_str::<Location2111>(LOCATION_JSON).unwrap(),
    );
    let app = versions_router(versions_config(&base))
        .merge(locations_2_1_1_router(locations.clone()))
        .merge(commands_2_1_1_router(Arc::new(Commands2111Config::new())));
    serve(listener, app);

    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN)
        .with_compat_raw_token(true);

    // ── Step 1: version negotiation — a 2.2.1-capable eMSP meets a 2.1.1-only
    //    CPO and must fall back to the highest mutual version, 2.1.1. ─────────
    let remote = client.versions().await.unwrap();
    assert_eq!(remote.len(), 1, "CPO advertises exactly one version");
    let best = negotiate_version(&remote, &[VersionNumber::V2_2_1, VersionNumber::V2_1_1]);
    assert_eq!(best, Some(VersionNumber::V2_1_1));

    // Fetch the role-less 2.1.1 catalogue from the advertised URL. The type
    // itself is the assertion that no `role` field is on the wire:
    // `v2_1_1::Endpoint` has none, and unknown fields would not round-trip.
    let fetcher = OcpiVersionFetcher::new();
    let details =
        LegacyVersionFetcher::fetch_version_details(&fetcher, remote[0].url.as_str(), TOKEN)
            .await
            .unwrap();
    assert_eq!(details.version, VersionNumber::V2_1_1);
    let endpoint_url = |module: ModuleID| -> String {
        details
            .endpoints
            .iter()
            .find(|e| e.identifier == module)
            .unwrap_or_else(|| panic!("catalogue advertises {module:?}"))
            .url
            .as_str()
            .to_owned()
    };

    // ── Step 2: Locations — real receiver router, discovered endpoint, all
    //    three object levels. Receiver URLs append `{country_code}/{party_id}`
    //    then the object ids (§8.2.2). ─────────────────────────────────────────
    let locations_base = format!("{}/NL/CPO", endpoint_url(ModuleID::Locations));

    let location = client
        .get_location_2_1_1(&locations_base, "LOC1")
        .await
        .unwrap();
    assert_eq!(location.id.as_str(), "LOC1");
    assert_eq!(location.evses.len(), 1);

    let evse = client
        .get_evse_2_1_1(&locations_base, "LOC1", "3256")
        .await
        .unwrap();
    assert_eq!(evse.evse_id.as_ref().unwrap().as_str(), "BE-BEC-E041503001");

    let connector = client
        .get_connector_2_1_1(&locations_base, "LOC1", "3256", "1")
        .await
        .unwrap();
    // The 2.1.1 singular `tariff_id` (vs. the 2.2.1 `tariff_ids` list)
    // survived server → wire → client intact.
    assert_eq!(connector.tariff_id.as_ref().unwrap().as_str(), "11");

    // Failure path: unknown Location → HTTP 404 carrying OCPI 2003
    // (`UnknownLocation`), surfaced by the client as `NotFound`.
    let err = client
        .get_location_2_1_1(&locations_base, "LOC404")
        .await
        .unwrap_err();
    assert!(
        matches!(err, ClientError::NotFound),
        "expected NotFound, got: {err:?}"
    );

    // ── Step 3: Commands — START_SESSION with the full 2.1.1 Token object,
    //    then the async-result callback route. ───────────────────────────────
    let commands_url = endpoint_url(ModuleID::Commands);
    let response_url =
        OcpiUrl::try_from(format!("{commands_url}/START_SESSION/result").as_str()).unwrap();

    let ack = client
        .start_session_2_1_1(
            &commands_url,
            StartSession2111 {
                response_url,
                token: make_token(),
                location_id: "LOC1".to_owned(),
                evse_uid: Some("3256".to_owned()),
            },
        )
        .await
        .unwrap();
    // `Commands2111Config` is the documented placeholder: every command acks
    // synchronously with NOT_SUPPORTED. The value here is the full serde +
    // path + envelope round-trip, not the business outcome.
    assert_eq!(ack.result, CommandResponseType2111::NotSupported);

    // The async callback reuses the same `CommandResponse` object (§13.2.2.1).
    // POSTing it to the result route must parse the `{command_type}` path
    // segment and return a success envelope.
    client
        .post_command_result_2_1_1(
            &format!("{commands_url}/START_SESSION/result"),
            CommandResponse2111 {
                result: CommandResponseType2111::Accepted,
            },
        )
        .await
        .unwrap();
}

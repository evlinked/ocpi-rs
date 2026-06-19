//! M6 ChargingProfiles **Sender PUT** smoke test (issue #75).
//!
//! Exercises the proactive `ActiveChargingProfile` update the Receiver (typically
//! CPO) pushes to the Sender (typically SCSP/eMSP) over a real loopback
//! transport: an in-process `axum` server is stood up on an ephemeral
//! `127.0.0.1` port and driven entirely through [`OcpiClient`] (no raw
//! `reqwest`).
//!
//! Two assertions, both spec-grounded:
//!   1. `put_active_charging_profile` against `charging_profiles_sender_router`
//!      round-trips: the `ActiveChargingProfile` body deserializes, the default
//!      handler no-ops, and the empty-data envelope parses back to `Ok(())`.
//!   2. The *same* call against `charging_profiles_router` (the Receiver/CPO
//!      interface) is rejected — that `PUT` expects a `SetChargingProfile`, so an
//!      `ActiveChargingProfile` body fails to deserialize. This demonstrates *why*
//!      the two interfaces live on separate routers: they share the
//!      `PUT /chargingprofiles/{session_id}` path but carry different bodies.
//!
//! Spec: `specs/ocpi/2.2.1/mod_charging_profiles.asciidoc` — §Sender Interface,
//! `mod_charging_profiles_msp_put_method`.

use std::sync::Arc;

use ocpi_client::OcpiClient;
use ocpi_server::{
    http::{charging_profiles_router, charging_profiles_sender_router},
    ChargingProfilesConfig,
};
use ocpi_types::{
    chrono::TimeZone,
    v2_2_1::{ActiveChargingProfile, ChargingProfile, ChargingProfilePeriod, ChargingRateUnit},
    Utc,
};

const TOKEN: &str = "TOKEN_C_cpo_calls_scsp";

/// A minimal `ActiveChargingProfile` as the CPO would push it.
fn active_profile() -> ActiveChargingProfile {
    ActiveChargingProfile {
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
async fn sender_put_active_profile_round_trips() {
    let (listener, base) = bind().await;
    serve(
        listener,
        charging_profiles_sender_router(Arc::new(ChargingProfilesConfig::new())),
    );

    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN);
    let result = client
        .put_active_charging_profile(
            &format!("{base}/chargingprofiles"),
            "SESSION-1",
            active_profile(),
        )
        .await;

    assert!(
        result.is_ok(),
        "Sender PUT of an ActiveChargingProfile must succeed against the sender router: {result:?}"
    );
}

#[tokio::test]
async fn sender_put_rejected_by_receiver_router() {
    // The Receiver (CPO) router's PUT expects a `SetChargingProfile`; an
    // `ActiveChargingProfile` body lacks `response_url`, so deserialization fails.
    let (listener, base) = bind().await;
    serve(
        listener,
        charging_profiles_router(Arc::new(ChargingProfilesConfig::new())),
    );

    let client = OcpiClient::new(url::Url::parse(&format!("{base}/")).unwrap(), TOKEN);
    let result = client
        .put_active_charging_profile(
            &format!("{base}/chargingprofiles"),
            "SESSION-1",
            active_profile(),
        )
        .await;

    assert!(
        result.is_err(),
        "pushing an ActiveChargingProfile to the Receiver PUT must fail — the two \
         interfaces share a path but not a body, hence the separate sender router"
    );
}

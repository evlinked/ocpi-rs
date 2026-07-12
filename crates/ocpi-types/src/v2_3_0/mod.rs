//! OCPI **2.3.0** typed models — forward-coverage track (milestone **M8**).
//!
//! ## The key insight: 2.3.0 is a superset of 2.2.1
//!
//! OCPI **2.3.0** builds *additively* on **2.2.1**. Per
//! `specs/ocpi/2.3.0/changelog.asciidoc` ("Changes between 2.2.1-d2 and
//! 2.3.0"), the entire 2.3.0-over-2.2.1 delta surface is:
//!
//! - a **new Payments module** (Terminal + payment/financial objects) —
//!   `specs/ocpi/2.3.0/mod_payments.asciidoc`;
//! - **Locations** gains a [`ParkingType`]-bearing **Parking** object linked to
//!   the EVSE (with vehicle types), an `accepted_emsps` list on the EVSE, ISO
//!   15118 Plug-and-Charge compatibility flags on the [`Connector`], a support
//!   telephone number on the [`Location`], and accessibility information —
//!   `specs/ocpi/2.3.0/mod_locations.asciidoc`;
//! - **North-American tax** support on Tariffs / CDRs —
//!   `specs/ocpi/2.3.0/mod_tariffs.asciidoc`, `mod_cdrs.asciidoc`;
//! - **Credentials** gains a hub party ID and reports hub clients as normal
//!   credentials roles — `specs/ocpi/2.3.0/credentials.asciidoc`;
//! - "Make OCPI extensible": tolerate unknown modules / fields / certain enum
//!   values — `specs/ocpi/2.3.0/transport_and_format.asciidoc`.
//!
//! Everything else — the response envelope, status codes, the role-bearing
//! `Endpoint` / `VersionDetails` version layer (unchanged since **2.2**), and
//! the bulk of the functional objects (Sessions, Tokens, Commands,
//! ChargingProfiles, HubClientInfo, and the wire-identical Locations /
//! Tariffs / CDRs surface) — is **shared** between 2.2.1 and 2.3.0.
//!
//! ## Convention: alias-by-default, override-only-the-deltas
//!
//! This module mirrors the [`crate::v2_2`] back-coverage convention (#149): it
//! re-exports the wire-identical 2.2.1 types by default (`pub use
//! crate::v2_2_1::…`) and re-exports the shared version/endpoint layer from
//! [`crate::version`]. A follow-up PR that implements one of the deltas above
//! does so by:
//!
//! 1. adding a `v2_3_0`-local `mod` (e.g. `mod payments;`) whose type is shaped
//!    to the 2.3.0 wire, then
//! 2. dropping that type's name from the `pub use crate::v2_2_1::{…}` block
//!    below and re-exporting the local override in its place.
//!
//! Everything outside that delta set stays a plain re-export, so the 2.3.0
//! track stays small: no full duplication of the 2.2.1 type-set, and the
//! `reuse_types_stay_aliases_of_2_2_1` compile-time assertions make CI fail the
//! moment a re-export silently forks.
//!
//! ## Status of the deltas
//!
//! - **Payments** — implemented (#176): the new-module delta lands as the
//!   `v2_3_0`-local [`payments`] submodule (`Terminal`,
//!   `FinancialAdviceConfirmation`, `InvoiceCreator`, `CaptureStatusCode`).
//! - **Credentials** — implemented (#179): [`credentials::Credentials`] adds the
//!   optional `hub_party_id` field; [`CredentialsRole`] stays a re-export
//!   (wire-identical — the 2.3.0 change is only *which* roles a hub lists).
//! - The Locations Parking/`accepted_emsps`/15118 additions and the
//!   North-American tax fields are not implemented yet — each lands in its own
//!   follow-up over this module.
//!
//! Until a module's full transport (types + client + server) is in place the
//! README support-matrix 2.3.0 column stays ☐ (planned), not ◑/☑.

// ── Payments (new in 2.3.0) ───────────────────────────────────────────────────
//
// The Payments module has no 2.2.1 predecessor, so it is a `v2_3_0`-local
// module rather than a re-export. Its wire identifier is the shared
// `ModuleID::Payments` variant (`"payments"`), added to the version enum for
// this release.
pub mod payments;
pub use payments::{CaptureStatusCode, FinancialAdviceConfirmation, InvoiceCreator, Terminal};

// ── Version / endpoint layer ──────────────────────────────────────────────────
//
// The Sender/Receiver `role` split arrived in OCPI **2.2** and is unchanged in
// 2.3.0, so the 2.3.0 `Endpoint` / `VersionDetails` are the shared
// `crate::version` types (the same ones 2.2 / 2.2.1 use), not a `v2_3_0`-local
// override. `VersionNumber` already carries the `V2_3_0` variant. The
// `payments` ModuleID that 2.3.0 adds is a shared-enum addition tracked with
// the Payments module follow-up.
pub use crate::version::{
    Endpoint, InterfaceRole, ModuleID, Version, VersionDetails, VersionNumber,
};

// ── 2.3.0-local delta modules ─────────────────────────────────────────────────
//
// The first genuine 2.3.0-vs-2.2.1 wire override: Credentials gains the
// `hub_party_id` field. `CredentialsRole` is re-exported from `credentials`
// (which itself re-exports the 2.2.1 type) so the two names travel together.
mod credentials;
pub use credentials::{Credentials, CredentialsRole};

// ── Functional + configuration module types ───────────────────────────────────
//
// Wire-identical to 2.2.1 → plain re-exports. Every 2.3.0-vs-2.2.1 delta
// (Payments, the Locations Parking/accepted_emsps/15118 additions, the tax
// fields, the Credentials hub additions) is additive and lands as a
// `v2_3_0`-local override in a follow-up; until then the full 2.2.1 surface is
// reachable here unchanged.
pub use crate::v2_2_1::{
    ActiveChargingProfile, ActiveChargingProfileResult, AdditionalGeoLocation, AllowedType,
    AuthMethod, AuthorizationInfo, CancelReservation, Capability, Cdr, CdrDimension,
    CdrDimensionType, CdrLocation, CdrToken, ChargingPeriod, ChargingPreferences,
    ChargingPreferencesResponse, ChargingProfile, ChargingProfilePeriod, ChargingProfileResponse,
    ChargingProfileResponseType, ChargingProfileResult, ChargingProfileResultType,
    ChargingRateUnit, ClearProfileResult, ClientInfo, CommandResponse, CommandResponseType,
    CommandResult, CommandResultType, CommandType, ConnectionStatus, Connector, ConnectorFormat,
    ConnectorType, DayOfWeek, EnergyContract, Evse, ExceptionalPeriod, Facility, Hours,
    ImageCategory, Location, LocationReferences, ParkingRestriction, ParkingType, PowerType,
    PriceComponent, ProfileType, PublishTokenType, RegularHours, ReservationRestrictionType,
    ReserveNow, Session, SessionStatus, SetChargingProfile, SignedData, SignedValue, StartSession,
    Status, StatusSchedule, StopSession, Tariff, TariffDimensionType, TariffElement,
    TariffRestrictions, TariffType, Token, TokenType, UnlockConnector, WhitelistType,
};

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{Endpoint, InterfaceRole, ModuleID, Version, VersionDetails, VersionNumber};
    use crate::common::Url;

    #[test]
    fn version_number_v2_3_0_serializes_as_dotted_string() {
        assert_eq!(
            serde_json::to_string(&VersionNumber::V2_3_0).unwrap(),
            "\"2.3.0\""
        );
        assert_eq!(
            "2.3.0".parse::<VersionNumber>().unwrap(),
            VersionNumber::V2_3_0
        );
    }

    #[test]
    fn endpoint_2_3_0_carries_role_on_the_wire() {
        // The Sender/Receiver split arrived in 2.2 and is unchanged in 2.3.0, so
        // a 2.3.0 endpoint MUST serialize a `role` key (unlike role-less 2.1.1).
        let ep = Endpoint {
            identifier: ModuleID::Locations,
            role: InterfaceRole::Sender,
            url: Url::try_from("https://example.com/ocpi/cpo/2.3.0/locations").unwrap(),
        };
        let json = serde_json::to_string(&ep).unwrap();
        assert!(
            json.contains("\"role\""),
            "2.3.0 endpoint must carry role: {json}"
        );
        assert!(json.contains("\"SENDER\""));
        let back: Endpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ep);
    }

    #[test]
    fn version_details_2_3_0_round_trips_with_roles() {
        // A 2.3.0 version-details catalogue: role-bearing endpoints, version
        // "2.3.0". Spec: specs/ocpi/2.3.0/version_information_endpoint.asciidoc.
        let json = r#"{
            "version": "2.3.0",
            "endpoints": [
                {
                    "identifier": "credentials",
                    "role": "SENDER",
                    "url": "https://example.com/ocpi/2.3.0/credentials"
                },
                {
                    "identifier": "locations",
                    "role": "SENDER",
                    "url": "https://example.com/ocpi/cpo/2.3.0/locations"
                },
                {
                    "identifier": "tokens",
                    "role": "RECEIVER",
                    "url": "https://example.com/ocpi/cpo/2.3.0/tokens"
                }
            ]
        }"#;
        let details: VersionDetails = serde_json::from_str(json).unwrap();
        assert_eq!(details.version, VersionNumber::V2_3_0);
        assert_eq!(details.endpoints.len(), 3);
        assert_eq!(details.endpoints[0].role, InterfaceRole::Sender);
        assert_eq!(details.endpoints[2].role, InterfaceRole::Receiver);

        let out = serde_json::to_string(&details).unwrap();
        let back: VersionDetails = serde_json::from_str(&out).unwrap();
        assert_eq!(back, details);
    }

    #[test]
    fn version_entry_2_3_0_round_trips() {
        let v = Version {
            version: VersionNumber::V2_3_0,
            url: Url::try_from("https://example.com/ocpi/cpo/2.3.0").unwrap(),
        };
        let back: Version = serde_json::from_str(&serde_json::to_string(&v).unwrap()).unwrap();
        assert_eq!(back, v);
        assert_eq!(back.version.as_str(), "2.3.0");
    }

    #[test]
    fn session_round_trips_through_the_2_3_0_re_export() {
        // A functional object reached through the `v2_3_0` re-export must
        // deserialize and round-trip exactly as its 2.2.1 source type — the
        // re-export is transparent, not a shadowing copy.
        let json = r#"{
            "country_code": "NL",
            "party_id": "ABC",
            "id": "session-2-3-0",
            "start_date_time": "2026-07-11T09:00:00Z",
            "kwh": 12.5,
            "cdr_token": {
                "country_code": "NL",
                "party_id": "ABC",
                "uid": "12345",
                "type": "RFID",
                "contract_id": "NL-ABC-C12345"
            },
            "auth_method": "WHITELIST",
            "location_id": "LOC1",
            "evse_uid": "EVSE1",
            "connector_id": "1",
            "currency": "EUR",
            "status": "ACTIVE",
            "last_updated": "2026-07-11T09:05:00Z"
        }"#;
        let via_2_3_0: super::Session = serde_json::from_str(json).unwrap();
        let via_2_2_1: crate::v2_2_1::Session = serde_json::from_str(json).unwrap();
        assert_eq!(via_2_3_0, via_2_2_1);
        let out = serde_json::to_string(&via_2_3_0).unwrap();
        let back: super::Session = serde_json::from_str(&out).unwrap();
        assert_eq!(back, via_2_3_0);
    }

    #[test]
    fn reuse_types_stay_aliases_of_2_2_1() {
        // Until a genuine 2.3.0 wire-delta override lands (Payments, the
        // Locations Parking/15118/accepted_emsps additions, the tax fields, the
        // Credentials hub additions), every `v2_3_0::X` MUST be the *very same
        // type* as `v2_2_1::X`. Each identity closure `|x| x` only typechecks
        // when the two paths name one nominal type (Rust does no subtyping
        // between distinct structs/enums), so a line here stops compiling the
        // moment a re-export is silently forked — a zero-cost, compile-time
        // alias assertion CI keeps honest. This is the same idiom `v2_2` uses.
        //
        // A representative type from each module the 2.3.0 changelog touches (so
        // the guard fires exactly where a future override would): Payments has
        // no re-export yet; the rest are covered below.
        let _: fn(crate::v2_2_1::Location) -> super::Location = |x| x; // Locations composite
        let _: fn(crate::v2_2_1::Evse) -> super::Evse = |x| x;
        let _: fn(crate::v2_2_1::Connector) -> super::Connector = |x| x; // 15118 flags land here
        let _: fn(crate::v2_2_1::Tariff) -> super::Tariff = |x| x; // NA tax fields land here
        let _: fn(crate::v2_2_1::Cdr) -> super::Cdr = |x| x; // NA tax fields land here
                                                             // NOTE: `Credentials` is intentionally *absent* here — #179 forked it
                                                             // into a `v2_3_0`-local override (the `hub_party_id` field), so it is no
                                                             // longer an alias of `v2_2_1::Credentials`. `CredentialsRole` stays
                                                             // wire-identical and is still covered by the re-export path.
        let _: fn(crate::v2_2_1::CredentialsRole) -> super::CredentialsRole = |x| x;
        let _: fn(crate::v2_2_1::Session) -> super::Session = |x| x;
        let _: fn(crate::v2_2_1::Token) -> super::Token = |x| x;
        let _: fn(crate::v2_2_1::StartSession) -> super::StartSession = |x| x;
        let _: fn(crate::v2_2_1::ChargingProfile) -> super::ChargingProfile = |x| x;
        let _: fn(crate::v2_2_1::ClientInfo) -> super::ClientInfo = |x| x; // HubClientInfo
    }
}

//! OCPI **2.2** typed models — back-coverage track (milestone **M7**).
//!
//! ## The key insight: 2.2 and 2.2.1 are near-identical
//!
//! OCPI **2.2.1** is a *patch* release over **2.2**. Per
//! `specs/ocpi/2.2.1/version_history.asciidoc`, 2.2.1 over 2.2 only:
//!
//! - added `country_code` / `party_id` to [`CdrToken`];
//! - added `connector_id` to the [`StartSession`] command (+ a
//!   `START_SESSION_CONNECTOR_REQUIRED` capability);
//! - added `AC_2_PHASE` / `AC_2_PHASE_SPLIT` to [`PowerType`] and several
//!   [`ConnectorType`] values;
//! - added the optional `home_charging_compensation` field to [`Cdr`];
//! - fixed [`SignedData`] field types/lengths, made `postal_code` optional and
//!   added `state` to [`CdrLocation`];
//! - description / example fixes.
//!
//! Everything else — the `role` field on [`Endpoint`] (introduced in **2.2**,
//! `specs/ocpi/2.2/version_information_endpoint.asciidoc`), the `roles` array on
//! [`Credentials`], HubClientInfo, ChargingProfiles, and the `OCPI-to/from-*`
//! routing headers — is **shared** between 2.2 and 2.2.1.
//!
//! ## Convention: alias-by-default, override-only-the-deltas
//!
//! `v2_2` re-exports the wire-identical 2.2.1 types by default
//! (`pub use crate::v2_2_1::…`) and re-exports the shared version/endpoint
//! layer from [`crate::version`]. It defines its **own** version of a type only
//! for a genuine 2.2-vs-2.2.1 **wire** delta — never for a mere alias. This
//! keeps the 2.2 track small: no full duplication of the 2.2.1 type-set.
//!
//! A follow-up PR (issue #153) that implements a delta does so by:
//!
//! 1. adding a `v2_2`-local `mod` (e.g. `mod cdrs;`) whose type is shaped to
//!    the 2.2 wire, then
//! 2. dropping that type's name from the `pub use crate::v2_2_1::{…}` block
//!    below and re-exporting the local override in its place.
//!
//! Everything outside that delta set stays a plain re-export.
//!
//! The known deltas — to be overridden in follow-ups — are: [`CdrToken`],
//! [`Cdr`], [`CdrLocation`], [`SignedData`], [`StartSession`], [`PowerType`],
//! and [`ConnectorType`]. Until #153 lands they are aliases of their 2.2.1
//! counterparts (proven by the alias assertions in the tests below).

// ── Version / endpoint layer ──────────────────────────────────────────────────
//
// The Sender/Receiver `role` split arrived in OCPI **2.2**, so the 2.2
// `Endpoint` / `VersionDetails` carry `role` and are wire-identical to 2.2.1 —
// they are the shared `crate::version` types (unlike the role-less
// `v2_1_1::Endpoint`). `VersionNumber` / `ModuleID` are version-agnostic and
// reused as-is; `ModuleID`'s variant set already covers the full 2.2 module
// set (Locations, Sessions, CDRs, Tariffs, Tokens, Commands, ChargingProfiles,
// HubClientInfo, Credentials).
pub use crate::version::{
    Endpoint, InterfaceRole, ModuleID, Version, VersionDetails, VersionNumber,
};

// ── Functional + configuration module types ───────────────────────────────────
//
// Wire-identical to 2.2.1 → plain re-exports. The delta types flagged in the
// module docs are still aliases here; #153 replaces those specific lines with
// `v2_2`-local overrides.
pub use crate::v2_2_1::{
    ActiveChargingProfile, ActiveChargingProfileResult, AdditionalGeoLocation, AllowedType,
    AuthMethod, AuthorizationInfo, CancelReservation, Capability, Cdr, CdrDimension,
    CdrDimensionType, CdrLocation, CdrToken, ChargingPeriod, ChargingPreferences,
    ChargingPreferencesResponse, ChargingProfile, ChargingProfilePeriod, ChargingProfileResponse,
    ChargingProfileResponseType, ChargingProfileResult, ChargingProfileResultType,
    ChargingRateUnit, ClearProfileResult, ClientInfo, CommandResponse, CommandResponseType,
    CommandResult, CommandResultType, CommandType, ConnectionStatus, Connector, ConnectorFormat,
    ConnectorType, Credentials, CredentialsRole, DayOfWeek, EnergyContract, Evse,
    ExceptionalPeriod, Facility, Hours, ImageCategory, Location, LocationReferences,
    ParkingRestriction, ParkingType, PowerType, PriceComponent, ProfileType, PublishTokenType,
    RegularHours, ReservationRestrictionType, ReserveNow, Session, SessionStatus,
    SetChargingProfile, SignedData, SignedValue, StartSession, Status, StatusSchedule, StopSession,
    Tariff, TariffDimensionType, TariffElement, TariffRestrictions, TariffType, Token, TokenType,
    UnlockConnector, WhitelistType,
};

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{Endpoint, InterfaceRole, ModuleID, Version, VersionDetails, VersionNumber};
    use crate::common::Url;

    #[test]
    fn version_number_v2_2_serializes_as_dotted_string() {
        assert_eq!(
            serde_json::to_string(&VersionNumber::V2_2).unwrap(),
            "\"2.2\""
        );
        assert_eq!("2.2".parse::<VersionNumber>().unwrap(), VersionNumber::V2_2);
    }

    #[test]
    fn endpoint_2_2_carries_role_on_the_wire() {
        // Contrast with `v2_1_1::Endpoint`, which is role-less: the 2.2 endpoint
        // MUST serialize a `role` key (the Sender/Receiver split arrived in 2.2).
        let ep = Endpoint {
            identifier: ModuleID::Locations,
            role: InterfaceRole::Sender,
            url: Url::try_from("https://example.com/ocpi/cpo/2.2/locations").unwrap(),
        };
        let json = serde_json::to_string(&ep).unwrap();
        assert!(
            json.contains("\"role\""),
            "2.2 endpoint must carry role: {json}"
        );
        assert!(json.contains("\"SENDER\""));
        let back: Endpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ep);

        // Proof of the delta from 2.1.1: the role-less 2.1.1 endpoint omits it.
        let legacy = crate::v2_1_1::Endpoint {
            identifier: ModuleID::Locations,
            url: Url::try_from("https://example.com/ocpi/cpo/2.1.1/locations").unwrap(),
        };
        assert!(!serde_json::to_string(&legacy).unwrap().contains("role"));
    }

    #[test]
    fn version_details_2_2_round_trips_with_roles() {
        // A 2.2 version-details catalogue: role-bearing endpoints, version "2.2".
        // Spec: specs/ocpi/2.2/version_information_endpoint.asciidoc (Endpoint
        // class — `role` field).
        let json = r#"{
            "version": "2.2",
            "endpoints": [
                {
                    "identifier": "credentials",
                    "role": "SENDER",
                    "url": "https://example.com/ocpi/2.2/credentials"
                },
                {
                    "identifier": "locations",
                    "role": "SENDER",
                    "url": "https://example.com/ocpi/cpo/2.2/locations"
                },
                {
                    "identifier": "tokens",
                    "role": "RECEIVER",
                    "url": "https://example.com/ocpi/cpo/2.2/tokens"
                }
            ]
        }"#;
        let details: VersionDetails = serde_json::from_str(json).unwrap();
        assert_eq!(details.version, VersionNumber::V2_2);
        assert_eq!(details.endpoints.len(), 3);
        assert_eq!(details.endpoints[0].role, InterfaceRole::Sender);
        assert_eq!(details.endpoints[2].role, InterfaceRole::Receiver);

        // Round-trip and prove `role` survives serialization.
        let out = serde_json::to_string(&details).unwrap();
        assert!(
            out.contains("\"role\""),
            "serialized 2.2 details must keep role"
        );
        let back: VersionDetails = serde_json::from_str(&out).unwrap();
        assert_eq!(back, details);
    }

    #[test]
    fn version_entry_2_2_round_trips() {
        let v = Version {
            version: VersionNumber::V2_2,
            url: Url::try_from("https://example.com/ocpi/cpo/2.2").unwrap(),
        };
        let back: Version = serde_json::from_str(&serde_json::to_string(&v).unwrap()).unwrap();
        assert_eq!(back, v);
        assert_eq!(back.version.as_str(), "2.2");
    }

    #[test]
    fn module_id_2_2_set_is_complete() {
        // The full OCPI 2.2 module set — unlike 2.1.1, it includes
        // ChargingProfiles and HubClientInfo. Assert every identifier maps to
        // its canonical wire string (no `versions` variant: the versions
        // endpoint is not a module identifier).
        let cases: &[(ModuleID, &str)] = &[
            (ModuleID::Cdrs, "\"cdrs\""),
            (ModuleID::ChargingProfiles, "\"chargingprofiles\""),
            (ModuleID::Commands, "\"commands\""),
            (ModuleID::Credentials, "\"credentials\""),
            (ModuleID::HubClientInfo, "\"hubclientinfo\""),
            (ModuleID::Locations, "\"locations\""),
            (ModuleID::Sessions, "\"sessions\""),
            (ModuleID::Tariffs, "\"tariffs\""),
            (ModuleID::Tokens, "\"tokens\""),
        ];
        for (id, expected) in cases {
            assert_eq!(&serde_json::to_string(id).unwrap(), expected, "{id:?}");
            let back: ModuleID = serde_json::from_str(expected).unwrap();
            assert_eq!(back, *id);
        }
    }

    #[test]
    fn delta_types_are_aliases_of_2_2_1_until_overridden() {
        // The seven known 2.2-vs-2.2.1 wire deltas (#153). Until an override
        // lands, `v2_2::X` must be the *very same type* as `v2_2_1::X`. Each
        // identity closure only compiles if the two paths name one type — a
        // zero-cost, compile-time alias assertion that will start failing the
        // moment #153 introduces a genuine local override (the reminder to drop
        // the corresponding line from this test).
        let _: fn(crate::v2_2_1::CdrToken) -> super::CdrToken = |x| x;
        let _: fn(crate::v2_2_1::Cdr) -> super::Cdr = |x| x;
        let _: fn(crate::v2_2_1::CdrLocation) -> super::CdrLocation = |x| x;
        let _: fn(crate::v2_2_1::SignedData) -> super::SignedData = |x| x;
        let _: fn(crate::v2_2_1::StartSession) -> super::StartSession = |x| x;
        let _: fn(crate::v2_2_1::PowerType) -> super::PowerType = |x| x;
        let _: fn(crate::v2_2_1::ConnectorType) -> super::ConnectorType = |x| x;
    }
}

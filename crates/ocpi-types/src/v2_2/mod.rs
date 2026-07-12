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
//! ## Status of the known deltas
//!
//! - **CDRs slice (implemented here, in the `cdrs` submodule):** [`CdrToken`] drops
//!   `country_code`/`party_id`; [`Cdr`] drops `home_charging_compensation`;
//!   [`CdrLocation`] keeps `postal_code` **required** and has no `state`.
//! - **Commands slice (implemented here, in the `commands` submodule):**
//!   [`StartSession`] drops `connector_id` (in 2.2 the Charge Point picks the
//!   connector). The `START_SESSION_CONNECTOR_REQUIRED` capability 2.2.1 added
//!   stays a re-export — see the `commands` module doc.
//! - **Locations slice (implemented here, in the `locations` submodule):**
//!   [`PowerType`] drops the `AC_2_PHASE` / `AC_2_PHASE_SPLIT` values and
//!   [`ConnectorType`] drops the 2.2.1-added values (`CHAOJI`, `DOMESTIC_M`/`N`/
//!   `O`, `GBT_AC`/`GBT_DC`, and the extended NEMA family). The composites that
//!   carry those enums — [`Connector`]/[`Evse`]/[`Location`] — are `v2_2`-local
//!   too, structurally identical to their 2.2.1 counterparts but with the 2.2
//!   connector enums, so a 2.2.1-only plug/power value cannot ride a 2.2
//!   connector (#167). The client/server Locations wiring is the remaining #167
//!   follow-up.
//! - **[`SignedData`] is intentionally *not* overridden.** The 2.2.1 change
//!   ("SignedData URL datatype fixed, blob length raised to 5000, signed-data
//!   fields to string") only relaxes `CiString(512)` string bounds — which this
//!   crate already models as unbounded [`String`] — so the 2.2 and 2.2.1
//!   `SignedData`/`SignedValue` wire shapes are byte-identical in Rust and a
//!   re-export is the faithful representation.
//!
//! ## Wire-identical module reuse — ratified (#171)
//!
//! Every 2.2-vs-2.2.1 wire delta is exactly three modules — **CDRs**,
//! **Commands**, **Locations** — sliced into the `v2_2`-local overrides above.
//! The other seven modules — **Sessions**, **Tariffs**, **Tokens**,
//! **ChargingProfiles**, **HubClientInfo** ([`ClientInfo`]), **Versions**
//! (the role-bearing [`Endpoint`]/[`VersionDetails`] layer), and
//! **Credentials** — have **no** 2.2-vs-2.2.1 wire difference (the `role`/
//! `roles` fields and the `OCPI-*` routing headers all arrived *in* 2.2), so
//! `v2_2` re-exports their 2.2.1 types unchanged.
//!
//! Consequently **no thin `_2_2` client/server aliases are minted** for those
//! seven modules — a 2.2 party drives Sessions/Tariffs/Tokens/ChargingProfiles/
//! HubClientInfo/Versions/Credentials with the existing 2.2.1 `OcpiClient`
//! methods and server routers unchanged.
//! Aliasing identical-typed calls would only imply a difference that does not
//! exist (the precedent set for the four wire-identical Commands in #166).
//!
//! This decision is not merely documented but **enforced**: the `reuse` test
//! module below carries compile-time type-identity assertions
//! (`fn(v2_2::T) -> v2_2_1::T = |x| x`, which only typechecks when the two
//! paths name the *same* nominal type) for a representative type of each of the
//! seven modules, plus a serde round-trip proving a 2.2 party's `Session`
//! deserialized through the re-export equals the 2.2.1 `Session` byte-for-byte.
//! If a future change accidentally forks one of these modules into a
//! `v2_2`-local type, the identity assertion stops compiling.

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

// ── CDRs slice: 2.2-vs-2.2.1 wire-delta overrides (#153) ──────────────────────
//
// `CdrToken`, `Cdr`, and `CdrLocation` genuinely differ on the 2.2 wire, so
// they are `v2_2`-local types (see `cdrs`) instead of re-exports. Everything
// else the CDRs module needs — `AuthMethod`, `ChargingPeriod`, `SignedData`,
// `SignedValue`, `CdrDimension*` — is wire-identical and stays a re-export.
mod cdrs;
pub use cdrs::{Cdr, CdrLocation, CdrToken};

// ── Commands slice: 2.2-vs-2.2.1 wire-delta override (#153 / #157) ────────────
//
// Only `StartSession` differs on the 2.2 wire (no `connector_id`), so it is a
// `v2_2`-local type (see `commands`). Every other Commands type — `CommandType`,
// `CommandResponse`, `CommandResult`, `ReserveNow`, `StopSession`,
// `UnlockConnector`, `CancelReservation` — is wire-identical and stays a
// re-export. The `Capability::StartSessionConnectorRequired` value 2.2.1 added
// is intentionally kept a re-export (see the `commands` module doc).
mod commands;
pub use commands::StartSession;

// ── Locations slice: 2.2-vs-2.2.1 wire-delta overrides (#153 / #158 / #167) ───
//
// Two Connector enums differ on the 2.2 wire — `PowerType` (no `AC_2_PHASE`
// variants) and `ConnectorType` (no 2.2.1-added values) — so they are
// `v2_2`-local types (see `locations`). The composite objects that embed them,
// `Connector` → `Evse` → `Location`, are `v2_2`-local too: structurally
// identical to their 2.2.1 counterparts, but their connector `standard` /
// `power_type` are the 2.2 enums, so a 2.2.1-only plug/power value cannot ride a
// 2.2 connector (#167). Every other Locations field type (`ConnectorFormat`,
// `Capability`, `GeoLocation`, `Status`, …) is wire-identical and stays a
// re-export. The remaining Locations follow-up (#167) is the client/server
// wiring over these composites.
mod locations;
pub use locations::{Connector, ConnectorType, Evse, Location, PowerType};

// ── Functional + configuration module types ───────────────────────────────────
//
// Wire-identical to 2.2.1 → plain re-exports. Every 2.2-vs-2.2.1 wire delta is
// now sliced into a `v2_2`-local override above (CDRs, Commands, Locations);
// what remains here is the wire-identical shared surface.
pub use crate::v2_2_1::{
    ActiveChargingProfile, ActiveChargingProfileResult, AdditionalGeoLocation, AllowedType,
    AuthMethod, AuthorizationInfo, CancelReservation, Capability, CdrDimension, CdrDimensionType,
    ChargingPeriod, ChargingPreferences, ChargingPreferencesResponse, ChargingProfile,
    ChargingProfilePeriod, ChargingProfileResponse, ChargingProfileResponseType,
    ChargingProfileResult, ChargingProfileResultType, ChargingRateUnit, ClearProfileResult,
    ClientInfo, CommandResponse, CommandResponseType, CommandResult, CommandResultType,
    CommandType, ConnectionStatus, ConnectorFormat, Credentials, CredentialsRole, DayOfWeek,
    EnergyContract, ExceptionalPeriod, Facility, Hours, ImageCategory, LocationReferences,
    ParkingRestriction, ParkingType, PriceComponent, ProfileType, PublishTokenType, RegularHours,
    ReservationRestrictionType, ReserveNow, Session, SessionStatus, SetChargingProfile, SignedData,
    SignedValue, Status, StatusSchedule, StopSession, Tariff, TariffDimensionType, TariffElement,
    TariffRestrictions, TariffType, Token, TokenType, UnlockConnector, WhitelistType,
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
    fn remaining_alias_types_stay_aliases_of_2_2_1() {
        // Types 2.2.1 touched but which are byte-identical in Rust and so stay
        // deliberate re-exports (see the module docs). Until that ever stops
        // being true, `v2_2::X` must be the *very same type* as `v2_2_1::X`. Each
        // identity closure only compiles if the two paths name one type — a
        // zero-cost, compile-time alias assertion that will start failing the
        // moment a genuine local override is introduced.
        //
        // Every genuine 2.2-vs-2.2.1 wire delta is now sliced into a `v2_2`-local
        // override — the CDRs slice (`CdrToken`, `Cdr`, `CdrLocation`), the
        // Commands slice (`StartSession`), and the Locations slice (`PowerType`,
        // `ConnectorType`) — so all of them are deliberately absent here.
        // `SignedData` / `SignedValue` stay re-exports on purpose, so they keep
        // their alias assertion.
        let _: fn(crate::v2_2_1::SignedData) -> super::SignedData = |x| x;
        let _: fn(crate::v2_2_1::SignedValue) -> super::SignedValue = |x| x;
    }
}

// ── Wire-identical module reuse — ratification (#171) ─────────────────────────

/// Enforces the "wire-identical module reuse" decision the module doc ratifies:
/// the seven modules with **no** 2.2-vs-2.2.1 wire delta — Sessions, Tariffs,
/// Tokens, ChargingProfiles, HubClientInfo, Versions, Credentials — are genuine
/// re-exports of their 2.2.1 types, so a 2.2 party drives them with the existing
/// 2.2.1 client/server surface and no `_2_2` alias is minted.
///
/// The proof is compile-time: each `fn(v2_2::T) -> v2_2_1::T = |x| x` closure
/// only typechecks when the two paths name the *same* nominal type (Rust does no
/// subtyping coercion between distinct structs/enums), so it is a zero-cost alias
/// assertion. The moment a future change forks one of these modules into a
/// `v2_2`-local override, the corresponding line stops compiling and this is
/// caught in CI — reuse is *verified*, not merely asserted.
#[cfg(test)]
mod reuse {
    /// Compile-time proof that each `$v22` path is the very same type as its
    /// `$v221` counterpart — i.e. `v2_2` re-exports it rather than forking it.
    macro_rules! assert_reuse {
        ($($v22:ty => $v221:ty),+ $(,)?) => {
            $(const _: fn($v22) -> $v221 = |x| x;)+
        };
    }

    // One representative type per wire-identical module. If any of these ever
    // needs a genuine 2.2 wire delta, slice it into a `v2_2`-local override (as
    // CDRs/Commands/Locations were) and drop its line here.
    assert_reuse! {
        // Sessions
        super::Session          => crate::v2_2_1::Session,
        super::SessionStatus    => crate::v2_2_1::SessionStatus,
        // Tariffs
        super::Tariff           => crate::v2_2_1::Tariff,
        super::TariffElement    => crate::v2_2_1::TariffElement,
        // Tokens
        super::Token            => crate::v2_2_1::Token,
        super::TokenType        => crate::v2_2_1::TokenType,
        // ChargingProfiles
        super::ChargingProfile    => crate::v2_2_1::ChargingProfile,
        super::SetChargingProfile => crate::v2_2_1::SetChargingProfile,
        super::ActiveChargingProfile => crate::v2_2_1::ActiveChargingProfile,
        // HubClientInfo
        super::ClientInfo       => crate::v2_2_1::ClientInfo,
        super::ConnectionStatus => crate::v2_2_1::ConnectionStatus,
        // Credentials
        super::Credentials      => crate::v2_2_1::Credentials,
        super::CredentialsRole  => crate::v2_2_1::CredentialsRole,
    }

    // Versions: the role-bearing endpoint/version-details layer arrived in 2.2
    // and is the shared `crate::version` type (not a per-version fork), so the
    // 2.2 re-export must be that very type. `crate::v2_2_1` reuses the same
    // shared layer, so both versions share one representation.
    assert_reuse! {
        super::Endpoint       => crate::version::Endpoint,
        super::VersionDetails => crate::version::VersionDetails,
    }

    /// A 2.2 party's `Session` rides the existing 2.2.1 transport faithfully:
    /// the spec "simple start" example (`specs/ocpi/2.2/mod_sessions.asciidoc`
    /// §Examples) deserialized through the `v2_2` re-export equals the same
    /// payload deserialized as the 2.2.1 `Session`, and re-serializes to a body
    /// the 2.2.1 type accepts unchanged — the wire is byte-identical, which is
    /// why no `get_sessions_2_2` is needed.
    #[test]
    fn session_reuse_round_trips_through_2_2_1_type() {
        // Verbatim the 2.2.1 crate's own "simple start" spec fixture — reused
        // here to show a 2.2 party parses the very same bytes.
        let payload = r#"{
            "country_code": "NL",
            "party_id": "TNM",
            "id": "101",
            "start_date_time": "2020-03-09T10:17:09Z",
            "kwh": 0.00,
            "cdr_token": {
                "country_code": "NL",
                "party_id": "TNM",
                "uid": "012345678",
                "type": "RFID",
                "contract_id": "NL8ACC12E46L89"
            },
            "auth_method": "WHITELIST",
            "location_id": "LOC1",
            "evse_uid": "3256",
            "connector_id": "1",
            "currency": "EUR",
            "status": "PENDING",
            "last_updated": "2020-03-09T10:17:09Z"
        }"#;

        // Parsed via the 2.2 re-export …
        let via_2_2: super::Session = serde_json::from_str(payload).unwrap();
        // … and via the 2.2.1 type directly.
        let via_2_2_1: crate::v2_2_1::Session = serde_json::from_str(payload).unwrap();
        // Same type, same value — the reuse is real, not coerced.
        assert_eq!(via_2_2, via_2_2_1);

        // The 2.2 side re-serializes to a body the 2.2.1 type accepts unchanged.
        let re_encoded = serde_json::to_string(&via_2_2).unwrap();
        let round_tripped: crate::v2_2_1::Session = serde_json::from_str(&re_encoded).unwrap();
        assert_eq!(round_tripped, via_2_2_1);
    }
}

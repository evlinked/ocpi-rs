//! # ocpi-types
//!
//! Typed, `serde`-serializable models for the **Open Charge Point Interface
//! (OCPI)** — the EV charging roaming protocol between Charge Point Operators
//! (CPOs) and e-Mobility Service Providers (eMSPs).
//!
//! This crate is the shared data contract used by `ocpi-client` and
//! `ocpi-server`. It is deliberately transport-agnostic: it models the wire
//! types, not the HTTP plumbing.
//!
//! ## Layout
//!
//! - [`envelope`] — the OCPI response envelope (`status_code`, `timestamp`, …).
//! - [`status`] — the canonical OCPI [`OcpiStatusCode`] set.
//! - [`common`] — common data types shared across modules.
//! - [`version`] — version negotiation primitives.
//! - `v2_1_1` / `v2_2_1` / `v2_3_0` — version-namespaced module models,
//!   populated incrementally per the roadmap milestones.
//!
//! ## Layout (continued)
//!
//! - [`transport`] — HTTP transport conventions: token auth, routing headers,
//!   pagination params and response metadata.
//!
//! ## Design philosophy
//!
//! Defer *logic*, not *schema*: types are forward-compatible from day one.
//! Reject the unsupported case explicitly (a distinct [`OcpiStatusCode`])
//! rather than silently dropping data, and keep field semantics aligned with
//! the governing OCPI specification.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod common;
pub mod envelope;
pub mod error;
pub mod status;
pub mod transport;
pub mod version;

pub mod v2_1_1;
pub mod v2_2_1;
pub mod v2_3_0;

pub use common::{
    CiString, CiString2, CiString255, CiString3, CiString36, CiString39, CiString48, CiString64,
    DisplayText, EnergyMix, EnergySource, EnergySourceCategory, EnvironmentalImpact,
    EnvironmentalImpactCategory, GeoLocation, Price, Role, Url,
};
pub use envelope::{OcpiPaged, OcpiResponse};
pub use error::OcpiError;
pub use status::OcpiStatusCode;
pub use v2_2_1::{
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
pub use version::{Endpoint, InterfaceRole, ModuleID, Version, VersionDetails, VersionNumber};

// Re-export common third-party types so downstream crates can use them
// without declaring direct dependencies on these packages.
pub use chrono::{self, DateTime, Utc};
pub use serde;
pub use serde_json;

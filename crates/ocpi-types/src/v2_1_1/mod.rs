//! OCPI **2.1.1** typed models.
//!
//! Modules: Versions, Credentials, Locations, Sessions, CDRs, Tariffs, Tokens,
//! Commands.
//!
//! Populated incrementally — see milestone **M7** in the roadmap. Shared
//! primitives live in [`crate::common`], [`crate::envelope`], and
//! [`crate::version`].
//!
//! ## Layout
//!
//! Each OCPI module lives in its own submodule file (`versions`,
//! `credentials`, `tokens`, `tariffs`, …), and every type is re-exported
//! at the [`crate::v2_1_1`] root so downstream imports stay flat
//! (`use ocpi_types::v2_1_1::Tariff;`). The per-module file layout lets the
//! remaining 2.1.1 modules be authored and merged in parallel with zero
//! shared-file conflicts — only the small `mod`/`pub use` block in this file is
//! touched, and only as clean one-line additions.
//!
//! ## Version-negotiation delta from 2.2.1
//!
//! The Sender/Receiver `InterfaceRole` split was introduced in OCPI **2.2**.
//! In 2.1.1 a version-details endpoint entry is therefore just
//! `{ identifier, url }` with **no `role` field**. The shared
//! [`crate::version::Endpoint`] carries a required `role`, so it cannot
//! represent a 2.1.1 endpoint faithfully. Rather than weaken the 2.2.1 type
//! by making `role` optional, this module defines a distinct 2.1.1-shaped
//! [`Endpoint`] and [`VersionDetails`]. [`crate::version::VersionNumber`] and
//! [`crate::version::ModuleID`] are version-agnostic and reused as-is.
//!
//! The 2.1.1 module set is **Locations, Sessions, CDRs, Tariffs, Tokens,
//! Commands, Credentials** — there is **no** `HubClientInfo` and **no**
//! `ChargingProfiles` (both arrived in 2.2). The reused
//! [`crate::version::ModuleID`] enum is a superset; constructing a 2.1.1
//! [`Endpoint`] with one of the 2.2-only identifiers is possible at the type
//! level but out of spec — see [`Endpoint::is_valid_2_1_1`].

mod cdrs;
mod commands;
mod credentials;
mod locations;
mod sessions;
mod tariffs;
mod tokens;
mod versions;

pub use cdrs::Cdr;
pub use commands::{
    CommandResponse, CommandResponseType, CommandType, ReserveNow, StartSession, StopSession,
    UnlockConnector,
};
pub use credentials::Credentials;
pub use locations::{
    AdditionalGeoLocation, Capability, Connector, ConnectorFormat, ConnectorType, Evse,
    ExceptionalPeriod, Facility, Hours, Location, LocationType, ParkingRestriction, PowerType,
    RegularHours, Status, StatusSchedule,
};
pub use sessions::{
    AuthMethod, CdrDimension, CdrDimensionType, ChargingPeriod, Session, SessionStatus,
};
pub use tariffs::{PriceComponent, Tariff, TariffDimensionType, TariffElement, TariffRestrictions};
pub use tokens::{Token, TokenType};
pub use versions::{Endpoint, VersionDetails};

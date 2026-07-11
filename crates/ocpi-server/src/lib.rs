//! # ocpi-server
//!
//! Server-side building blocks for the **receiver** role — the side that
//! exposes OCPI endpoints and is called by remote parties.
//!
//! The core is framework-agnostic: you implement handler traits such as
//! [`VersionsHandler`] or [`CredentialsHandler`]. Enable the `axum` feature
//! for ready-made routers (see the `http` module).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use ocpi_types::{
    v2_2_1::{
        ActiveChargingProfile, ActiveChargingProfileResult, AuthorizationInfo, CancelReservation,
        Cdr, ChargingPreferences, ChargingPreferencesResponse, ChargingProfileResponse,
        ChargingProfileResponseType, ChargingProfileResult, ClearProfileResult, ClientInfo,
        CommandResponse, CommandResponseType, CommandResult, CommandType, Connector, Credentials,
        Evse, Location, LocationReferences, ProfileType, ReserveNow, Session, SetChargingProfile,
        StartSession, StopSession, Tariff, Token, TokenType, UnlockConnector,
    },
    version::{Endpoint, Version, VersionDetails, VersionNumber},
    DateTime, OcpiStatusCode, Utc,
};
// The role-less (pre-2.2) version-details shape, distinct from the role-bearing
// [`ocpi_types::version::VersionDetails`]. Aliased to keep both names usable
// side by side without shadowing.
use ocpi_types::v2_1_1::VersionDetails as LegacyVersionDetails;
// The flat OCPI 2.1.1 credentials object (no `roles` array), aliased to keep it
// distinct from the role-bearing 2.2.1 `Credentials` imported above.
use ocpi_types::v2_1_1::Credentials as Credentials2111;
// The role-less OCPI 2.1.1 version-details endpoint (no `role` field), aliased
// to keep it distinct from the role-bearing 2.2.1 `Endpoint` imported above.
use ocpi_types::v2_1_1::Endpoint as Endpoint2111;
// The OCPI 2.1.1 `Tariff` (no `country_code`/`party_id`/`type`/min-max price),
// aliased to keep it distinct from the root-exported 2.2.1 `Tariff` above.
use ocpi_types::v2_1_1::Tariff as Tariff2111;
// The OCPI 2.1.1 Session object (auth_id, embedded location, one-word
// start/end timestamps, no charging-preferences), aliased to keep it distinct
// from the role-bearing 2.2.1 `Session` imported above. See
// [`Sessions2111Config`].
use ocpi_types::v2_1_1::Session as Session2111;
// The OCPI 2.1.1 CDR object (bare `auth_id`, embedded `location`,
// `stop_date_time`, single numeric `total_cost`, no `session_id`), aliased to
// keep it distinct from the role-bearing 2.2.1 `Cdr` imported above. See
// [`Cdrs2111Config`].
use ocpi_types::v2_1_1::Cdr as Cdr2111;
// The OCPI 2.2 CDR object — a `CdrToken` with no `country_code`/`party_id`, a
// `Cdr` with no `home_charging_compensation`, a `CdrLocation` with a required
// `postal_code` and no `state` — aliased to keep it distinct from the
// role-bearing 2.2.1 `Cdr` imported above. See [`Cdrs22Config`].
use ocpi_types::v2_2::Cdr as Cdr22;
// The OCPI 2.1.1 Tokens surface — `Token` keys on `auth_id`, `TokenType` covers
// only `OTHER`/`RFID`, and `AuthorizationInfo` omits `token`/
// `authorization_reference` — aliased to keep the 2.2.1 names above unqualified.
// See [`Tokens2111Config`].
use ocpi_types::v2_1_1::{
    AuthorizationInfo as AuthorizationInfo2111, LocationReferences as LocationReferences2111,
    Token as Token2111, TokenType as TokenType2111,
};
// The OCPI 2.1.1 Locations objects (required `type`, no `country_code`/`party_id`
// on the object, singular `Connector.tariff_id`), aliased to keep them distinct
// from the root-exported 2.2.1 `Location`/`Evse`/`Connector`. The receiver
// transport is identical to 2.2.1 — Locations is a client-owned object push, so
// the path keeps the `{country_code}/{party_id}/{location_id}` segments. See
// [`Locations2111Config`].
use ocpi_types::v2_1_1::{Connector as Connector2111, Evse as Evse2111, Location as Location2111};
// The OCPI 2.1.1 Commands surface — four command types (no `CANCEL_RESERVATION`),
// a full-`Token` `StartSession`, and a single `CommandResponse` (no 2.2 `timeout`/
// `message` fields) used for both the synchronous ack and the async callback —
// aliased to keep the 2.2.1 names above unqualified. See [`Commands2111Config`].
use ocpi_types::v2_1_1::{
    CommandResponse as CommandResponse2111, CommandResponseType as CommandResponseType2111,
    CommandType as CommandType2111, ReserveNow as ReserveNow2111, StartSession as StartSession2111,
    StopSession as StopSession2111, UnlockConnector as UnlockConnector2111,
};
// The OCPI 2.2 Commands surface — wire-identical to 2.2.1 except `StartSession`,
// which drops `connector_id` (added in 2.2.1; in 2.2 the Charge Point picks the
// connector). Only that one type is aliased; every other 2.2 Commands type is the
// re-exported 2.2.1 type. See [`Commands22Config`] / [`http::commands_2_2_router`].
use ocpi_types::v2_2::StartSession as StartSession22;

// ── ServerError ───────────────────────────────────────────────────────────────

/// An error raised while handling an inbound OCPI request.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    /// A wrapped error originating from the type layer.
    #[error(transparent)]
    Ocpi(#[from] ocpi_types::OcpiError),

    /// The caller's token was missing or not recognised.
    #[error("unauthorized")]
    Unauthorized,

    /// The requested operation is not yet implemented.
    #[error("not yet implemented: {0}")]
    NotImplemented(&'static str),

    /// A `POST /credentials` was received from a party that is already
    /// registered. The axum layer should respond with HTTP 405.
    #[error("already registered")]
    AlreadyRegistered,

    /// A `PUT` or `DELETE /credentials` was received from a party that has
    /// not yet registered. The axum layer should respond with HTTP 405.
    #[error("not registered")]
    NotRegistered,

    /// The requested resource was not found (unknown ID in the store).
    ///
    /// Maps to OCPI status code `2003` (Unknown Location).
    #[error("not found")]
    NotFound,

    /// A real-time authorization was requested for a token the eMSP does not know.
    ///
    /// Maps to OCPI status code `2004` (Unknown Token).
    #[error("unknown token")]
    UnknownToken,
}

impl ServerError {
    /// Map this error to the OCPI status code that should be returned in the
    /// response envelope.
    #[must_use]
    pub fn status_code(&self) -> OcpiStatusCode {
        match self {
            Self::Ocpi(ocpi_types::OcpiError::Status(code)) => *code,
            Self::Unauthorized => OcpiStatusCode::ClientError,
            Self::AlreadyRegistered | Self::NotRegistered => OcpiStatusCode::ClientError,
            Self::NotFound => OcpiStatusCode::UnknownLocation,
            Self::UnknownToken => OcpiStatusCode::UnknownToken,
            Self::Ocpi(_) | Self::NotImplemented(_) => OcpiStatusCode::ServerError,
        }
    }
}

// ── VersionsHandler ───────────────────────────────────────────────────────────

/// Handles the OCPI versions / version-details endpoints (receiver role).
///
/// Implementors return the list of supported OCPI versions and the endpoint
/// catalogue for each version.
///
/// The axum integration in [`http::versions_router`] accepts any [`VersionsConfig`]
/// directly. This trait is provided for custom, dynamic, or async-backed
/// implementations.
#[allow(async_fn_in_trait)]
pub trait VersionsHandler {
    /// Return all supported OCPI versions (`GET /versions`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the versions cannot be retrieved.
    async fn list_versions(&self) -> Result<Vec<Version>, ServerError>;

    /// Return the endpoint catalogue for a specific OCPI version
    /// (`GET /versions/{version}`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::Ocpi`] with
    /// [`OcpiStatusCode::UnsupportedVersion`] when the version is not
    /// supported.
    async fn version_details(&self, version: VersionNumber) -> Result<VersionDetails, ServerError>;
}

// ── VersionsConfig ────────────────────────────────────────────────────────────

/// A static in-memory version registry for use with [`http::versions_router`].
///
/// Populate this at server startup with the versions and endpoint URLs your
/// OCPI node exposes.
#[derive(Debug, Clone)]
pub struct VersionsConfig {
    /// Ordered list of supported versions (returned by `GET /versions`).
    pub versions: Vec<Version>,
    /// Role-bearing endpoint catalogues (OCPI 2.2+) keyed by version number.
    pub details: std::collections::HashMap<VersionNumber, VersionDetails>,
    /// Role-less endpoint catalogues (OCPI 2.1.1 and earlier, before the
    /// Sender/Receiver split arrived in 2.2) keyed by version number.
    ///
    /// Kept separate from [`details`](Self::details) because the 2.1.1
    /// version-details endpoints carry **no `role` field**; see
    /// [`ocpi_types::v2_1_1::VersionDetails`]. A version registered here is
    /// advertised in `GET /versions` exactly like a role-bearing one.
    pub legacy_details: std::collections::HashMap<VersionNumber, LegacyVersionDetails>,
}

impl VersionsConfig {
    /// Create an empty registry; add entries with
    /// [`add_version`](Self::add_version) or
    /// [`add_legacy_version`](Self::add_legacy_version).
    #[must_use]
    pub fn new() -> Self {
        Self {
            versions: Vec::new(),
            details: std::collections::HashMap::new(),
            legacy_details: std::collections::HashMap::new(),
        }
    }

    /// Register a role-bearing (OCPI 2.2+) version and its endpoint catalogue.
    pub fn add_version(&mut self, entry: Version, details: VersionDetails) {
        self.versions.push(entry);
        self.details.insert(details.version, details);
    }

    /// Register a role-less (OCPI 2.1.1 / pre-2.2) version and its endpoint
    /// catalogue.
    ///
    /// The version is advertised in `GET /versions` just like a role-bearing
    /// one, but `GET /versions/{version}` serves the role-less
    /// [`ocpi_types::v2_1_1::VersionDetails`] — faithful to the 2.1.1 spec,
    /// whose endpoints have no `role`. This is what lets a node advertise both
    /// 2.2.1 and 2.1.1 and complete a `GET /versions/2.1.1` exchange with a
    /// legacy partner.
    pub fn add_legacy_version(&mut self, entry: Version, details: LegacyVersionDetails) {
        self.versions.push(entry);
        self.legacy_details.insert(details.version, details);
    }
}

impl Default for VersionsConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(async_fn_in_trait)]
impl VersionsHandler for VersionsConfig {
    async fn list_versions(&self) -> Result<Vec<Version>, ServerError> {
        Ok(self.versions.clone())
    }

    async fn version_details(&self, version: VersionNumber) -> Result<VersionDetails, ServerError> {
        self.details
            .get(&version)
            .cloned()
            .ok_or(ServerError::Ocpi(ocpi_types::OcpiError::Status(
                OcpiStatusCode::UnsupportedVersion,
            )))
    }
}

// ── CredentialsHandler ────────────────────────────────────────────────────────

/// Handles the OCPI credentials / registration handshake (receiver role).
///
/// All four spec methods are required. Implementors are responsible for:
/// - Persisting/revoking credentials tokens.
/// - Returning [`ServerError::AlreadyRegistered`] from `register` when the
///   caller is already known (the axum layer turns this into HTTP 405).
/// - Returning [`ServerError::NotRegistered`] from `update_credentials` and
///   `delete_credentials` when the caller is not yet registered (HTTP 405).
/// - Calling [`Credentials::check_single_role`] if multi-role is not yet
///   supported, and returning [`ServerError::Ocpi`] wrapping
///   [`OcpiStatusCode::ServerError`].
///
/// Spec: `specs/ocpi/2.2.1/credentials.asciidoc`
#[allow(async_fn_in_trait)]
pub trait CredentialsHandler {
    /// Return this server's own [`Credentials`] (`GET /credentials`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::Unauthorized`] when `token` is not recognised.
    async fn get_credentials(&self, token: &str) -> Result<Credentials, ServerError>;

    /// Register a new party and return the server's credentials for them
    /// (`POST /credentials`).
    ///
    /// # Errors
    ///
    /// - [`ServerError::Unauthorized`] — `token` not recognised.
    /// - [`ServerError::AlreadyRegistered`] — caller already registered (→ HTTP 405).
    async fn register(
        &self,
        token: &str,
        credentials: Credentials,
    ) -> Result<Credentials, ServerError>;

    /// Update an existing registration and return the refreshed server
    /// credentials (`PUT /credentials`).
    ///
    /// # Errors
    ///
    /// - [`ServerError::Unauthorized`] — `token` not recognised.
    /// - [`ServerError::NotRegistered`] — caller not yet registered (→ HTTP 405).
    async fn update_credentials(
        &self,
        token: &str,
        credentials: Credentials,
    ) -> Result<Credentials, ServerError>;

    /// Revoke a registration (`DELETE /credentials`).
    ///
    /// # Errors
    ///
    /// - [`ServerError::Unauthorized`] — `token` not recognised.
    /// - [`ServerError::NotRegistered`] — caller not yet registered (→ HTTP 405).
    async fn delete_credentials(&self, token: &str) -> Result<(), ServerError>;
}

// ── VersionFetcher ──────────────────────────────────────────────────────────

/// An error raised while fetching a registering party's version catalogue
/// during the credentials handshake (the "fetch-back" step).
///
/// Any variant maps to OCPI status code `3001`
/// ([`OcpiStatusCode::UnableToUseClientApi`]) in the `POST`/`PUT /credentials`
/// response — the server could not use the client's API.
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    /// The HTTP request to the client's `/versions` or version-details
    /// endpoint failed (connection error, non-2xx status, …).
    #[error("transport error: {0}")]
    Transport(String),

    /// The client and server share no mutually-supported OCPI version.
    #[error("no mutually-supported version")]
    NoMutualVersion,

    /// The response body could not be parsed as the expected OCPI object.
    #[error("invalid response: {0}")]
    Invalid(String),
}

/// The future type returned by [`VersionFetcher`] methods.
///
/// Boxed and `Send` so it can be awaited inside an axum handler (whose future
/// must be `Send`). Using a boxed future — rather than `async fn` in the trait
/// — keeps the trait object-safe (`dyn VersionFetcher`) and side-steps the
/// `async_fn_in_trait` / axum `Send`-bound incompatibility.
pub type FetchFuture<'a, T> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, FetchError>> + Send + 'a>>;

/// Fetches a remote party's version catalogue during the registration
/// handshake.
///
/// `ocpi-server` must not depend on an HTTP client (that would risk a cyclic
/// dependency with `ocpi-client`), so the fetch-back is expressed as this
/// contract. The host application supplies an implementation — typically one
/// backed by `ocpi-client`'s `reqwest` transport — and passes it to
/// [`CredentialsConfig::new_with_fetcher`].
///
/// Spec: `specs/ocpi/2.2.1/credentials.asciidoc` — §POST Method (the receiver
/// fetches the sender's endpoints for the registered version).
pub trait VersionFetcher: Send + Sync {
    /// `GET {url}` — retrieve the remote party's `/versions` list, presenting
    /// `token` as the OCPI `Authorization` credential.
    ///
    /// `url` is the `url` field of the [`Credentials`] object the party sent.
    fn fetch_versions<'a>(&'a self, url: &'a str, token: &'a str) -> FetchFuture<'a, Vec<Version>>;

    /// `GET {url}` — retrieve the endpoint catalogue for a single version.
    ///
    /// `url` is the [`Version::url`] selected from the `/versions` list.
    fn fetch_version_details<'a>(
        &'a self,
        url: &'a str,
        token: &'a str,
    ) -> FetchFuture<'a, VersionDetails>;
}

/// Fetches a remote party's **role-less OCPI 2.1.1** version catalogue during
/// the registration handshake — the 2.1.1 counterpart to [`VersionFetcher`].
///
/// A faithful 2.1.1 partner emits version details whose endpoints carry **no
/// `role`** ([`ocpi_types::v2_1_1::Endpoint`]). Reusing [`VersionFetcher`],
/// whose `fetch_version_details` returns the role-bearing
/// [`ocpi_types::version::VersionDetails`], would fail to deserialize such a
/// response. The `/versions` list itself is identical across versions (a
/// role-less list of [`Version`]), so only `fetch_version_details` diverges.
///
/// Like [`VersionFetcher`], the implementation lives in `ocpi-client` (the
/// `ocpi-server` crate must not depend on an HTTP client); pass it to
/// [`Credentials2111Config::new_with_fetcher`].
///
/// Spec: OCPI 2.1.1 — *Credentials* / *Registration* (the receiver fetches the
/// sender's `/versions` + version details after a `POST`/`PUT /credentials`).
pub trait LegacyVersionFetcher: Send + Sync {
    /// `GET {url}` — retrieve the remote party's `/versions` list, presenting
    /// `token` as the OCPI `Authorization` credential.
    fn fetch_versions<'a>(&'a self, url: &'a str, token: &'a str) -> FetchFuture<'a, Vec<Version>>;

    /// `GET {url}` — retrieve the **role-less** endpoint catalogue for a single
    /// version.
    fn fetch_version_details<'a>(
        &'a self,
        url: &'a str,
        token: &'a str,
    ) -> FetchFuture<'a, LegacyVersionDetails>;
}

/// Pick the entry with the highest [`VersionNumber`] that also appears in
/// `supported`, or `None` if there is no overlap. Mirrors the sender-side
/// negotiation in `ocpi-client`.
fn select_best_version<'a>(
    remote: &'a [Version],
    supported: &[VersionNumber],
) -> Option<&'a Version> {
    remote
        .iter()
        .filter(|v| supported.contains(&v.version))
        .max_by_key(|v| v.version)
}

// ── CredentialsConfig ─────────────────────────────────────────────────────────

/// A registered remote party: their [`Credentials`] plus, when the
/// registration fetch-back ran, the endpoint catalogue fetched from their
/// `/versions` details.
#[derive(Debug, Clone)]
pub struct RegisteredParty {
    /// The credentials object the party presented at registration.
    pub credentials: Credentials,
    /// Endpoints fetched from the party's selected version details, or `None`
    /// when no [`VersionFetcher`] was configured (fetch-back skipped).
    pub endpoints: Option<Vec<Endpoint>>,
}

/// An in-memory credentials store for use with [`http::credentials_router`].
///
/// Holds the server's own [`Credentials`] and a token-keyed registry of
/// registered parties. Thread-safe via interior mutability (`RwLock`); wrap
/// in `Arc` to share across axum handlers.
///
/// `CredentialsConfig` intentionally does **not** implement
/// [`CredentialsHandler`] — wiring that trait generically through axum runs
/// into `async_fn_in_trait` / `Send` bound issues. Use this concrete type with
/// [`http::credentials_router`] instead, and keep the trait for custom
/// out-of-process implementations.
pub struct CredentialsConfig {
    /// The credentials this server returns on every successful request.
    pub own_credentials: Credentials,
    registered: std::sync::RwLock<std::collections::HashMap<String, RegisteredParty>>,
    /// OCPI versions this server supports, used to negotiate the fetch-back
    /// version against the registering party's `/versions` list.
    supported_versions: Vec<VersionNumber>,
    /// Optional transport for the registration fetch-back. When `None`, the
    /// fetch-back step is skipped and parties register without an endpoint
    /// catalogue.
    fetcher: Option<std::sync::Arc<dyn VersionFetcher>>,
}

impl std::fmt::Debug for CredentialsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CredentialsConfig")
            .field("own_credentials", &self.own_credentials)
            .field(
                "registered_count",
                &self.registered.read().map(|m| m.len()).unwrap_or(0),
            )
            .field("supported_versions", &self.supported_versions)
            .field("fetch_back", &self.fetcher.is_some())
            .finish()
    }
}

impl CredentialsConfig {
    /// Create a new registry with the given server credentials.
    ///
    /// No parties are registered initially and no registration fetch-back is
    /// performed (parties register without an endpoint catalogue). Call
    /// [`register`](Self::register) or let parties register via the axum
    /// router. Use [`new_with_fetcher`](Self::new_with_fetcher) to enable the
    /// fetch-back step.
    #[must_use]
    pub fn new(own_credentials: Credentials) -> Self {
        Self {
            own_credentials,
            registered: std::sync::RwLock::new(std::collections::HashMap::new()),
            supported_versions: vec![VersionNumber::V2_2_1],
            fetcher: None,
        }
    }

    /// Create a registry that performs the OCPI registration fetch-back.
    ///
    /// On `POST`/`PUT /credentials`, the server calls `fetcher` to `GET` the
    /// registering party's `/versions` list, selects the highest version that
    /// is also in `supported_versions`, fetches that version's endpoint
    /// catalogue, and stores it alongside the party's credentials. Any failure
    /// surfaces as OCPI status code `3001`.
    ///
    /// `supported_versions` is the server's own list of supported OCPI
    /// versions (it is the negotiation counterpart to the registering party's
    /// advertised versions).
    #[must_use]
    pub fn new_with_fetcher(
        own_credentials: Credentials,
        supported_versions: Vec<VersionNumber>,
        fetcher: std::sync::Arc<dyn VersionFetcher>,
    ) -> Self {
        Self {
            own_credentials,
            registered: std::sync::RwLock::new(std::collections::HashMap::new()),
            supported_versions,
            fetcher: Some(fetcher),
        }
    }

    /// Returns `true` if `token` belongs to a registered party.
    #[must_use]
    pub fn is_registered(&self, token: &str) -> bool {
        self.registered
            .read()
            .expect("lock not poisoned")
            .contains_key(token)
    }

    /// Register a new party under `token`, storing their [`Credentials`]
    /// without an endpoint catalogue.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::AlreadyRegistered`] if `token` is already known.
    pub fn register(&self, token: &str, credentials: Credentials) -> Result<(), ServerError> {
        self.register_with_endpoints(token, credentials, None)
    }

    /// Register a new party under `token`, storing their [`Credentials`] and
    /// the endpoint catalogue fetched during the registration handshake.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::AlreadyRegistered`] if `token` is already known.
    pub fn register_with_endpoints(
        &self,
        token: &str,
        credentials: Credentials,
        endpoints: Option<Vec<Endpoint>>,
    ) -> Result<(), ServerError> {
        let mut map = self.registered.write().expect("lock not poisoned");
        if map.contains_key(token) {
            return Err(ServerError::AlreadyRegistered);
        }
        map.insert(
            token.to_owned(),
            RegisteredParty {
                credentials,
                endpoints,
            },
        );
        Ok(())
    }

    /// Update the stored credentials for an already-registered party, clearing
    /// any stored endpoint catalogue.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotRegistered`] if `token` is not in the
    /// registry.
    pub fn update(&self, token: &str, credentials: Credentials) -> Result<(), ServerError> {
        self.update_with_endpoints(token, credentials, None)
    }

    /// Update the stored credentials and endpoint catalogue for an
    /// already-registered party.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotRegistered`] if `token` is not in the
    /// registry.
    pub fn update_with_endpoints(
        &self,
        token: &str,
        credentials: Credentials,
        endpoints: Option<Vec<Endpoint>>,
    ) -> Result<(), ServerError> {
        let mut map = self.registered.write().expect("lock not poisoned");
        if !map.contains_key(token) {
            return Err(ServerError::NotRegistered);
        }
        map.insert(
            token.to_owned(),
            RegisteredParty {
                credentials,
                endpoints,
            },
        );
        Ok(())
    }

    /// Return the endpoint catalogue stored for a registered party, if any.
    ///
    /// Returns `None` when the token is unknown or when the party registered
    /// without a fetch-back (no [`VersionFetcher`] configured). The catalogue
    /// is cloned out from behind the lock.
    #[must_use]
    pub fn get_endpoints(&self, token: &str) -> Option<Vec<Endpoint>> {
        self.registered
            .read()
            .expect("lock not poisoned")
            .get(token)
            .and_then(|party| party.endpoints.clone())
    }

    /// Run the registration fetch-back for a registering party.
    ///
    /// Returns `Ok(None)` when no [`VersionFetcher`] is configured (the step is
    /// skipped). Otherwise it `GET`s the party's `/versions` list, selects the
    /// highest mutually-supported version, fetches that version's endpoint
    /// catalogue, and returns it.
    ///
    /// The presented [`Credentials::token`] is used as the OCPI authorization
    /// credential for the outbound calls; [`Credentials::url`] is the party's
    /// `/versions` URL.
    ///
    /// # Errors
    ///
    /// Returns [`FetchError`] on any transport, negotiation, or parse failure.
    /// Callers map this to OCPI status code `3001`.
    pub async fn fetch_back(
        &self,
        credentials: &Credentials,
    ) -> Result<Option<Vec<Endpoint>>, FetchError> {
        let Some(fetcher) = self.fetcher.as_ref() else {
            return Ok(None);
        };
        let url = credentials.url.as_str();
        let token = credentials.token.as_str();
        let remote = fetcher.fetch_versions(url, token).await?;
        let chosen = select_best_version(&remote, &self.supported_versions)
            .ok_or(FetchError::NoMutualVersion)?;
        let details = fetcher
            .fetch_version_details(chosen.url.as_str(), token)
            .await?;
        Ok(Some(details.endpoints))
    }

    /// Invalidate `token`, removing it from the registry if present.
    ///
    /// Unlike [`delete`](Self::delete) this never errors on an unknown token —
    /// it is used to burn the single-use bootstrap *Token A* once registration
    /// completes. Per `specs/ocpi/2.2.1/credentials.asciidoc` §Registration the
    /// Sender switches to *Token C* (the server's
    /// [`own_credentials`](Self::own_credentials) token) for every subsequent
    /// request, and `CREDENTIALS_TOKEN_A` "MAY no longer be used". Returns
    /// `true` if the token was present.
    pub fn invalidate(&self, token: &str) -> bool {
        self.registered
            .write()
            .expect("lock not poisoned")
            .remove(token)
            .is_some()
    }

    /// Remove the registration for `token`.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotRegistered`] if `token` is not in the
    /// registry.
    pub fn delete(&self, token: &str) -> Result<(), ServerError> {
        let mut map = self.registered.write().expect("lock not poisoned");
        if !map.contains_key(token) {
            return Err(ServerError::NotRegistered);
        }
        map.remove(token);
        Ok(())
    }
}

// ── Credentials2111Config (OCPI 2.1.1) ─────────────────────────────────────────

/// An in-memory credentials store for the **flat OCPI 2.1.1** registration
/// handshake, the 2.1.1 counterpart to [`CredentialsConfig`].
///
/// Holds the server's own flat [`Credentials2111`] and a token-keyed registry
/// of registered parties. Thread-safe via interior mutability (`RwLock`); wrap
/// in `Arc` to share across axum handlers (see [`http::credentials_2_1_1_router`]).
///
/// The 2.1.1 registration *fetch-back* — `GET`-ing the registering party's
/// `/versions` for its endpoint catalogue — is wired through the role-less
/// [`LegacyVersionFetcher`] (a 2.1.1 party advertises role-less version
/// details, which the role-bearing [`VersionFetcher`] cannot model). Build the
/// store with [`new_with_fetcher`](Self::new_with_fetcher) to enable it; plain
/// [`new`](Self::new) registers parties without an endpoint catalogue (the same
/// path [`CredentialsConfig`] takes when no fetcher is configured).
pub struct Credentials2111Config {
    /// The credentials this server returns on every successful request — its
    /// `token` is the issued *Token C* the registry is keyed by.
    pub own_credentials: Credentials2111,
    registered: std::sync::RwLock<std::collections::HashMap<String, RegisteredParty2111>>,
    /// OCPI versions this server supports, used to negotiate the fetch-back
    /// version against the registering party's `/versions` list.
    supported_versions: Vec<VersionNumber>,
    /// Optional transport for the registration fetch-back. When `None`, the
    /// fetch-back step is skipped and parties register without an endpoint
    /// catalogue.
    fetcher: Option<std::sync::Arc<dyn LegacyVersionFetcher>>,
}

/// A registered 2.1.1 remote party: their flat [`Credentials2111`] plus, when
/// the registration fetch-back ran, the role-less endpoint catalogue fetched
/// from their `/versions` details. The 2.1.1 counterpart to [`RegisteredParty`].
#[derive(Debug, Clone)]
pub struct RegisteredParty2111 {
    /// The flat credentials object the party presented at registration.
    pub credentials: Credentials2111,
    /// Endpoints fetched from the party's selected version details, or `None`
    /// when no [`LegacyVersionFetcher`] was configured (fetch-back skipped).
    pub endpoints: Option<Vec<Endpoint2111>>,
}

impl std::fmt::Debug for Credentials2111Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials2111Config")
            .field("own_credentials", &self.own_credentials)
            .field(
                "registered_count",
                &self.registered.read().map(|m| m.len()).unwrap_or(0),
            )
            .field("supported_versions", &self.supported_versions)
            .field("fetch_back", &self.fetcher.is_some())
            .finish()
    }
}

impl Credentials2111Config {
    /// Create a new 2.1.1 registry with the given server credentials.
    ///
    /// No parties are registered initially and no registration fetch-back is
    /// performed (parties register without an endpoint catalogue). Parties
    /// register via the axum router ([`http::credentials_2_1_1_router`]) or
    /// [`register`](Self::register). Use
    /// [`new_with_fetcher`](Self::new_with_fetcher) to enable the fetch-back.
    #[must_use]
    pub fn new(own_credentials: Credentials2111) -> Self {
        Self {
            own_credentials,
            registered: std::sync::RwLock::new(std::collections::HashMap::new()),
            supported_versions: vec![VersionNumber::V2_1_1],
            fetcher: None,
        }
    }

    /// Create a 2.1.1 registry that performs the registration fetch-back.
    ///
    /// On `POST`/`PUT /credentials`, the server calls `fetcher` to `GET` the
    /// registering party's `/versions` list, selects the highest version that
    /// is also in `supported_versions`, fetches that version's **role-less**
    /// endpoint catalogue, and stores it alongside the party's credentials. Any
    /// failure surfaces as OCPI status code `3001`.
    #[must_use]
    pub fn new_with_fetcher(
        own_credentials: Credentials2111,
        supported_versions: Vec<VersionNumber>,
        fetcher: std::sync::Arc<dyn LegacyVersionFetcher>,
    ) -> Self {
        Self {
            own_credentials,
            registered: std::sync::RwLock::new(std::collections::HashMap::new()),
            supported_versions,
            fetcher: Some(fetcher),
        }
    }

    /// Returns `true` if `token` belongs to a registered party.
    #[must_use]
    pub fn is_registered(&self, token: &str) -> bool {
        self.registered
            .read()
            .expect("lock not poisoned")
            .contains_key(token)
    }

    /// Register a new party under `token`, storing their flat credentials
    /// without an endpoint catalogue.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::AlreadyRegistered`] if `token` is already known.
    pub fn register(&self, token: &str, credentials: Credentials2111) -> Result<(), ServerError> {
        self.register_with_endpoints(token, credentials, None)
    }

    /// Register a new party under `token`, storing their flat credentials and
    /// the role-less endpoint catalogue fetched during the handshake.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::AlreadyRegistered`] if `token` is already known.
    pub fn register_with_endpoints(
        &self,
        token: &str,
        credentials: Credentials2111,
        endpoints: Option<Vec<Endpoint2111>>,
    ) -> Result<(), ServerError> {
        let mut map = self.registered.write().expect("lock not poisoned");
        if map.contains_key(token) {
            return Err(ServerError::AlreadyRegistered);
        }
        map.insert(
            token.to_owned(),
            RegisteredParty2111 {
                credentials,
                endpoints,
            },
        );
        Ok(())
    }

    /// Update the stored credentials for an already-registered party, clearing
    /// any stored endpoint catalogue.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotRegistered`] if `token` is not in the registry.
    pub fn update(&self, token: &str, credentials: Credentials2111) -> Result<(), ServerError> {
        self.update_with_endpoints(token, credentials, None)
    }

    /// Update the stored credentials and role-less endpoint catalogue for an
    /// already-registered party.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotRegistered`] if `token` is not in the registry.
    pub fn update_with_endpoints(
        &self,
        token: &str,
        credentials: Credentials2111,
        endpoints: Option<Vec<Endpoint2111>>,
    ) -> Result<(), ServerError> {
        let mut map = self.registered.write().expect("lock not poisoned");
        if !map.contains_key(token) {
            return Err(ServerError::NotRegistered);
        }
        map.insert(
            token.to_owned(),
            RegisteredParty2111 {
                credentials,
                endpoints,
            },
        );
        Ok(())
    }

    /// Return the role-less endpoint catalogue stored for a registered party.
    ///
    /// Returns `None` when the token is unknown or when the party registered
    /// without a fetch-back (no [`LegacyVersionFetcher`] configured).
    #[must_use]
    pub fn get_endpoints(&self, token: &str) -> Option<Vec<Endpoint2111>> {
        self.registered
            .read()
            .expect("lock not poisoned")
            .get(token)
            .and_then(|party| party.endpoints.clone())
    }

    /// Run the 2.1.1 registration fetch-back for a registering party.
    ///
    /// Returns `Ok(None)` when no [`LegacyVersionFetcher`] is configured (the
    /// step is skipped). Otherwise it `GET`s the party's `/versions` list,
    /// selects the highest mutually-supported version, fetches that version's
    /// role-less endpoint catalogue, and returns it.
    ///
    /// The presented [`Credentials2111::token`] is used as the OCPI
    /// authorization credential for the outbound calls;
    /// [`Credentials2111::url`] is the party's `/versions` URL.
    ///
    /// # Errors
    ///
    /// Returns [`FetchError`] on any transport, negotiation, or parse failure.
    /// Callers map this to OCPI status code `3001`.
    pub async fn fetch_back(
        &self,
        credentials: &Credentials2111,
    ) -> Result<Option<Vec<Endpoint2111>>, FetchError> {
        let Some(fetcher) = self.fetcher.as_ref() else {
            return Ok(None);
        };
        let url = credentials.url.as_str();
        let token = credentials.token.as_str();
        let remote = fetcher.fetch_versions(url, token).await?;
        let chosen = select_best_version(&remote, &self.supported_versions)
            .ok_or(FetchError::NoMutualVersion)?;
        let details = fetcher
            .fetch_version_details(chosen.url.as_str(), token)
            .await?;
        Ok(Some(details.endpoints))
    }

    /// Invalidate `token`, removing it from the registry if present.
    ///
    /// Unlike [`delete`](Self::delete) this never errors on an unknown token —
    /// it is used to burn the single-use bootstrap *Token A* once registration
    /// completes. Returns `true` if the token was present.
    pub fn invalidate(&self, token: &str) -> bool {
        self.registered
            .write()
            .expect("lock not poisoned")
            .remove(token)
            .is_some()
    }

    /// Remove the registration for `token`.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotRegistered`] if `token` is not in the registry.
    pub fn delete(&self, token: &str) -> Result<(), ServerError> {
        let mut map = self.registered.write().expect("lock not poisoned");
        if !map.contains_key(token) {
            return Err(ServerError::NotRegistered);
        }
        map.remove(token);
        Ok(())
    }
}

// ── SessionsHandler ───────────────────────────────────────────────────────────

/// Handles the OCPI Sessions module endpoints.
///
/// Implements both the **sender** interface (CPO exposes `GET /sessions`) and
/// the **receiver** interface (eMSP exposes `GET/PUT/PATCH
/// /sessions/{country_code}/{party_id}/{session_id}`).
///
/// Spec: `specs/ocpi/2.2.1/mod_sessions.asciidoc`
#[allow(async_fn_in_trait)]
pub trait SessionsHandler {
    /// Paginated list of sessions whose `last_updated` is in
    /// `[date_from, date_to)` — sender interface (`GET /sessions`).
    ///
    /// Returns `(page_items, total_count)`.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the query cannot be executed.
    async fn get_sessions(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> Result<(Vec<Session>, u32), ServerError>;

    /// Fetch a single session by its composite key — receiver interface
    /// (`GET /sessions/{country_code}/{party_id}/{session_id}`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] when the session does not exist.
    async fn get_session(
        &self,
        country_code: &str,
        party_id: &str,
        session_id: &str,
    ) -> Result<Session, ServerError>;

    /// Create or replace a session — receiver interface (`PUT`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] on storage failure.
    async fn put_session(
        &self,
        country_code: &str,
        party_id: &str,
        session_id: &str,
        session: Session,
    ) -> Result<(), ServerError>;

    /// Apply a JSON merge-patch (RFC 7396) to an existing session — receiver
    /// interface (`PATCH`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] when the session does not exist, or
    /// [`ServerError::NotImplemented`] if serialization fails.
    async fn patch_session(
        &self,
        country_code: &str,
        party_id: &str,
        session_id: &str,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError>;

    /// Evaluate the driver's [`ChargingPreferences`] for an ongoing session —
    /// sender interface (`PUT /sessions/{session_id}/charging_preferences`).
    ///
    /// The Sender endpoint addresses the session by `session_id` alone (no
    /// `country_code`/`party_id` segments). Returns the CPO's
    /// [`ChargingPreferencesResponse`].
    ///
    /// The default implementation returns
    /// [`ServerError::NotImplemented`] so existing implementors are not broken;
    /// override it to support smart charging.
    ///
    /// Spec: `specs/ocpi/2.2.1/mod_sessions.asciidoc` — §Set: Charging Preferences.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] when the session does not exist, or
    /// [`ServerError::NotImplemented`] when the handler does not support
    /// charging preferences.
    async fn set_charging_preferences(
        &self,
        session_id: &str,
        preferences: ChargingPreferences,
    ) -> Result<ChargingPreferencesResponse, ServerError> {
        let _ = (session_id, preferences);
        Err(ServerError::NotImplemented("set_charging_preferences"))
    }
}

// ── SessionsConfig ────────────────────────────────────────────────────────────

/// Thread-safe in-memory sessions store for use with [`http::sessions_router`].
///
/// Sessions are keyed by `"{country_code}/{party_id}/{session_id}"`. Wrap in
/// `Arc` to share across axum handlers or multiple threads.
pub struct SessionsConfig {
    sessions: std::sync::RwLock<std::collections::HashMap<String, Session>>,
}

impl std::fmt::Debug for SessionsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionsConfig")
            .field(
                "session_count",
                &self.sessions.read().map(|m| m.len()).unwrap_or(0),
            )
            .finish()
    }
}

impl SessionsConfig {
    /// Create an empty sessions store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            sessions: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    fn composite_key(country_code: &str, party_id: &str, session_id: &str) -> String {
        format!("{country_code}/{party_id}/{session_id}")
    }

    /// Insert or replace a session.
    pub fn put(&self, country_code: &str, party_id: &str, session_id: &str, session: Session) {
        let key = Self::composite_key(country_code, party_id, session_id);
        self.sessions
            .write()
            .expect("lock not poisoned")
            .insert(key, session);
    }

    /// Retrieve a session by its composite key.
    #[must_use]
    pub fn get(&self, country_code: &str, party_id: &str, session_id: &str) -> Option<Session> {
        let key = Self::composite_key(country_code, party_id, session_id);
        self.sessions
            .read()
            .expect("lock not poisoned")
            .get(&key)
            .cloned()
    }

    /// Apply a JSON merge-patch to an existing session.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] if no session matches the key.
    pub fn patch_json(
        &self,
        country_code: &str,
        party_id: &str,
        session_id: &str,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError> {
        let key = Self::composite_key(country_code, party_id, session_id);
        let mut map = self.sessions.write().expect("lock not poisoned");
        let session = map.get(&key).ok_or(ServerError::NotFound)?;
        let mut base = ocpi_types::serde_json::to_value(session.clone())
            .map_err(|_| ServerError::NotImplemented("patch serialize"))?;
        json_merge(&mut base, partial);
        let updated: Session = ocpi_types::serde_json::from_value(base)
            .map_err(|_| ServerError::NotImplemented("patch deserialize"))?;
        map.insert(key, updated);
        Ok(())
    }

    /// Return a filtered and paginated slice of sessions.
    ///
    /// Filters by `last_updated >= date_from` and (if provided)
    /// `last_updated < date_to`. Results are sorted by `last_updated`.
    ///
    /// Returns `(page_items, total_matching_count)`.
    #[must_use]
    pub fn list(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> (Vec<Session>, u32) {
        let map = self.sessions.read().expect("lock not poisoned");
        let mut filtered: Vec<&Session> = map
            .values()
            .filter(|s| s.last_updated >= date_from && date_to.is_none_or(|dt| s.last_updated < dt))
            .collect();
        filtered.sort_by_key(|s| s.last_updated);
        let total = filtered.len() as u32;
        let page: Vec<Session> = filtered
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect();
        (page, total)
    }

    /// Evaluate driver [`ChargingPreferences`] for an ongoing session, looked up
    /// by `session_id` alone (the Sender `PUT
    /// /sessions/{session_id}/charging_preferences` endpoint omits the
    /// `country_code`/`party_id` segments).
    ///
    /// This in-memory reference store applies a deterministic default policy
    /// modelling a CPO that needs planning input for smart-charging profiles:
    ///
    /// - unknown `session_id` → [`ServerError::NotFound`]
    /// - [`ProfileType::Regular`] → [`ChargingPreferencesResponse::Accepted`]
    ///   (no planning input needed)
    /// - any other profile with no `departure_time` →
    ///   [`ChargingPreferencesResponse::DepartureRequired`]
    /// - [`ProfileType::Cheap`] / [`ProfileType::Green`] with a `departure_time`
    ///   but no `energy_need` →
    ///   [`ChargingPreferencesResponse::EnergyNeedRequired`]
    /// - otherwise → [`ChargingPreferencesResponse::Accepted`]
    ///
    /// Real CPOs replace this by implementing
    /// [`SessionsHandler::set_charging_preferences`].
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] when no session has id `session_id`.
    pub fn set_charging_preferences(
        &self,
        session_id: &str,
        preferences: &ChargingPreferences,
    ) -> Result<ChargingPreferencesResponse, ServerError> {
        let exists = self
            .sessions
            .read()
            .expect("lock not poisoned")
            .values()
            .any(|s| s.id.as_str() == session_id);
        if !exists {
            return Err(ServerError::NotFound);
        }
        Ok(match preferences.profile_type {
            ProfileType::Regular => ChargingPreferencesResponse::Accepted,
            _ if preferences.departure_time.is_none() => {
                ChargingPreferencesResponse::DepartureRequired
            }
            ProfileType::Cheap | ProfileType::Green if preferences.energy_need.is_none() => {
                ChargingPreferencesResponse::EnergyNeedRequired
            }
            _ => ChargingPreferencesResponse::Accepted,
        })
    }
}

impl Default for SessionsConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(async_fn_in_trait)]
impl SessionsHandler for SessionsConfig {
    async fn get_sessions(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> Result<(Vec<Session>, u32), ServerError> {
        Ok(self.list(date_from, date_to, offset, limit))
    }

    async fn get_session(
        &self,
        country_code: &str,
        party_id: &str,
        session_id: &str,
    ) -> Result<Session, ServerError> {
        self.get(country_code, party_id, session_id)
            .ok_or(ServerError::NotFound)
    }

    async fn put_session(
        &self,
        country_code: &str,
        party_id: &str,
        session_id: &str,
        session: Session,
    ) -> Result<(), ServerError> {
        self.put(country_code, party_id, session_id, session);
        Ok(())
    }

    async fn patch_session(
        &self,
        country_code: &str,
        party_id: &str,
        session_id: &str,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError> {
        self.patch_json(country_code, party_id, session_id, partial)
    }

    async fn set_charging_preferences(
        &self,
        session_id: &str,
        preferences: ChargingPreferences,
    ) -> Result<ChargingPreferencesResponse, ServerError> {
        self.set_charging_preferences(session_id, &preferences)
    }
}

// ── CdrsHandler ───────────────────────────────────────────────────────────────

/// Handles the OCPI CDRs module endpoints.
///
/// Implements both the **sender** interface (CPO exposes `GET /cdrs`) and the
/// **receiver** interface (eMSP exposes `POST /cdrs`).
///
/// Spec: `specs/ocpi/2.2.1/mod_cdrs.asciidoc`
#[allow(async_fn_in_trait)]
pub trait CdrsHandler {
    /// Paginated list of CDRs whose `last_updated` is in `[date_from, date_to)`
    /// — sender interface (`GET /cdrs`).
    ///
    /// Returns `(page_items, total_count)`.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the query cannot be executed.
    async fn get_cdrs(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> Result<(Vec<Cdr>, u32), ServerError>;

    /// Fetch a single CDR by its ID — sender interface (`GET /cdrs/{cdr_id}`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] when the CDR does not exist.
    async fn get_cdr(&self, cdr_id: &str) -> Result<Cdr, ServerError>;

    /// Store a new CDR and return its URL — receiver interface (`POST /cdrs`).
    ///
    /// The returned `String` is the absolute URL at which the stored CDR can be
    /// retrieved (used for the HTTP `Location` response header).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] on storage failure.
    async fn post_cdr(&self, cdr: Cdr) -> Result<String, ServerError>;
}

// ── CdrsConfig ────────────────────────────────────────────────────────────────

/// Thread-safe in-memory CDR store for use with [`http::cdrs_router`].
///
/// CDRs are keyed by their `id`. The `base_url` (e.g.
/// `"https://example.com/ocpi/2.2.1"`) is prepended to construct the
/// `Location` header returned by `POST /cdrs`.
pub struct CdrsConfig {
    base_url: String,
    cdrs: std::sync::RwLock<std::collections::HashMap<String, Cdr>>,
}

impl std::fmt::Debug for CdrsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CdrsConfig")
            .field("cdr_count", &self.cdrs.read().map(|m| m.len()).unwrap_or(0))
            .field("base_url", &self.base_url)
            .finish()
    }
}

impl CdrsConfig {
    /// Create an empty CDR store.
    ///
    /// `base_url` is used to build the `Location` header on `POST /cdrs`
    /// (e.g. `"https://example.com/ocpi/2.2.1"`).
    #[must_use]
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            cdrs: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// Construct the URL for a CDR by its ID.
    fn cdr_url(&self, cdr_id: &str) -> String {
        format!("{}/cdrs/{cdr_id}", self.base_url.trim_end_matches('/'))
    }

    /// Store a CDR and return its URL.
    pub fn store(&self, cdr: Cdr) -> String {
        let id = cdr.id.as_str().to_string();
        let url = self.cdr_url(&id);
        self.cdrs
            .write()
            .expect("lock not poisoned")
            .insert(id, cdr);
        url
    }

    /// Retrieve a CDR by its ID.
    #[must_use]
    pub fn get(&self, cdr_id: &str) -> Option<Cdr> {
        self.cdrs
            .read()
            .expect("lock not poisoned")
            .get(cdr_id)
            .cloned()
    }

    /// Return a filtered and paginated slice of CDRs.
    ///
    /// Filters by `last_updated >= date_from` and (if provided)
    /// `last_updated < date_to`. Results are sorted by `last_updated`.
    ///
    /// Returns `(page_items, total_matching_count)`.
    #[must_use]
    pub fn list(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> (Vec<Cdr>, u32) {
        let map = self.cdrs.read().expect("lock not poisoned");
        let mut filtered: Vec<&Cdr> = map
            .values()
            .filter(|c| c.last_updated >= date_from && date_to.is_none_or(|dt| c.last_updated < dt))
            .collect();
        filtered.sort_by_key(|c| c.last_updated);
        let total = filtered.len() as u32;
        let page: Vec<Cdr> = filtered
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect();
        (page, total)
    }
}

impl Default for CdrsConfig {
    fn default() -> Self {
        Self::new("")
    }
}

#[allow(async_fn_in_trait)]
impl CdrsHandler for CdrsConfig {
    async fn get_cdrs(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> Result<(Vec<Cdr>, u32), ServerError> {
        Ok(self.list(date_from, date_to, offset, limit))
    }

    async fn get_cdr(&self, cdr_id: &str) -> Result<Cdr, ServerError> {
        self.get(cdr_id).ok_or(ServerError::NotFound)
    }

    async fn post_cdr(&self, cdr: Cdr) -> Result<String, ServerError> {
        Ok(self.store(cdr))
    }
}

// ── Sessions2111Handler (OCPI 2.1.1) ────────────────────────────────────────────

/// Handles the OCPI **2.1.1** Sessions module endpoints.
///
/// Implements both the **sender** interface (CPO exposes `GET /sessions`) and
/// the **receiver** interface (eMSP exposes `GET/PUT/PATCH
/// /sessions/{country_code}/{party_id}/{session_id}`).
///
/// ## Delta from the 2.2.1 [`SessionsHandler`]
///
/// The transport is otherwise identical — per OCPI 2.1.1 §9.2.2 *"Sessions is
/// a client owned object, so the end-points need to contain the required extra
/// fields: {party_id} and {country_code}"*, so the receiver path carries the
/// `{country_code}/{party_id}/{session_id}` segments exactly as in 2.2.1. The
/// `{country_code}/{party_id}` URL segments predate the 2.2 `OCPI-to/from-*`
/// routing *headers*. Only the payload differs: the 2.1.1 [`Session2111`]
/// object (`auth_id`, embedded `location`, one-word `start_datetime`). 2.1.1
/// has **no** `charging_preferences` endpoint (that arrived in 2.2).
///
/// Spec: OCPI 2.1.1 — *Sessions* module (§9), `specs/ocpi/2.1.1/OCPI_2.1.1.pdf`.
#[allow(async_fn_in_trait)]
pub trait Sessions2111Handler {
    /// Paginated list of 2.1.1 sessions whose `last_updated` is in
    /// `[date_from, date_to)` — sender interface (`GET /sessions`).
    ///
    /// Returns `(page_items, total_count)`.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the query cannot be executed.
    async fn get_sessions(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> Result<(Vec<Session2111>, u32), ServerError>;

    /// Fetch a single 2.1.1 session by its composite key — receiver interface
    /// (`GET /sessions/{country_code}/{party_id}/{session_id}`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] when the session does not exist.
    async fn get_session(
        &self,
        country_code: &str,
        party_id: &str,
        session_id: &str,
    ) -> Result<Session2111, ServerError>;

    /// Create or replace a 2.1.1 session — receiver interface (`PUT`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] on storage failure.
    async fn put_session(
        &self,
        country_code: &str,
        party_id: &str,
        session_id: &str,
        session: Session2111,
    ) -> Result<(), ServerError>;

    /// Apply a JSON merge-patch (RFC 7396) to an existing 2.1.1 session —
    /// receiver interface (`PATCH`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] when the session does not exist, or
    /// [`ServerError::NotImplemented`] if serialization fails.
    async fn patch_session(
        &self,
        country_code: &str,
        party_id: &str,
        session_id: &str,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError>;
}

// ── Sessions2111Config (OCPI 2.1.1) ─────────────────────────────────────────────

/// Thread-safe in-memory **OCPI 2.1.1** sessions store for use with
/// [`http::sessions_2_1_1_router`].
///
/// Mirrors [`SessionsConfig`] but stores the 2.1.1 [`Session2111`] shape.
/// Sessions are keyed by `"{country_code}/{party_id}/{session_id}"`. Wrap in
/// `Arc` to share across axum handlers or multiple threads.
pub struct Sessions2111Config {
    sessions: std::sync::RwLock<std::collections::HashMap<String, Session2111>>,
}

impl std::fmt::Debug for Sessions2111Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sessions2111Config")
            .field(
                "session_count",
                &self.sessions.read().map(|m| m.len()).unwrap_or(0),
            )
            .finish()
    }
}

impl Sessions2111Config {
    /// Create an empty 2.1.1 sessions store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            sessions: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    fn composite_key(country_code: &str, party_id: &str, session_id: &str) -> String {
        format!("{country_code}/{party_id}/{session_id}")
    }

    /// Insert or replace a session.
    pub fn put(&self, country_code: &str, party_id: &str, session_id: &str, session: Session2111) {
        let key = Self::composite_key(country_code, party_id, session_id);
        self.sessions
            .write()
            .expect("lock not poisoned")
            .insert(key, session);
    }

    /// Retrieve a session by its composite key.
    #[must_use]
    pub fn get(&self, country_code: &str, party_id: &str, session_id: &str) -> Option<Session2111> {
        let key = Self::composite_key(country_code, party_id, session_id);
        self.sessions
            .read()
            .expect("lock not poisoned")
            .get(&key)
            .cloned()
    }

    /// Apply a JSON merge-patch to an existing session.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] if no session matches the key.
    pub fn patch_json(
        &self,
        country_code: &str,
        party_id: &str,
        session_id: &str,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError> {
        let key = Self::composite_key(country_code, party_id, session_id);
        let mut map = self.sessions.write().expect("lock not poisoned");
        let session = map.get(&key).ok_or(ServerError::NotFound)?;
        let mut base = ocpi_types::serde_json::to_value(session.clone())
            .map_err(|_| ServerError::NotImplemented("patch serialize"))?;
        json_merge(&mut base, partial);
        let updated: Session2111 = ocpi_types::serde_json::from_value(base)
            .map_err(|_| ServerError::NotImplemented("patch deserialize"))?;
        map.insert(key, updated);
        Ok(())
    }

    /// Return a filtered and paginated slice of sessions.
    ///
    /// Filters by `last_updated >= date_from` and (if provided)
    /// `last_updated < date_to`. Results are sorted by `last_updated`.
    ///
    /// Returns `(page_items, total_matching_count)`.
    #[must_use]
    pub fn list(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> (Vec<Session2111>, u32) {
        let map = self.sessions.read().expect("lock not poisoned");
        let mut filtered: Vec<&Session2111> = map
            .values()
            .filter(|s| s.last_updated >= date_from && date_to.is_none_or(|dt| s.last_updated < dt))
            .collect();
        filtered.sort_by_key(|s| s.last_updated);
        let total = filtered.len() as u32;
        let page: Vec<Session2111> = filtered
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect();
        (page, total)
    }
}

impl Default for Sessions2111Config {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(async_fn_in_trait)]
impl Sessions2111Handler for Sessions2111Config {
    async fn get_sessions(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> Result<(Vec<Session2111>, u32), ServerError> {
        Ok(self.list(date_from, date_to, offset, limit))
    }

    async fn get_session(
        &self,
        country_code: &str,
        party_id: &str,
        session_id: &str,
    ) -> Result<Session2111, ServerError> {
        self.get(country_code, party_id, session_id)
            .ok_or(ServerError::NotFound)
    }

    async fn put_session(
        &self,
        country_code: &str,
        party_id: &str,
        session_id: &str,
        session: Session2111,
    ) -> Result<(), ServerError> {
        self.put(country_code, party_id, session_id, session);
        Ok(())
    }

    async fn patch_session(
        &self,
        country_code: &str,
        party_id: &str,
        session_id: &str,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError> {
        self.patch_json(country_code, party_id, session_id, partial)
    }
}

// ── Cdrs2111Handler (OCPI 2.1.1) ────────────────────────────────────────────────

/// Handles the OCPI **2.1.1** CDRs module endpoints.
///
/// Implements both the **sender** interface (CPO exposes `GET /cdrs`) and the
/// **receiver** interface (eMSP exposes `POST /cdrs` + `GET /cdrs/{cdr_id}`).
///
/// ## Delta from the 2.2.1 [`CdrsHandler`]
///
/// The transport is identical — a CDR is a **server-owned** object (the
/// receiver names it via the `Location` header on `POST /cdrs`, §10.2.2), so
/// the 2.1.1 paths are **flat** (`/cdrs`, `/cdrs/{cdr_id}`) with no
/// `{country_code}/{party_id}` segments, exactly as in 2.2.1. Only the payload
/// differs: the 2.1.1 [`Cdr2111`] object (bare `auth_id`, embedded `location`,
/// `stop_date_time`, a single numeric `total_cost`, no `session_id`).
///
/// Spec: OCPI 2.1.1 — *CDRs* module (§10), `specs/ocpi/2.1.1/OCPI_2.1.1.pdf`.
#[allow(async_fn_in_trait)]
pub trait Cdrs2111Handler {
    /// Paginated list of 2.1.1 CDRs whose `last_updated` is in
    /// `[date_from, date_to)` — sender interface (`GET /cdrs`).
    ///
    /// Returns `(page_items, total_count)`.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the query cannot be executed.
    async fn get_cdrs(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> Result<(Vec<Cdr2111>, u32), ServerError>;

    /// Fetch a single 2.1.1 CDR by its ID — receiver interface
    /// (`GET /cdrs/{cdr_id}`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] when the CDR does not exist.
    async fn get_cdr(&self, cdr_id: &str) -> Result<Cdr2111, ServerError>;

    /// Store a new 2.1.1 CDR and return its URL — receiver interface
    /// (`POST /cdrs`).
    ///
    /// The returned `String` is the absolute URL at which the stored CDR can be
    /// retrieved (used for the HTTP `Location` response header).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] on storage failure.
    async fn post_cdr(&self, cdr: Cdr2111) -> Result<String, ServerError>;
}

// ── Cdrs2111Config (OCPI 2.1.1) ─────────────────────────────────────────────────

/// Thread-safe in-memory **OCPI 2.1.1** CDR store for use with
/// [`http::cdrs_2_1_1_router`].
///
/// Mirrors [`CdrsConfig`] but stores the 2.1.1 [`Cdr2111`] shape. CDRs are
/// keyed by their `id`. The `base_url` (e.g.
/// `"https://example.com/ocpi/2.1.1"`) is prepended to construct the
/// `Location` header returned by `POST /cdrs`.
pub struct Cdrs2111Config {
    base_url: String,
    cdrs: std::sync::RwLock<std::collections::HashMap<String, Cdr2111>>,
}

impl std::fmt::Debug for Cdrs2111Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cdrs2111Config")
            .field("cdr_count", &self.cdrs.read().map(|m| m.len()).unwrap_or(0))
            .field("base_url", &self.base_url)
            .finish()
    }
}

impl Cdrs2111Config {
    /// Create an empty 2.1.1 CDR store.
    ///
    /// `base_url` is used to build the `Location` header on `POST /cdrs`
    /// (e.g. `"https://example.com/ocpi/2.1.1"`).
    #[must_use]
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            cdrs: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// Construct the URL for a CDR by its ID.
    fn cdr_url(&self, cdr_id: &str) -> String {
        format!("{}/cdrs/{cdr_id}", self.base_url.trim_end_matches('/'))
    }

    /// Store a CDR and return its URL.
    pub fn store(&self, cdr: Cdr2111) -> String {
        let id = cdr.id.as_str().to_string();
        let url = self.cdr_url(&id);
        self.cdrs
            .write()
            .expect("lock not poisoned")
            .insert(id, cdr);
        url
    }

    /// Retrieve a CDR by its ID.
    #[must_use]
    pub fn get(&self, cdr_id: &str) -> Option<Cdr2111> {
        self.cdrs
            .read()
            .expect("lock not poisoned")
            .get(cdr_id)
            .cloned()
    }

    /// Return a filtered and paginated slice of CDRs.
    ///
    /// Filters by `last_updated >= date_from` and (if provided)
    /// `last_updated < date_to`. Results are sorted by `last_updated`.
    ///
    /// Returns `(page_items, total_matching_count)`.
    #[must_use]
    pub fn list(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> (Vec<Cdr2111>, u32) {
        let map = self.cdrs.read().expect("lock not poisoned");
        let mut filtered: Vec<&Cdr2111> = map
            .values()
            .filter(|c| c.last_updated >= date_from && date_to.is_none_or(|dt| c.last_updated < dt))
            .collect();
        filtered.sort_by_key(|c| c.last_updated);
        let total = filtered.len() as u32;
        let page: Vec<Cdr2111> = filtered
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect();
        (page, total)
    }
}

impl Default for Cdrs2111Config {
    fn default() -> Self {
        Self::new("")
    }
}

#[allow(async_fn_in_trait)]
impl Cdrs2111Handler for Cdrs2111Config {
    async fn get_cdrs(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> Result<(Vec<Cdr2111>, u32), ServerError> {
        Ok(self.list(date_from, date_to, offset, limit))
    }

    async fn get_cdr(&self, cdr_id: &str) -> Result<Cdr2111, ServerError> {
        self.get(cdr_id).ok_or(ServerError::NotFound)
    }

    async fn post_cdr(&self, cdr: Cdr2111) -> Result<String, ServerError> {
        Ok(self.store(cdr))
    }
}

// ── Cdrs22Handler (OCPI 2.2) ────────────────────────────────────────────────────

/// Handles the OCPI **2.2** CDRs module endpoints.
///
/// Implements both the **sender** interface (CPO exposes `GET /cdrs`) and the
/// **receiver** interface (eMSP exposes `POST /cdrs` + `GET /cdrs/{cdr_id}`).
///
/// ## Delta from the 2.2.1 [`CdrsHandler`]
///
/// The transport is identical — a CDR is a **server-owned** object (the
/// receiver names it via the `Location` header on `POST /cdrs`, §8.2.2), so the
/// 2.2 paths are **flat** (`/cdrs`, `/cdrs/{cdr_id}`) with no
/// `{country_code}/{party_id}` segments, exactly as in 2.2.1. Only the payload
/// differs: the 2.2 [`Cdr22`] object — a `CdrToken` with no `country_code`/
/// `party_id`, a `Cdr` with no `home_charging_compensation`, and a
/// `CdrLocation` with a required `postal_code` and no `state`. Deserializing
/// into [`Cdr22`] means a 2.2 partner's CDR round-trips faithfully instead of
/// being coerced through the 2.2.1 struct.
///
/// Spec: OCPI 2.2 — *CDRs* module (§8), `specs/ocpi/2.2/mod_cdrs.asciidoc`.
#[allow(async_fn_in_trait)]
pub trait Cdrs22Handler {
    /// Paginated list of 2.2 CDRs whose `last_updated` is in
    /// `[date_from, date_to)` — sender interface (`GET /cdrs`).
    ///
    /// Returns `(page_items, total_count)`.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the query cannot be executed.
    async fn get_cdrs(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> Result<(Vec<Cdr22>, u32), ServerError>;

    /// Fetch a single 2.2 CDR by its ID — receiver interface
    /// (`GET /cdrs/{cdr_id}`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] when the CDR does not exist.
    async fn get_cdr(&self, cdr_id: &str) -> Result<Cdr22, ServerError>;

    /// Store a new 2.2 CDR and return its URL — receiver interface
    /// (`POST /cdrs`).
    ///
    /// The returned `String` is the absolute URL at which the stored CDR can be
    /// retrieved (used for the HTTP `Location` response header).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] on storage failure.
    async fn post_cdr(&self, cdr: Cdr22) -> Result<String, ServerError>;
}

// ── Cdrs22Config (OCPI 2.2) ─────────────────────────────────────────────────────

/// Thread-safe in-memory **OCPI 2.2** CDR store for use with
/// [`http::cdrs_2_2_router`].
///
/// Mirrors [`Cdrs2111Config`] but stores the 2.2 [`Cdr22`] shape. CDRs are
/// keyed by their `id`. The `base_url` (e.g.
/// `"https://example.com/ocpi/2.2"`) is prepended to construct the `Location`
/// header returned by `POST /cdrs`.
pub struct Cdrs22Config {
    base_url: String,
    cdrs: std::sync::RwLock<std::collections::HashMap<String, Cdr22>>,
}

impl std::fmt::Debug for Cdrs22Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cdrs22Config")
            .field("cdr_count", &self.cdrs.read().map(|m| m.len()).unwrap_or(0))
            .field("base_url", &self.base_url)
            .finish()
    }
}

impl Cdrs22Config {
    /// Create an empty 2.2 CDR store.
    ///
    /// `base_url` is used to build the `Location` header on `POST /cdrs`
    /// (e.g. `"https://example.com/ocpi/2.2"`).
    #[must_use]
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            cdrs: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// Construct the URL for a CDR by its ID.
    fn cdr_url(&self, cdr_id: &str) -> String {
        format!("{}/cdrs/{cdr_id}", self.base_url.trim_end_matches('/'))
    }

    /// Store a CDR and return its URL.
    pub fn store(&self, cdr: Cdr22) -> String {
        let id = cdr.id.as_str().to_string();
        let url = self.cdr_url(&id);
        self.cdrs
            .write()
            .expect("lock not poisoned")
            .insert(id, cdr);
        url
    }

    /// Retrieve a CDR by its ID.
    #[must_use]
    pub fn get(&self, cdr_id: &str) -> Option<Cdr22> {
        self.cdrs
            .read()
            .expect("lock not poisoned")
            .get(cdr_id)
            .cloned()
    }

    /// Return a filtered and paginated slice of CDRs.
    ///
    /// Filters by `last_updated >= date_from` and (if provided)
    /// `last_updated < date_to`. Results are sorted by `last_updated`.
    ///
    /// Returns `(page_items, total_matching_count)`.
    #[must_use]
    pub fn list(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> (Vec<Cdr22>, u32) {
        let map = self.cdrs.read().expect("lock not poisoned");
        let mut filtered: Vec<&Cdr22> = map
            .values()
            .filter(|c| c.last_updated >= date_from && date_to.is_none_or(|dt| c.last_updated < dt))
            .collect();
        filtered.sort_by_key(|c| c.last_updated);
        let total = filtered.len() as u32;
        let page: Vec<Cdr22> = filtered
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect();
        (page, total)
    }
}

impl Default for Cdrs22Config {
    fn default() -> Self {
        Self::new("")
    }
}

#[allow(async_fn_in_trait)]
impl Cdrs22Handler for Cdrs22Config {
    async fn get_cdrs(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> Result<(Vec<Cdr22>, u32), ServerError> {
        Ok(self.list(date_from, date_to, offset, limit))
    }

    async fn get_cdr(&self, cdr_id: &str) -> Result<Cdr22, ServerError> {
        self.get(cdr_id).ok_or(ServerError::NotFound)
    }

    async fn post_cdr(&self, cdr: Cdr22) -> Result<String, ServerError> {
        Ok(self.store(cdr))
    }
}

// ── TariffsHandler ────────────────────────────────────────────────────────────

/// Handles the OCPI Tariffs module endpoints.
///
/// Implements the **sender** interface (CPO exposes `GET /tariffs`) and the
/// **receiver** interface (eMSP exposes `GET/PUT/DELETE
/// /tariffs/{country_code}/{party_id}/{tariff_id}`).
///
/// Spec: `specs/ocpi/2.2.1/mod_tariffs.asciidoc`
#[allow(async_fn_in_trait)]
pub trait TariffsHandler {
    /// Paginated list of tariffs whose `last_updated` is in
    /// `[date_from, date_to)` — sender interface (`GET /tariffs`).
    ///
    /// Returns `(page_items, total_count)`.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the query cannot be executed.
    async fn get_tariffs(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> Result<(Vec<Tariff>, u32), ServerError>;

    /// Fetch a single tariff by its composite key — receiver interface
    /// (`GET /tariffs/{country_code}/{party_id}/{tariff_id}`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] when the tariff does not exist.
    async fn get_tariff(
        &self,
        country_code: &str,
        party_id: &str,
        tariff_id: &str,
    ) -> Result<Tariff, ServerError>;

    /// Create or replace a tariff — receiver interface (`PUT`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] on storage failure.
    async fn put_tariff(
        &self,
        country_code: &str,
        party_id: &str,
        tariff_id: &str,
        tariff: Tariff,
    ) -> Result<(), ServerError>;

    /// Delete a tariff — receiver interface (`DELETE`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] when the tariff does not exist.
    async fn delete_tariff(
        &self,
        country_code: &str,
        party_id: &str,
        tariff_id: &str,
    ) -> Result<(), ServerError>;
}

// ── TariffsConfig ─────────────────────────────────────────────────────────────

/// Thread-safe in-memory tariffs store for use with [`http::tariffs_router`].
///
/// Tariffs are keyed by `"{country_code}/{party_id}/{tariff_id}"`. Wrap in
/// `Arc` to share across axum handlers or multiple threads.
pub struct TariffsConfig {
    tariffs: std::sync::RwLock<std::collections::HashMap<String, Tariff>>,
}

impl std::fmt::Debug for TariffsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TariffsConfig")
            .field(
                "tariff_count",
                &self.tariffs.read().map(|m| m.len()).unwrap_or(0),
            )
            .finish()
    }
}

impl TariffsConfig {
    /// Create an empty tariffs store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tariffs: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    fn composite_key(country_code: &str, party_id: &str, tariff_id: &str) -> String {
        format!("{country_code}/{party_id}/{tariff_id}")
    }

    /// Insert or replace a tariff.
    pub fn put(&self, country_code: &str, party_id: &str, tariff_id: &str, tariff: Tariff) {
        let key = Self::composite_key(country_code, party_id, tariff_id);
        self.tariffs
            .write()
            .expect("lock not poisoned")
            .insert(key, tariff);
    }

    /// Retrieve a tariff by its composite key.
    #[must_use]
    pub fn get(&self, country_code: &str, party_id: &str, tariff_id: &str) -> Option<Tariff> {
        let key = Self::composite_key(country_code, party_id, tariff_id);
        self.tariffs
            .read()
            .expect("lock not poisoned")
            .get(&key)
            .cloned()
    }

    /// Remove a tariff by its composite key.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] if no tariff matches the key.
    pub fn delete(
        &self,
        country_code: &str,
        party_id: &str,
        tariff_id: &str,
    ) -> Result<(), ServerError> {
        let key = Self::composite_key(country_code, party_id, tariff_id);
        let mut map = self.tariffs.write().expect("lock not poisoned");
        if map.remove(&key).is_some() {
            Ok(())
        } else {
            Err(ServerError::NotFound)
        }
    }

    /// Return a filtered and paginated slice of tariffs.
    ///
    /// Filters by `last_updated >= date_from` and (if provided)
    /// `last_updated < date_to`. Results are sorted by `last_updated`.
    ///
    /// Returns `(page_items, total_matching_count)`.
    #[must_use]
    pub fn list(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> (Vec<Tariff>, u32) {
        let map = self.tariffs.read().expect("lock not poisoned");
        let mut filtered: Vec<&Tariff> = map
            .values()
            .filter(|t| t.last_updated >= date_from && date_to.is_none_or(|dt| t.last_updated < dt))
            .collect();
        filtered.sort_by_key(|t| t.last_updated);
        let total = filtered.len() as u32;
        let page: Vec<Tariff> = filtered
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect();
        (page, total)
    }
}

impl Default for TariffsConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(async_fn_in_trait)]
impl TariffsHandler for TariffsConfig {
    async fn get_tariffs(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> Result<(Vec<Tariff>, u32), ServerError> {
        Ok(self.list(date_from, date_to, offset, limit))
    }

    async fn get_tariff(
        &self,
        country_code: &str,
        party_id: &str,
        tariff_id: &str,
    ) -> Result<Tariff, ServerError> {
        self.get(country_code, party_id, tariff_id)
            .ok_or(ServerError::NotFound)
    }

    async fn put_tariff(
        &self,
        country_code: &str,
        party_id: &str,
        tariff_id: &str,
        tariff: Tariff,
    ) -> Result<(), ServerError> {
        self.put(country_code, party_id, tariff_id, tariff);
        Ok(())
    }

    async fn delete_tariff(
        &self,
        country_code: &str,
        party_id: &str,
        tariff_id: &str,
    ) -> Result<(), ServerError> {
        self.delete(country_code, party_id, tariff_id)
    }
}

// ── Tariffs2111Handler (OCPI 2.1.1) ─────────────────────────────────────────────

/// Handles the **OCPI 2.1.1** Tariffs module endpoints — the 2.1.1 counterpart
/// to [`TariffsHandler`].
///
/// The transport paths are identical to 2.2.1: the Sender (CPO) interface is
/// flat (`GET /tariffs`, §11.2.1) and the Receiver (eMSP) interface is a
/// client-owned object keyed by `{country_code}/{party_id}/{tariff_id}`
/// (§11.2.2). Only the [`Tariff2111`] object shape differs from 2.2.1 (no
/// `country_code`/`party_id`/`type`/`min_price`/`max_price`).
///
/// This mirrors the 2.2.1 surface — `GET` (Sender list) + `GET`/`PUT`/`DELETE`
/// (Receiver). The 2.1.1 spec additionally lists a Receiver `PATCH` (partial
/// tariff updates); it is deferred for parity with the 2.2.1 router, which also
/// omits it.
///
/// Spec: OCPI 2.1.1 — *Tariffs* module (`specs/ocpi/2.1.1/OCPI_2.1.1.pdf`, §11).
#[allow(async_fn_in_trait)]
pub trait Tariffs2111Handler {
    /// Paginated list of 2.1.1 tariffs whose `last_updated` is in
    /// `[date_from, date_to)` — Sender interface (`GET /tariffs`).
    ///
    /// Returns `(page_items, total_count)`.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the query cannot be executed.
    async fn get_tariffs(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> Result<(Vec<Tariff2111>, u32), ServerError>;

    /// Fetch a single 2.1.1 tariff by its composite key — Receiver interface
    /// (`GET /tariffs/{country_code}/{party_id}/{tariff_id}`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] when the tariff does not exist.
    async fn get_tariff(
        &self,
        country_code: &str,
        party_id: &str,
        tariff_id: &str,
    ) -> Result<Tariff2111, ServerError>;

    /// Create or replace a 2.1.1 tariff — Receiver interface (`PUT`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] on storage failure.
    async fn put_tariff(
        &self,
        country_code: &str,
        party_id: &str,
        tariff_id: &str,
        tariff: Tariff2111,
    ) -> Result<(), ServerError>;

    /// Delete a 2.1.1 tariff — Receiver interface (`DELETE`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] when the tariff does not exist.
    async fn delete_tariff(
        &self,
        country_code: &str,
        party_id: &str,
        tariff_id: &str,
    ) -> Result<(), ServerError>;
}

// ── Tariffs2111Config ───────────────────────────────────────────────────────────

/// Thread-safe in-memory **OCPI 2.1.1** tariffs store for use with
/// [`http::tariffs_2_1_1_router`], the 2.1.1 counterpart to [`TariffsConfig`].
///
/// Tariffs are keyed by `"{country_code}/{party_id}/{tariff_id}"`. Wrap in
/// `Arc` to share across axum handlers or multiple threads.
pub struct Tariffs2111Config {
    tariffs: std::sync::RwLock<std::collections::HashMap<String, Tariff2111>>,
}

impl std::fmt::Debug for Tariffs2111Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tariffs2111Config")
            .field(
                "tariff_count",
                &self.tariffs.read().map(|m| m.len()).unwrap_or(0),
            )
            .finish()
    }
}

impl Tariffs2111Config {
    /// Create an empty 2.1.1 tariffs store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tariffs: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    fn composite_key(country_code: &str, party_id: &str, tariff_id: &str) -> String {
        format!("{country_code}/{party_id}/{tariff_id}")
    }

    /// Insert or replace a tariff.
    pub fn put(&self, country_code: &str, party_id: &str, tariff_id: &str, tariff: Tariff2111) {
        let key = Self::composite_key(country_code, party_id, tariff_id);
        self.tariffs
            .write()
            .expect("lock not poisoned")
            .insert(key, tariff);
    }

    /// Retrieve a tariff by its composite key.
    #[must_use]
    pub fn get(&self, country_code: &str, party_id: &str, tariff_id: &str) -> Option<Tariff2111> {
        let key = Self::composite_key(country_code, party_id, tariff_id);
        self.tariffs
            .read()
            .expect("lock not poisoned")
            .get(&key)
            .cloned()
    }

    /// Remove a tariff by its composite key.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] if no tariff matches the key.
    pub fn delete(
        &self,
        country_code: &str,
        party_id: &str,
        tariff_id: &str,
    ) -> Result<(), ServerError> {
        let key = Self::composite_key(country_code, party_id, tariff_id);
        let mut map = self.tariffs.write().expect("lock not poisoned");
        if map.remove(&key).is_some() {
            Ok(())
        } else {
            Err(ServerError::NotFound)
        }
    }

    /// Return a filtered and paginated slice of tariffs.
    ///
    /// Filters by `last_updated >= date_from` and (if provided)
    /// `last_updated < date_to`. Results are sorted by `last_updated`.
    ///
    /// Returns `(page_items, total_matching_count)`.
    #[must_use]
    pub fn list(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> (Vec<Tariff2111>, u32) {
        let map = self.tariffs.read().expect("lock not poisoned");
        let mut filtered: Vec<&Tariff2111> = map
            .values()
            .filter(|t| t.last_updated >= date_from && date_to.is_none_or(|dt| t.last_updated < dt))
            .collect();
        filtered.sort_by_key(|t| t.last_updated);
        let total = filtered.len() as u32;
        let page: Vec<Tariff2111> = filtered
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect();
        (page, total)
    }
}

impl Default for Tariffs2111Config {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(async_fn_in_trait)]
impl Tariffs2111Handler for Tariffs2111Config {
    async fn get_tariffs(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> Result<(Vec<Tariff2111>, u32), ServerError> {
        Ok(self.list(date_from, date_to, offset, limit))
    }

    async fn get_tariff(
        &self,
        country_code: &str,
        party_id: &str,
        tariff_id: &str,
    ) -> Result<Tariff2111, ServerError> {
        self.get(country_code, party_id, tariff_id)
            .ok_or(ServerError::NotFound)
    }

    async fn put_tariff(
        &self,
        country_code: &str,
        party_id: &str,
        tariff_id: &str,
        tariff: Tariff2111,
    ) -> Result<(), ServerError> {
        self.put(country_code, party_id, tariff_id, tariff);
        Ok(())
    }

    async fn delete_tariff(
        &self,
        country_code: &str,
        party_id: &str,
        tariff_id: &str,
    ) -> Result<(), ServerError> {
        self.delete(country_code, party_id, tariff_id)
    }
}

// ── Tokens2111Handler (OCPI 2.1.1) ──────────────────────────────────────────────

/// Handles the OCPI **2.1.1** Tokens module endpoints.
///
/// Implements the **sender** interface (eMSP exposes `GET /tokens` list for CPO
/// pull, plus the real-time `POST /tokens/{token_uid}/authorize`) and the
/// **receiver** interface (CPO receives `GET/PUT/PATCH
/// /tokens/{country_code}/{party_id}/{token_uid}`).
///
/// ## Delta from the 2.2.1 [`TokensHandler`]
///
/// Per OCPI 2.1.1 §12.2.2 *"Token is a client owned object, so the end-points
/// need to contain the required extra fields: {party_id} and {country_code}"*,
/// so the receiver path carries the `{country_code}/{party_id}/{token_uid}`
/// segments exactly as in 2.2.1 — those URL segments predate the 2.2
/// `OCPI-to/from-*` routing *headers*. Only the payload differs: the 2.1.1
/// [`Token2111`] (`auth_id`, `OTHER`/`RFID` only) and the slimmer 2.1.1
/// [`AuthorizationInfo2111`] (no `token`, no `authorization_reference`).
///
/// Spec: OCPI 2.1.1 — *Tokens* module (§12), `specs/ocpi/2.1.1/OCPI_2.1.1.pdf`.
#[allow(async_fn_in_trait)]
pub trait Tokens2111Handler {
    /// Paginated list of 2.1.1 tokens whose `last_updated` is in
    /// `[date_from, date_to)` — sender interface (`GET /tokens`).
    ///
    /// Returns `(page_items, total_count)`.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the query cannot be executed.
    async fn get_tokens(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> Result<(Vec<Token2111>, u32), ServerError>;

    /// Fetch a single 2.1.1 token by its composite key — receiver interface
    /// (`GET /tokens/{country_code}/{party_id}/{token_uid}?type=`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] when the token does not exist.
    async fn get_token(
        &self,
        country_code: &str,
        party_id: &str,
        token_uid: &str,
        token_type: TokenType2111,
    ) -> Result<Token2111, ServerError>;

    /// Create or replace a 2.1.1 token — receiver interface (`PUT`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] on storage failure.
    async fn put_token(
        &self,
        country_code: &str,
        party_id: &str,
        token_uid: &str,
        token_type: TokenType2111,
        token: Token2111,
    ) -> Result<(), ServerError>;

    /// Apply a JSON merge-patch (RFC 7396) to an existing 2.1.1 token — receiver
    /// interface (`PATCH`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] when the token does not exist.
    async fn patch_token(
        &self,
        country_code: &str,
        party_id: &str,
        token_uid: &str,
        token_type: TokenType2111,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError>;

    /// Real-time authorization — sender interface
    /// (`POST /tokens/{token_uid}/authorize?type=`).
    ///
    /// Returns the 2.1.1 [`AuthorizationInfo2111`] when the token is known.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::UnknownToken`] (OCPI 2004) when the token is not
    /// found in this eMSP's system.
    async fn authorize(
        &self,
        token_uid: &str,
        token_type: TokenType2111,
        location: Option<LocationReferences2111>,
    ) -> Result<AuthorizationInfo2111, ServerError>;
}

// ── Tokens2111Config (OCPI 2.1.1) ───────────────────────────────────────────────

/// Thread-safe in-memory **OCPI 2.1.1** tokens store for use with
/// [`http::tokens_2_1_1_router`].
///
/// Mirrors [`TokensConfig`] but stores the 2.1.1 [`Token2111`] shape and returns
/// the 2.1.1 [`AuthorizationInfo2111`]. Tokens are keyed by
/// `"{country_code}/{party_id}/{token_uid}/{token_type}"`. Wrap in `Arc` to
/// share across axum handlers or multiple threads.
pub struct Tokens2111Config {
    tokens: std::sync::RwLock<std::collections::HashMap<String, Token2111>>,
}

impl std::fmt::Debug for Tokens2111Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tokens2111Config")
            .field(
                "token_count",
                &self.tokens.read().map(|m| m.len()).unwrap_or(0),
            )
            .finish()
    }
}

impl Tokens2111Config {
    /// Create an empty 2.1.1 tokens store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tokens: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    fn composite_key(
        country_code: &str,
        party_id: &str,
        token_uid: &str,
        token_type: TokenType2111,
    ) -> String {
        format!(
            "{country_code}/{party_id}/{token_uid}/{}",
            token_type_2_1_1_str(token_type)
        )
    }

    /// Insert or replace a token.
    pub fn put(
        &self,
        country_code: &str,
        party_id: &str,
        token_uid: &str,
        token_type: TokenType2111,
        token: Token2111,
    ) {
        let key = Self::composite_key(country_code, party_id, token_uid, token_type);
        self.tokens
            .write()
            .expect("lock not poisoned")
            .insert(key, token);
    }

    /// Retrieve a token by its composite key.
    #[must_use]
    pub fn get(
        &self,
        country_code: &str,
        party_id: &str,
        token_uid: &str,
        token_type: TokenType2111,
    ) -> Option<Token2111> {
        let key = Self::composite_key(country_code, party_id, token_uid, token_type);
        self.tokens
            .read()
            .expect("lock not poisoned")
            .get(&key)
            .cloned()
    }

    /// Apply a JSON merge-patch to an existing token.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] if no token matches the key.
    pub fn patch_json(
        &self,
        country_code: &str,
        party_id: &str,
        token_uid: &str,
        token_type: TokenType2111,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError> {
        let key = Self::composite_key(country_code, party_id, token_uid, token_type);
        let mut map = self.tokens.write().expect("lock not poisoned");
        let token = map.get(&key).ok_or(ServerError::NotFound)?;
        let mut base = ocpi_types::serde_json::to_value(token.clone())
            .map_err(|_| ServerError::NotImplemented("patch serialize"))?;
        json_merge(&mut base, partial);
        let updated: Token2111 = ocpi_types::serde_json::from_value(base)
            .map_err(|_| ServerError::NotImplemented("patch deserialize"))?;
        map.insert(key, updated);
        Ok(())
    }

    /// Return a filtered and paginated slice of tokens.
    ///
    /// Filters by `last_updated >= date_from` and (if provided)
    /// `last_updated < date_to`. Results are sorted by `last_updated`.
    ///
    /// Returns `(page_items, total_matching_count)`.
    #[must_use]
    pub fn list(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> (Vec<Token2111>, u32) {
        let map = self.tokens.read().expect("lock not poisoned");
        let mut filtered: Vec<&Token2111> = map
            .values()
            .filter(|t| t.last_updated >= date_from && date_to.is_none_or(|dt| t.last_updated < dt))
            .collect();
        filtered.sort_by_key(|t| t.last_updated);
        let total = filtered.len() as u32;
        let page: Vec<Token2111> = filtered
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect();
        (page, total)
    }

    /// Perform a real-time authorization lookup by `uid` and `token_type`.
    ///
    /// Searches all stored tokens (regardless of owner party) for a match.
    /// A valid token yields `ALLOWED` (echoing the requested `location`); an
    /// invalid one yields `BLOCKED` (with `location` cleared).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::UnknownToken`] (OCPI 2004) if no token with the
    /// given uid and type is known to this store.
    pub fn authorize(
        &self,
        token_uid: &str,
        token_type: TokenType2111,
        location: Option<LocationReferences2111>,
    ) -> Result<AuthorizationInfo2111, ServerError> {
        use ocpi_types::v2_2_1::AllowedType;
        let map = self.tokens.read().expect("lock not poisoned");
        let token = map
            .values()
            .find(|t| t.uid.as_str() == token_uid && t.token_type == token_type)
            .ok_or(ServerError::UnknownToken)?;
        let allowed = if token.valid {
            AllowedType::Allowed
        } else {
            AllowedType::Blocked
        };
        let location = if matches!(allowed, AllowedType::Allowed) {
            location
        } else {
            None
        };
        Ok(AuthorizationInfo2111 {
            allowed,
            location,
            info: None,
        })
    }
}

impl Default for Tokens2111Config {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(async_fn_in_trait)]
impl Tokens2111Handler for Tokens2111Config {
    async fn get_tokens(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> Result<(Vec<Token2111>, u32), ServerError> {
        Ok(self.list(date_from, date_to, offset, limit))
    }

    async fn get_token(
        &self,
        country_code: &str,
        party_id: &str,
        token_uid: &str,
        token_type: TokenType2111,
    ) -> Result<Token2111, ServerError> {
        self.get(country_code, party_id, token_uid, token_type)
            .ok_or(ServerError::NotFound)
    }

    async fn put_token(
        &self,
        country_code: &str,
        party_id: &str,
        token_uid: &str,
        token_type: TokenType2111,
        token: Token2111,
    ) -> Result<(), ServerError> {
        self.put(country_code, party_id, token_uid, token_type, token);
        Ok(())
    }

    async fn patch_token(
        &self,
        country_code: &str,
        party_id: &str,
        token_uid: &str,
        token_type: TokenType2111,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError> {
        self.patch_json(country_code, party_id, token_uid, token_type, partial)
    }

    async fn authorize(
        &self,
        token_uid: &str,
        token_type: TokenType2111,
        location: Option<LocationReferences2111>,
    ) -> Result<AuthorizationInfo2111, ServerError> {
        // The inherent `authorize` shadows this trait method (inherent wins in
        // method lookup), so this delegates to the store without recursing.
        self.authorize(token_uid, token_type, location)
    }
}

// ── TokensHandler ─────────────────────────────────────────────────────────────

/// Handles the OCPI Tokens module endpoints.
///
/// Implements the **receiver** interface (CPO receives token updates from eMSP),
/// the **sender** interface (eMSP exposes `GET /tokens` list for CPO pull), and
/// the real-time **authorize** endpoint (eMSP receiver, CPO sender).
///
/// Spec: `specs/ocpi/2.2.1/mod_tokens.asciidoc`
#[allow(async_fn_in_trait)]
pub trait TokensHandler {
    /// Paginated list of tokens — sender interface (`GET /tokens`).
    ///
    /// Returns `(page_items, total_count)`.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the query cannot be executed.
    async fn get_tokens(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> Result<(Vec<Token>, u32), ServerError>;

    /// Fetch a single token by its composite key — receiver interface
    /// (`GET /tokens/{country_code}/{party_id}/{token_uid}?type=`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] when the token does not exist.
    async fn get_token(
        &self,
        country_code: &str,
        party_id: &str,
        token_uid: &str,
        token_type: TokenType,
    ) -> Result<Token, ServerError>;

    /// Create or replace a token — receiver interface (`PUT`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] on storage failure.
    async fn put_token(
        &self,
        country_code: &str,
        party_id: &str,
        token_uid: &str,
        token_type: TokenType,
        token: Token,
    ) -> Result<(), ServerError>;

    /// Apply a JSON merge-patch (RFC 7396) to an existing token — receiver
    /// interface (`PATCH`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] when the token does not exist.
    async fn patch_token(
        &self,
        country_code: &str,
        party_id: &str,
        token_uid: &str,
        token_type: TokenType,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError>;

    /// Real-time authorization — sender interface
    /// (`POST /tokens/{token_uid}/authorize?type=`).
    ///
    /// Returns [`AuthorizationInfo`] when the token is known.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::UnknownToken`] (OCPI 2004) when the token is
    /// not found in this eMSP's system.
    async fn authorize(
        &self,
        token_uid: &str,
        token_type: TokenType,
        location: Option<LocationReferences>,
    ) -> Result<AuthorizationInfo, ServerError>;
}

// ── TokensConfig ──────────────────────────────────────────────────────────────

/// Thread-safe in-memory tokens store for use with [`http::tokens_router`].
///
/// Tokens are keyed by `"{country_code}/{party_id}/{token_uid}/{token_type}"`.
/// Wrap in `Arc` to share across axum handlers or multiple threads.
pub struct TokensConfig {
    tokens: std::sync::RwLock<std::collections::HashMap<String, Token>>,
}

impl std::fmt::Debug for TokensConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokensConfig")
            .field(
                "token_count",
                &self.tokens.read().map(|m| m.len()).unwrap_or(0),
            )
            .finish()
    }
}

fn token_type_str(t: TokenType) -> &'static str {
    match t {
        TokenType::AdHocUser => "AD_HOC_USER",
        TokenType::AppUser => "APP_USER",
        TokenType::Other => "OTHER",
        TokenType::Rfid => "RFID",
    }
}

fn token_type_2_1_1_str(t: TokenType2111) -> &'static str {
    match t {
        TokenType2111::Other => "OTHER",
        TokenType2111::Rfid => "RFID",
    }
}

impl TokensConfig {
    /// Create an empty tokens store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tokens: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    fn composite_key(
        country_code: &str,
        party_id: &str,
        token_uid: &str,
        token_type: TokenType,
    ) -> String {
        format!(
            "{country_code}/{party_id}/{token_uid}/{}",
            token_type_str(token_type)
        )
    }

    /// Insert or replace a token.
    pub fn put(
        &self,
        country_code: &str,
        party_id: &str,
        token_uid: &str,
        token_type: TokenType,
        token: Token,
    ) {
        let key = Self::composite_key(country_code, party_id, token_uid, token_type);
        self.tokens
            .write()
            .expect("lock not poisoned")
            .insert(key, token);
    }

    /// Retrieve a token by its composite key.
    #[must_use]
    pub fn get(
        &self,
        country_code: &str,
        party_id: &str,
        token_uid: &str,
        token_type: TokenType,
    ) -> Option<Token> {
        let key = Self::composite_key(country_code, party_id, token_uid, token_type);
        self.tokens
            .read()
            .expect("lock not poisoned")
            .get(&key)
            .cloned()
    }

    /// Apply a JSON merge-patch to an existing token.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] if no token matches the key.
    pub fn patch_json(
        &self,
        country_code: &str,
        party_id: &str,
        token_uid: &str,
        token_type: TokenType,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError> {
        let key = Self::composite_key(country_code, party_id, token_uid, token_type);
        let mut map = self.tokens.write().expect("lock not poisoned");
        let token = map.get(&key).ok_or(ServerError::NotFound)?;
        let mut base = ocpi_types::serde_json::to_value(token.clone())
            .map_err(|_| ServerError::NotImplemented("patch serialize"))?;
        json_merge(&mut base, partial);
        let updated: Token = ocpi_types::serde_json::from_value(base)
            .map_err(|_| ServerError::NotImplemented("patch deserialize"))?;
        map.insert(key, updated);
        Ok(())
    }

    /// Return a filtered and paginated slice of tokens.
    ///
    /// Filters by `last_updated >= date_from` and (if provided)
    /// `last_updated < date_to`. Results are sorted by `last_updated`.
    ///
    /// Returns `(page_items, total_matching_count)`.
    #[must_use]
    pub fn list(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> (Vec<Token>, u32) {
        let map = self.tokens.read().expect("lock not poisoned");
        let mut filtered: Vec<&Token> = map
            .values()
            .filter(|t| t.last_updated >= date_from && date_to.is_none_or(|dt| t.last_updated < dt))
            .collect();
        filtered.sort_by_key(|t| t.last_updated);
        let total = filtered.len() as u32;
        let page: Vec<Token> = filtered
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect();
        (page, total)
    }

    /// Perform a real-time authorization lookup by `uid` and `token_type`.
    ///
    /// Searches all stored tokens (regardless of owner party) for a match.
    /// Returns [`ServerError::UnknownToken`] (OCPI 2004) when not found.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::UnknownToken`] if no token with the given uid and
    /// type is known to this store.
    pub fn authorize(
        &self,
        token_uid: &str,
        token_type: TokenType,
        location: Option<LocationReferences>,
    ) -> Result<AuthorizationInfo, ServerError> {
        use ocpi_types::v2_2_1::AllowedType;
        let map = self.tokens.read().expect("lock not poisoned");
        let token = map
            .values()
            .find(|t| t.uid.as_str() == token_uid && t.token_type == token_type)
            .cloned()
            .ok_or(ServerError::UnknownToken)?;
        let allowed = if token.valid {
            AllowedType::Allowed
        } else {
            AllowedType::Blocked
        };
        let location = if matches!(allowed, AllowedType::Allowed) {
            location
        } else {
            None
        };
        Ok(AuthorizationInfo {
            allowed,
            token,
            location,
            authorization_reference: None,
            info: None,
        })
    }
}

impl Default for TokensConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(async_fn_in_trait)]
impl TokensHandler for TokensConfig {
    async fn get_tokens(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> Result<(Vec<Token>, u32), ServerError> {
        Ok(self.list(date_from, date_to, offset, limit))
    }

    async fn get_token(
        &self,
        country_code: &str,
        party_id: &str,
        token_uid: &str,
        token_type: TokenType,
    ) -> Result<Token, ServerError> {
        self.get(country_code, party_id, token_uid, token_type)
            .ok_or(ServerError::NotFound)
    }

    async fn put_token(
        &self,
        country_code: &str,
        party_id: &str,
        token_uid: &str,
        token_type: TokenType,
        token: Token,
    ) -> Result<(), ServerError> {
        self.put(country_code, party_id, token_uid, token_type, token);
        Ok(())
    }

    async fn patch_token(
        &self,
        country_code: &str,
        party_id: &str,
        token_uid: &str,
        token_type: TokenType,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError> {
        self.patch_json(country_code, party_id, token_uid, token_type, partial)
    }

    async fn authorize(
        &self,
        token_uid: &str,
        token_type: TokenType,
        location: Option<LocationReferences>,
    ) -> Result<AuthorizationInfo, ServerError> {
        self.authorize(token_uid, token_type, location)
    }
}

// ── CommandsHandler ───────────────────────────────────────────────────────────

/// Handles the OCPI Commands module endpoints.
///
/// Implements the **receiver** interface (CPO receives commands from eMSP) and
/// the **sender** interface (eMSP receives async `CommandResult` callbacks from the CPO).
///
/// The two-phase flow: eMSP sends a command to CPO (receiver), CPO acknowledges
/// immediately with a [`CommandResponse`], then asynchronously POSTs a
/// [`CommandResult`] back to the `response_url` the eMSP included in the command.
///
/// Spec: `specs/ocpi/2.2.1/mod_commands.asciidoc`
#[allow(async_fn_in_trait)]
pub trait CommandsHandler {
    /// Receive a `CANCEL_RESERVATION` command — receiver interface
    /// (`POST /commands/CANCEL_RESERVATION`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the command cannot be forwarded to the Charge Point.
    async fn handle_cancel_reservation(
        &self,
        cmd: CancelReservation,
    ) -> Result<CommandResponse, ServerError>;

    /// Receive a `RESERVE_NOW` command — receiver interface
    /// (`POST /commands/RESERVE_NOW`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the command cannot be forwarded to the Charge Point.
    async fn handle_reserve_now(&self, cmd: ReserveNow) -> Result<CommandResponse, ServerError>;

    /// Receive a `START_SESSION` command — receiver interface
    /// (`POST /commands/START_SESSION`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the command cannot be forwarded to the Charge Point.
    async fn handle_start_session(&self, cmd: StartSession)
        -> Result<CommandResponse, ServerError>;

    /// Receive a `STOP_SESSION` command — receiver interface
    /// (`POST /commands/STOP_SESSION`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the command cannot be forwarded to the Charge Point.
    async fn handle_stop_session(&self, cmd: StopSession) -> Result<CommandResponse, ServerError>;

    /// Receive an `UNLOCK_CONNECTOR` command — receiver interface
    /// (`POST /commands/UNLOCK_CONNECTOR`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the command cannot be forwarded to the Charge Point.
    async fn handle_unlock_connector(
        &self,
        cmd: UnlockConnector,
    ) -> Result<CommandResponse, ServerError>;

    /// Receive the asynchronous result from the Charge Point — sender interface
    /// (`POST /commands/{command_type}/result`).
    ///
    /// The CPO delivers this after the Charge Point has executed (or failed to
    /// execute) the command. The `response_url` in each command object points here.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the result cannot be processed.
    async fn receive_command_result(
        &self,
        command_type: CommandType,
        result: CommandResult,
    ) -> Result<(), ServerError>;
}

// ── CommandsConfig ────────────────────────────────────────────────────────────

/// Stateless placeholder Commands handler for use with [`http::commands_router`].
///
/// Returns [`CommandResponseType::NotSupported`] for every incoming command.
/// Replace with a concrete bridge implementation when real CPO/OCPP integration
/// is needed; implement [`CommandsHandler`] on your own type and wire it to an
/// axum state of `Arc<YourType>`.
#[derive(Debug, Default)]
pub struct CommandsConfig;

impl CommandsConfig {
    /// Create a new `CommandsConfig` placeholder.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Returns the default "not supported" [`CommandResponse`].
    ///
    /// Used by the placeholder implementation and useful as a starting point
    /// when overriding specific commands.
    #[must_use]
    pub fn not_supported_response() -> CommandResponse {
        CommandResponse {
            result: CommandResponseType::NotSupported,
            timeout: 30,
            message: vec![],
        }
    }
}

#[allow(async_fn_in_trait)]
impl CommandsHandler for CommandsConfig {
    async fn handle_cancel_reservation(
        &self,
        _cmd: CancelReservation,
    ) -> Result<CommandResponse, ServerError> {
        Ok(Self::not_supported_response())
    }

    async fn handle_reserve_now(&self, _cmd: ReserveNow) -> Result<CommandResponse, ServerError> {
        Ok(Self::not_supported_response())
    }

    async fn handle_start_session(
        &self,
        _cmd: StartSession,
    ) -> Result<CommandResponse, ServerError> {
        Ok(Self::not_supported_response())
    }

    async fn handle_stop_session(&self, _cmd: StopSession) -> Result<CommandResponse, ServerError> {
        Ok(Self::not_supported_response())
    }

    async fn handle_unlock_connector(
        &self,
        _cmd: UnlockConnector,
    ) -> Result<CommandResponse, ServerError> {
        Ok(Self::not_supported_response())
    }

    async fn receive_command_result(
        &self,
        _command_type: CommandType,
        _result: CommandResult,
    ) -> Result<(), ServerError> {
        Ok(())
    }
}

// ── Commands2111Handler (OCPI 2.1.1) ────────────────────────────────────────────

/// Handles the OCPI **2.1.1** Commands module endpoints.
///
/// Implements the **receiver** interface (CPO receives commands from an eMSP)
/// and the **sender** interface (eMSP receives the async result callback from
/// the CPO).
///
/// The two-phase flow: the eMSP POSTs a command to the CPO (receiver), the CPO
/// acknowledges immediately with a [`CommandResponse2111`], then asynchronously
/// POSTs a second [`CommandResponse2111`] back to the `response_url` the eMSP
/// included in the command.
///
/// ## Delta from the 2.2.1 [`CommandsHandler`]
///
/// - **No `CANCEL_RESERVATION`** — that command (and its `CancelReservation`
///   body) arrived in OCPI 2.2, so a 2.1.1 peer never emits it and there is no
///   `handle_cancel_reservation` method here.
/// - **[`StartSession2111`] carries the full [`Token2111`] object**, not a token
///   reference.
/// - **The async result is a [`CommandResponse2111`]**, not a distinct
///   `CommandResult` — 2.1.1 reuses the single `CommandResponse` object for both
///   the synchronous ack and the async callback (§13.2.2.1).
///
/// The 2.1.1 receiver path is **flat** (`/commands/{command}`) — Commands is a
/// verb-style RPC keyed by the Sender-supplied `response_url`, so there are no
/// `{country_code}/{party_id}` segments, exactly as in 2.2.1.
///
/// Spec: OCPI 2.1.1 — *Commands* module (§13),
/// `specs/ocpi/2.1.1/OCPI_2.1.1.pdf`.
#[allow(async_fn_in_trait)]
pub trait Commands2111Handler {
    /// Receive a `RESERVE_NOW` command — receiver interface
    /// (`POST /commands/RESERVE_NOW`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the command cannot be forwarded to the Charge Point.
    async fn handle_reserve_now(
        &self,
        cmd: ReserveNow2111,
    ) -> Result<CommandResponse2111, ServerError>;

    /// Receive a `START_SESSION` command — receiver interface
    /// (`POST /commands/START_SESSION`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the command cannot be forwarded to the Charge Point.
    async fn handle_start_session(
        &self,
        cmd: StartSession2111,
    ) -> Result<CommandResponse2111, ServerError>;

    /// Receive a `STOP_SESSION` command — receiver interface
    /// (`POST /commands/STOP_SESSION`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the command cannot be forwarded to the Charge Point.
    async fn handle_stop_session(
        &self,
        cmd: StopSession2111,
    ) -> Result<CommandResponse2111, ServerError>;

    /// Receive an `UNLOCK_CONNECTOR` command — receiver interface
    /// (`POST /commands/UNLOCK_CONNECTOR`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the command cannot be forwarded to the Charge Point.
    async fn handle_unlock_connector(
        &self,
        cmd: UnlockConnector2111,
    ) -> Result<CommandResponse2111, ServerError>;

    /// Receive the asynchronous result from the Charge Point — sender interface
    /// (`POST /commands/{command_type}/result`).
    ///
    /// The CPO delivers this after the Charge Point has executed (or failed to
    /// execute) the command. The `response_url` in each command object points
    /// here. In 2.1.1 the body is a [`CommandResponse2111`] (not a `CommandResult`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the result cannot be processed.
    async fn receive_command_result(
        &self,
        command_type: CommandType2111,
        result: CommandResponse2111,
    ) -> Result<(), ServerError>;
}

// ── Commands2111Config (OCPI 2.1.1) ─────────────────────────────────────────────

/// Stateless placeholder **OCPI 2.1.1** Commands handler for use with
/// [`http::commands_2_1_1_router`].
///
/// Returns [`CommandResponseType2111::NotSupported`] for every incoming command.
/// Replace with a concrete bridge implementation when real CPO/OCPP integration
/// is needed; implement [`Commands2111Handler`] on your own type and wire it to
/// an axum state of `Arc<YourType>`.
#[derive(Debug, Default)]
pub struct Commands2111Config;

impl Commands2111Config {
    /// Create a new `Commands2111Config` placeholder.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Returns the default "not supported" [`CommandResponse2111`].
    ///
    /// Used by the placeholder implementation and useful as a starting point
    /// when overriding specific commands. The 2.1.1 `CommandResponse` carries
    /// only `result` (no 2.2 `timeout`/`message` fields).
    #[must_use]
    pub fn not_supported_response() -> CommandResponse2111 {
        CommandResponse2111 {
            result: CommandResponseType2111::NotSupported,
        }
    }
}

#[allow(async_fn_in_trait)]
impl Commands2111Handler for Commands2111Config {
    async fn handle_reserve_now(
        &self,
        _cmd: ReserveNow2111,
    ) -> Result<CommandResponse2111, ServerError> {
        Ok(Self::not_supported_response())
    }

    async fn handle_start_session(
        &self,
        _cmd: StartSession2111,
    ) -> Result<CommandResponse2111, ServerError> {
        Ok(Self::not_supported_response())
    }

    async fn handle_stop_session(
        &self,
        _cmd: StopSession2111,
    ) -> Result<CommandResponse2111, ServerError> {
        Ok(Self::not_supported_response())
    }

    async fn handle_unlock_connector(
        &self,
        _cmd: UnlockConnector2111,
    ) -> Result<CommandResponse2111, ServerError> {
        Ok(Self::not_supported_response())
    }

    async fn receive_command_result(
        &self,
        _command_type: CommandType2111,
        _result: CommandResponse2111,
    ) -> Result<(), ServerError> {
        Ok(())
    }
}

// ── Commands22Handler (OCPI 2.2) ────────────────────────────────────────────────

/// Handles the OCPI **2.2** Commands module endpoints.
///
/// Implements the **receiver** interface (CPO receives commands from an eMSP)
/// and the **sender** interface (eMSP receives the async `CommandResult`
/// callback from the CPO), exactly like the 2.2.1 [`CommandsHandler`].
///
/// ## Delta from the 2.2.1 [`CommandsHandler`]
///
/// The two versions are wire-identical **except** for `START_SESSION`:
/// [`handle_start_session`](Commands22Handler::handle_start_session) takes a
/// [`StartSession22`] (`ocpi_types::v2_2::StartSession`), which has **no**
/// `connector_id` — that field arrived in 2.2.1 (together with the
/// `START_SESSION_CONNECTOR_REQUIRED` EVSE capability). In OCPI 2.2 a
/// `START_SESSION` targets a Location, optionally narrowed to an EVSE, and the
/// Charge Point picks the connector. Landing the command in the 2.2 type means a
/// stray `connector_id` from a non-conformant peer is **not** carried into the
/// session rather than being silently honoured. Every other command body
/// (`CancelReservation`, `ReserveNow`, `StopSession`, `UnlockConnector`) and the
/// `CommandResponse`/`CommandResult` objects are the re-exported 2.2.1 types.
///
/// The 2.2 receiver path is **flat** (`/commands/{command}`) — Commands is a
/// verb-style RPC keyed by the Sender-supplied `response_url`, identical to
/// 2.2.1.
///
/// Spec: `specs/ocpi/2.2/mod_commands.asciidoc`.
#[allow(async_fn_in_trait)]
pub trait Commands22Handler {
    /// Receive a `CANCEL_RESERVATION` command — receiver interface
    /// (`POST /commands/CANCEL_RESERVATION`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the command cannot be forwarded to the Charge Point.
    async fn handle_cancel_reservation(
        &self,
        cmd: CancelReservation,
    ) -> Result<CommandResponse, ServerError>;

    /// Receive a `RESERVE_NOW` command — receiver interface
    /// (`POST /commands/RESERVE_NOW`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the command cannot be forwarded to the Charge Point.
    async fn handle_reserve_now(&self, cmd: ReserveNow) -> Result<CommandResponse, ServerError>;

    /// Receive a `START_SESSION` command — receiver interface
    /// (`POST /commands/START_SESSION`).
    ///
    /// Takes the **2.2** [`StartSession22`] — no `connector_id`.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the command cannot be forwarded to the Charge Point.
    async fn handle_start_session(
        &self,
        cmd: StartSession22,
    ) -> Result<CommandResponse, ServerError>;

    /// Receive a `STOP_SESSION` command — receiver interface
    /// (`POST /commands/STOP_SESSION`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the command cannot be forwarded to the Charge Point.
    async fn handle_stop_session(&self, cmd: StopSession) -> Result<CommandResponse, ServerError>;

    /// Receive an `UNLOCK_CONNECTOR` command — receiver interface
    /// (`POST /commands/UNLOCK_CONNECTOR`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the command cannot be forwarded to the Charge Point.
    async fn handle_unlock_connector(
        &self,
        cmd: UnlockConnector,
    ) -> Result<CommandResponse, ServerError>;

    /// Receive the asynchronous result from the Charge Point — sender interface
    /// (`POST /commands/{command_type}/result`).
    ///
    /// The CPO delivers this after the Charge Point has executed (or failed to
    /// execute) the command. The `response_url` in each command object points here.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the result cannot be processed.
    async fn receive_command_result(
        &self,
        command_type: CommandType,
        result: CommandResult,
    ) -> Result<(), ServerError>;
}

// ── Commands22Config (OCPI 2.2) ─────────────────────────────────────────────────

/// Stateless placeholder **OCPI 2.2** Commands handler for use with
/// [`http::commands_2_2_router`].
///
/// Returns [`CommandResponseType::NotSupported`] for every incoming command.
/// Replace with a concrete bridge implementation when real CPO/OCPP integration
/// is needed; implement [`Commands22Handler`] on your own type and wire it to an
/// axum state of `Arc<YourType>`.
#[derive(Debug, Default)]
pub struct Commands22Config;

impl Commands22Config {
    /// Create a new `Commands22Config` placeholder.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Returns the default "not supported" [`CommandResponse`].
    ///
    /// Used by the placeholder implementation and useful as a starting point
    /// when overriding specific commands. The 2.2 `CommandResponse` is
    /// wire-identical to 2.2.1 (`result` + `timeout` + `message`).
    #[must_use]
    pub fn not_supported_response() -> CommandResponse {
        CommandResponse {
            result: CommandResponseType::NotSupported,
            timeout: 30,
            message: vec![],
        }
    }
}

#[allow(async_fn_in_trait)]
impl Commands22Handler for Commands22Config {
    async fn handle_cancel_reservation(
        &self,
        _cmd: CancelReservation,
    ) -> Result<CommandResponse, ServerError> {
        Ok(Self::not_supported_response())
    }

    async fn handle_reserve_now(&self, _cmd: ReserveNow) -> Result<CommandResponse, ServerError> {
        Ok(Self::not_supported_response())
    }

    async fn handle_start_session(
        &self,
        _cmd: StartSession22,
    ) -> Result<CommandResponse, ServerError> {
        Ok(Self::not_supported_response())
    }

    async fn handle_stop_session(&self, _cmd: StopSession) -> Result<CommandResponse, ServerError> {
        Ok(Self::not_supported_response())
    }

    async fn handle_unlock_connector(
        &self,
        _cmd: UnlockConnector,
    ) -> Result<CommandResponse, ServerError> {
        Ok(Self::not_supported_response())
    }

    async fn receive_command_result(
        &self,
        _command_type: CommandType,
        _result: CommandResult,
    ) -> Result<(), ServerError> {
        Ok(())
    }
}

// ── ChargingProfilesHandler ───────────────────────────────────────────────────

/// Handles the OCPI ChargingProfiles module endpoints.
///
/// Implements the **receiver** interface (typically the CPO — receives
/// ChargingProfile requests for an ongoing session) and the **sender** interface
/// (typically the SCSP/eMSP — receives the asynchronous Charge Point results).
///
/// Like Commands, the flow is two-phase: the Sender requests/sets/clears a
/// profile via the receiver interface, the Receiver acknowledges immediately
/// with a [`ChargingProfileResponse`], then asynchronously POSTs the final
/// Charge Point result back to the `response_url` the Sender supplied.
///
/// Spec: `specs/ocpi/2.2.1/mod_charging_profiles.asciidoc`
#[allow(async_fn_in_trait)]
pub trait ChargingProfilesHandler {
    /// Get the currently planned `ActiveChargingProfile` for a session — receiver
    /// interface (`GET /chargingprofiles/{session_id}?duration={n}&response_url={url}`).
    ///
    /// The returned [`ChargingProfileResponse`] is only the Receiver's
    /// acknowledgment; the `ActiveChargingProfileResult` is delivered later via
    /// POST on `response_url`.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the request cannot be forwarded to the Charge Point.
    async fn get_active_profile(
        &self,
        session_id: &str,
        duration: u32,
        response_url: &str,
    ) -> Result<ChargingProfileResponse, ServerError>;

    /// Create or update a ChargingProfile on a session — receiver interface
    /// (`PUT /chargingprofiles/{session_id}`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the request cannot be forwarded to the Charge Point.
    async fn set_charging_profile(
        &self,
        session_id: &str,
        profile: SetChargingProfile,
    ) -> Result<ChargingProfileResponse, ServerError>;

    /// Cancel/clear an existing ChargingProfile on a session — receiver interface
    /// (`DELETE /chargingprofiles/{session_id}?response_url={url}`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the request cannot be forwarded to the Charge Point.
    async fn clear_charging_profile(
        &self,
        session_id: &str,
        response_url: &str,
    ) -> Result<ChargingProfileResponse, ServerError>;

    /// Receive an asynchronous `ActiveChargingProfileResult` — sender interface
    /// (`POST /chargingprofiles/{session_id}/activeprofile`), the result of a
    /// prior `get_active_profile` request.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the result cannot be processed.
    async fn receive_active_profile_result(
        &self,
        result: ActiveChargingProfileResult,
    ) -> Result<(), ServerError>;

    /// Receive an asynchronous `ChargingProfileResult` — sender interface
    /// (`POST /chargingprofiles/{session_id}/result`), the result of a prior
    /// `set_charging_profile` request.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the result cannot be processed.
    async fn receive_charging_profile_result(
        &self,
        result: ChargingProfileResult,
    ) -> Result<(), ServerError>;

    /// Receive an asynchronous `ClearProfileResult` — sender interface
    /// (`POST /chargingprofiles/{session_id}/clearprofile`), the result of a prior
    /// `clear_charging_profile` request.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the result cannot be processed.
    async fn receive_clear_profile_result(
        &self,
        result: ClearProfileResult,
    ) -> Result<(), ServerError>;

    /// Receive a proactively-pushed `ActiveChargingProfile` update — sender
    /// interface (`PUT /chargingprofiles/{session_id}`).
    ///
    /// The Receiver (typically CPO) calls this whenever it learns the
    /// `ActiveChargingProfile` for an ongoing session has changed — but only once
    /// the Sender has at least once successfully set a profile for that session
    /// via the receiver `PUT` (`SetChargingProfile`). Unlike the three `POST`
    /// result callbacks, this is *not* a response to a prior Sender request; it is
    /// an unsolicited update keyed by `session_id`.
    ///
    /// Spec: `mod_charging_profiles.asciidoc` §Sender Interface,
    /// `mod_charging_profiles_msp_put_method`.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the update cannot be processed.
    async fn receive_active_profile_update(
        &self,
        session_id: &str,
        profile: ActiveChargingProfile,
    ) -> Result<(), ServerError>;
}

// ── ChargingProfilesConfig ────────────────────────────────────────────────────

/// Stateless placeholder ChargingProfiles handler for use with
/// [`http::charging_profiles_router`].
///
/// Returns [`ChargingProfileResponseType::NotSupported`] for every receiver
/// request, and accepts (no-ops) every asynchronous result callback. Replace it
/// with a concrete bridge implementation when real CPO/OCPP smart-charging
/// integration is needed: implement [`ChargingProfilesHandler`] on your own type
/// and wire it to an axum state of `Arc<YourType>`.
#[derive(Debug, Default)]
pub struct ChargingProfilesConfig;

impl ChargingProfilesConfig {
    /// Create a new `ChargingProfilesConfig` placeholder.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Returns the default "not supported" [`ChargingProfileResponse`].
    ///
    /// Used by the placeholder implementation and useful as a starting point
    /// when overriding specific receiver methods.
    #[must_use]
    pub fn not_supported_response() -> ChargingProfileResponse {
        ChargingProfileResponse {
            result: ChargingProfileResponseType::NotSupported,
            timeout: 0,
        }
    }
}

#[allow(async_fn_in_trait)]
impl ChargingProfilesHandler for ChargingProfilesConfig {
    async fn get_active_profile(
        &self,
        _session_id: &str,
        _duration: u32,
        _response_url: &str,
    ) -> Result<ChargingProfileResponse, ServerError> {
        Ok(Self::not_supported_response())
    }

    async fn set_charging_profile(
        &self,
        _session_id: &str,
        _profile: SetChargingProfile,
    ) -> Result<ChargingProfileResponse, ServerError> {
        Ok(Self::not_supported_response())
    }

    async fn clear_charging_profile(
        &self,
        _session_id: &str,
        _response_url: &str,
    ) -> Result<ChargingProfileResponse, ServerError> {
        Ok(Self::not_supported_response())
    }

    async fn receive_active_profile_result(
        &self,
        _result: ActiveChargingProfileResult,
    ) -> Result<(), ServerError> {
        Ok(())
    }

    async fn receive_charging_profile_result(
        &self,
        _result: ChargingProfileResult,
    ) -> Result<(), ServerError> {
        Ok(())
    }

    async fn receive_clear_profile_result(
        &self,
        _result: ClearProfileResult,
    ) -> Result<(), ServerError> {
        Ok(())
    }

    async fn receive_active_profile_update(
        &self,
        _session_id: &str,
        _profile: ActiveChargingProfile,
    ) -> Result<(), ServerError> {
        Ok(())
    }
}

// ── HubClientInfoHandler ──────────────────────────────────────────────────────

/// Handles the OCPI HubClientInfo module endpoints.
///
/// This is a **Configuration Module** — OCPI routing headers
/// (`OCPI-to/from-party-id/country-code`) are **not** used on these endpoints.
///
/// The Hub pushes `ClientInfo` objects to connected parties (Receiver
/// interface: PUT/GET). Connected parties can also pull the full list from
/// the Hub (Sender interface: GET paginated).
///
/// Spec: `specs/ocpi/2.2.1/mod_hub_client_info.asciidoc`
#[allow(async_fn_in_trait)]
pub trait HubClientInfoHandler {
    /// Retrieve a single `ClientInfo` object as stored in the local system.
    ///
    /// Receiver interface: `GET /clientinfo/{country_code}/{party_id}`.
    ///
    /// Returns `Ok(None)` when no entry exists for that party.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] on storage failure.
    async fn get_client_info(
        &self,
        country_code: &str,
        party_id: &str,
    ) -> Result<Option<ClientInfo>, ServerError>;

    /// Store a new or updated `ClientInfo` object pushed by the Hub.
    ///
    /// Receiver interface: `PUT /clientinfo/{country_code}/{party_id}`.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] on storage failure.
    async fn put_client_info(
        &self,
        country_code: &str,
        party_id: &str,
        info: ClientInfo,
    ) -> Result<(), ServerError>;
}

// ── HubClientInfoConfig ───────────────────────────────────────────────────────

/// Thread-safe in-memory HubClientInfo store for use with
/// [`http::hub_client_info_router`].
///
/// Entries are keyed by `"{country_code}/{party_id}"`. Wrap in `Arc` to share
/// across axum handlers or multiple threads.
pub struct HubClientInfoConfig {
    entries: std::sync::RwLock<std::collections::HashMap<String, ClientInfo>>,
}

impl std::fmt::Debug for HubClientInfoConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HubClientInfoConfig")
            .field(
                "entry_count",
                &self.entries.read().map(|m| m.len()).unwrap_or(0),
            )
            .finish()
    }
}

impl HubClientInfoConfig {
    /// Create an empty HubClientInfo store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    fn composite_key(country_code: &str, party_id: &str) -> String {
        format!("{country_code}/{party_id}")
    }

    /// Insert or replace a `ClientInfo` entry.
    pub fn put(&self, country_code: &str, party_id: &str, info: ClientInfo) {
        let key = Self::composite_key(country_code, party_id);
        self.entries
            .write()
            .expect("lock not poisoned")
            .insert(key, info);
    }

    /// Retrieve a `ClientInfo` entry by its key.
    #[must_use]
    pub fn get(&self, country_code: &str, party_id: &str) -> Option<ClientInfo> {
        let key = Self::composite_key(country_code, party_id);
        self.entries
            .read()
            .expect("lock not poisoned")
            .get(&key)
            .cloned()
    }

    /// Return a filtered and paginated slice of all `ClientInfo` entries.
    ///
    /// Filters by `last_updated >= date_from` and (if provided)
    /// `last_updated < date_to`. Results are sorted by `last_updated`.
    ///
    /// Returns `(page_items, total_matching_count)`.
    #[must_use]
    pub fn list(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> (Vec<ClientInfo>, u32) {
        let map = self.entries.read().expect("lock not poisoned");
        let mut filtered: Vec<&ClientInfo> = map
            .values()
            .filter(|c| c.last_updated >= date_from && date_to.is_none_or(|dt| c.last_updated < dt))
            .collect();
        filtered.sort_by_key(|c| c.last_updated);
        let total = filtered.len() as u32;
        let page: Vec<ClientInfo> = filtered
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect();
        (page, total)
    }
}

impl Default for HubClientInfoConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(async_fn_in_trait)]
impl HubClientInfoHandler for HubClientInfoConfig {
    async fn get_client_info(
        &self,
        country_code: &str,
        party_id: &str,
    ) -> Result<Option<ClientInfo>, ServerError> {
        Ok(self.get(country_code, party_id))
    }

    async fn put_client_info(
        &self,
        country_code: &str,
        party_id: &str,
        info: ClientInfo,
    ) -> Result<(), ServerError> {
        self.put(country_code, party_id, info);
        Ok(())
    }
}

// ── LocationsHandler ──────────────────────────────────────────────────────────

/// Handles the OCPI Locations module **receiver** interface (the eMSP side that
/// receives Location/EVSE/Connector data pushed by a CPO).
///
/// The receiver addresses objects by their composite key
/// `{country_code}/{party_id}/{location_id}[/{evse_uid}][/{connector_id}]`, where
/// `country_code` and `party_id` identify the CPO that owns the Location. EVSEs
/// and Connectors are nested inside their parent Location, so the sub-object
/// methods locate the parent first and mutate it in place.
///
/// The paginated [`list_locations`](Self::list_locations) method serves the CPO
/// **sender** interface (`GET /locations`) for parity with the other module
/// handlers.
///
/// Spec: `specs/ocpi/2.2.1/mod_locations.asciidoc` — §Receiver Interface.
#[allow(async_fn_in_trait)]
pub trait LocationsHandler {
    /// Paginated list of locations whose `last_updated` is in
    /// `[date_from, date_to)` — sender interface (`GET /locations`).
    ///
    /// Returns `(page_items, total_count)`.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] if the query cannot be executed.
    async fn list_locations(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> Result<(Vec<Location>, u32), ServerError>;

    /// Fetch a single Location by its composite key (`GET`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] when the Location does not exist.
    async fn get_location(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
    ) -> Result<Location, ServerError>;

    /// Create or replace a Location (`PUT`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError`] on storage failure.
    async fn put_location(&self, location: Location) -> Result<(), ServerError>;

    /// Apply a JSON merge-patch (RFC 7396) to an existing Location (`PATCH`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] when the Location does not exist.
    async fn patch_location(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError>;

    /// Fetch a single EVSE nested in a Location (`GET`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] when the Location or EVSE is unknown.
    async fn get_evse(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
    ) -> Result<Evse, ServerError>;

    /// Create or replace an EVSE nested in a Location (`PUT`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] when the parent Location is unknown.
    async fn put_evse(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse: Evse,
    ) -> Result<(), ServerError>;

    /// Apply a JSON merge-patch to a nested EVSE (`PATCH`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] when the Location or EVSE is unknown.
    async fn patch_evse(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError>;

    /// Fetch a single Connector nested in an EVSE (`GET`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] when the Location, EVSE, or Connector is
    /// unknown.
    async fn get_connector(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
        connector_id: &str,
    ) -> Result<Connector, ServerError>;

    /// Create or replace a Connector nested in an EVSE (`PUT`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] when the parent Location or EVSE is
    /// unknown.
    async fn put_connector(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
        connector: Connector,
    ) -> Result<(), ServerError>;

    /// Apply a JSON merge-patch to a nested Connector (`PATCH`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] when the Location, EVSE, or Connector is
    /// unknown.
    async fn patch_connector(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
        connector_id: &str,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError>;
}

// ── LocationsConfig ───────────────────────────────────────────────────────────

/// Thread-safe in-memory Locations store for use with [`http::locations_router`].
///
/// Locations are keyed by `"{country_code}/{party_id}/{location_id}"`. EVSEs and
/// Connectors are stored nested inside their parent [`Location`] (matching the
/// OCPI object model), so sub-object writes mutate the owning Location in place.
/// Wrap in `Arc` to share across axum handlers or multiple threads.
pub struct LocationsConfig {
    locations: std::sync::RwLock<std::collections::HashMap<String, Location>>,
}

impl std::fmt::Debug for LocationsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocationsConfig")
            .field(
                "location_count",
                &self.locations.read().map(|m| m.len()).unwrap_or(0),
            )
            .finish()
    }
}

/// RFC 7396 merge-patch over a serde value: serialize `current`, apply `partial`,
/// then deserialize back into `T`.
fn apply_merge_patch<T>(
    current: &T,
    partial: ocpi_types::serde_json::Value,
) -> Result<T, ServerError>
where
    T: ocpi_types::serde::Serialize + ocpi_types::serde::de::DeserializeOwned,
{
    let mut base = ocpi_types::serde_json::to_value(current)
        .map_err(|_| ServerError::NotImplemented("patch serialize"))?;
    json_merge(&mut base, partial);
    ocpi_types::serde_json::from_value(base)
        .map_err(|_| ServerError::NotImplemented("patch deserialize"))
}

impl LocationsConfig {
    /// Create an empty Locations store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            locations: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    fn composite_key(country_code: &str, party_id: &str, location_id: &str) -> String {
        format!("{country_code}/{party_id}/{location_id}")
    }

    /// Insert or replace a Location, keyed by its own `country_code`, `party_id`,
    /// and `id`.
    pub fn put(&self, location: Location) {
        let key = Self::composite_key(
            location.country_code.as_str(),
            location.party_id.as_str(),
            location.id.as_str(),
        );
        self.locations
            .write()
            .expect("lock not poisoned")
            .insert(key, location);
    }

    /// Retrieve a Location by its composite key.
    #[must_use]
    pub fn get(&self, country_code: &str, party_id: &str, location_id: &str) -> Option<Location> {
        let key = Self::composite_key(country_code, party_id, location_id);
        self.locations
            .read()
            .expect("lock not poisoned")
            .get(&key)
            .cloned()
    }

    /// Retrieve a nested EVSE by its `uid`.
    #[must_use]
    pub fn get_evse(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
    ) -> Option<Evse> {
        self.get(country_code, party_id, location_id)?
            .evses
            .into_iter()
            .find(|e| e.uid.as_str() == evse_uid)
    }

    /// Retrieve a nested Connector by its `id`.
    #[must_use]
    pub fn get_connector(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
        connector_id: &str,
    ) -> Option<Connector> {
        self.get_evse(country_code, party_id, location_id, evse_uid)?
            .connectors
            .into_iter()
            .find(|c| c.id.as_str() == connector_id)
    }

    /// Apply a JSON merge-patch to an existing Location.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] if no Location matches the key.
    pub fn patch_location(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError> {
        let key = Self::composite_key(country_code, party_id, location_id);
        let mut map = self.locations.write().expect("lock not poisoned");
        let current = map.get(&key).ok_or(ServerError::NotFound)?;
        let updated = apply_merge_patch(current, partial)?;
        map.insert(key, updated);
        Ok(())
    }

    /// Insert or replace a nested EVSE inside an existing Location.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] if the parent Location is unknown.
    pub fn put_evse(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse: Evse,
    ) -> Result<(), ServerError> {
        let key = Self::composite_key(country_code, party_id, location_id);
        let mut map = self.locations.write().expect("lock not poisoned");
        let location = map.get_mut(&key).ok_or(ServerError::NotFound)?;
        upsert_by(&mut location.evses, evse, |e| e.uid.as_str().to_owned());
        Ok(())
    }

    /// Apply a JSON merge-patch to a nested EVSE.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] if the Location or EVSE is unknown.
    pub fn patch_evse(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError> {
        let key = Self::composite_key(country_code, party_id, location_id);
        let mut map = self.locations.write().expect("lock not poisoned");
        let location = map.get_mut(&key).ok_or(ServerError::NotFound)?;
        let evse = location
            .evses
            .iter_mut()
            .find(|e| e.uid.as_str() == evse_uid)
            .ok_or(ServerError::NotFound)?;
        *evse = apply_merge_patch(evse, partial)?;
        Ok(())
    }

    /// Insert or replace a nested Connector inside an existing EVSE.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] if the Location or EVSE is unknown.
    pub fn put_connector(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
        connector: Connector,
    ) -> Result<(), ServerError> {
        let key = Self::composite_key(country_code, party_id, location_id);
        let mut map = self.locations.write().expect("lock not poisoned");
        let location = map.get_mut(&key).ok_or(ServerError::NotFound)?;
        let evse = location
            .evses
            .iter_mut()
            .find(|e| e.uid.as_str() == evse_uid)
            .ok_or(ServerError::NotFound)?;
        upsert_by(&mut evse.connectors, connector, |c| {
            c.id.as_str().to_owned()
        });
        Ok(())
    }

    /// Apply a JSON merge-patch to a nested Connector.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::NotFound`] if the Location, EVSE, or Connector is
    /// unknown.
    pub fn patch_connector(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
        connector_id: &str,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError> {
        let key = Self::composite_key(country_code, party_id, location_id);
        let mut map = self.locations.write().expect("lock not poisoned");
        let location = map.get_mut(&key).ok_or(ServerError::NotFound)?;
        let evse = location
            .evses
            .iter_mut()
            .find(|e| e.uid.as_str() == evse_uid)
            .ok_or(ServerError::NotFound)?;
        let connector = evse
            .connectors
            .iter_mut()
            .find(|c| c.id.as_str() == connector_id)
            .ok_or(ServerError::NotFound)?;
        *connector = apply_merge_patch(connector, partial)?;
        Ok(())
    }

    /// Return a filtered and paginated slice of Locations.
    ///
    /// Filters by `last_updated >= date_from` and (if provided)
    /// `last_updated < date_to`. Results are sorted by `last_updated`.
    ///
    /// Returns `(page_items, total_matching_count)`.
    #[must_use]
    pub fn list(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> (Vec<Location>, u32) {
        let map = self.locations.read().expect("lock not poisoned");
        let mut filtered: Vec<&Location> = map
            .values()
            .filter(|l| l.last_updated >= date_from && date_to.is_none_or(|dt| l.last_updated < dt))
            .collect();
        filtered.sort_by_key(|l| l.last_updated);
        let total = filtered.len() as u32;
        let page: Vec<Location> = filtered
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect();
        (page, total)
    }
}

/// Replace the element of `items` whose key matches `incoming`'s, or append it.
fn upsert_by<T, F>(items: &mut Vec<T>, incoming: T, key: F)
where
    F: Fn(&T) -> String,
{
    let incoming_key = key(&incoming);
    if let Some(slot) = items.iter_mut().find(|item| key(item) == incoming_key) {
        *slot = incoming;
    } else {
        items.push(incoming);
    }
}

impl Default for LocationsConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(async_fn_in_trait)]
impl LocationsHandler for LocationsConfig {
    async fn list_locations(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> Result<(Vec<Location>, u32), ServerError> {
        Ok(self.list(date_from, date_to, offset, limit))
    }

    async fn get_location(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
    ) -> Result<Location, ServerError> {
        self.get(country_code, party_id, location_id)
            .ok_or(ServerError::NotFound)
    }

    async fn put_location(&self, location: Location) -> Result<(), ServerError> {
        self.put(location);
        Ok(())
    }

    async fn patch_location(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError> {
        self.patch_location(country_code, party_id, location_id, partial)
    }

    async fn get_evse(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
    ) -> Result<Evse, ServerError> {
        self.get_evse(country_code, party_id, location_id, evse_uid)
            .ok_or(ServerError::NotFound)
    }

    async fn put_evse(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse: Evse,
    ) -> Result<(), ServerError> {
        self.put_evse(country_code, party_id, location_id, evse)
    }

    async fn patch_evse(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError> {
        self.patch_evse(country_code, party_id, location_id, evse_uid, partial)
    }

    async fn get_connector(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
        connector_id: &str,
    ) -> Result<Connector, ServerError> {
        self.get_connector(country_code, party_id, location_id, evse_uid, connector_id)
            .ok_or(ServerError::NotFound)
    }

    async fn put_connector(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
        connector: Connector,
    ) -> Result<(), ServerError> {
        self.put_connector(country_code, party_id, location_id, evse_uid, connector)
    }

    async fn patch_connector(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
        connector_id: &str,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError> {
        self.patch_connector(
            country_code,
            party_id,
            location_id,
            evse_uid,
            connector_id,
            partial,
        )
    }
}

// ── Locations2111Handler (OCPI 2.1.1) ───────────────────────────────────────────

/// Handles the OCPI **2.1.1** Locations module **receiver** interface (the eMSP
/// side that receives Location/EVSE/Connector data pushed by a CPO).
///
/// ## Delta from the 2.2.1 [`LocationsHandler`]
///
/// The transport is identical — Locations is a **client-owned** object (§2.2
/// eMSP Interface), so the receiver addresses objects by their composite key
/// `{country_code}/{party_id}/{location_id}[/{evse_uid}][/{connector_id}]`,
/// keeping the `{country_code}/{party_id}` segments exactly as in 2.2.1. Only
/// the object shape differs: the 2.1.1 [`Location2111`] carries a required
/// `type`, **no** `country_code`/`party_id` on the object itself (they live only
/// in the URL), and a singular `Connector.tariff_id`. Because the object does
/// not carry its owner identity, [`put_location`](Self::put_location) takes the
/// `country_code`/`party_id` from the URL, unlike the 2.2.1 variant.
///
/// This is the eMSP **receiver** only; the CPO sender getters (`GET /locations`)
/// are the 2.1.1 client methods (`OcpiClient::get_locations_2_1_1`, #113/#118).
///
/// Spec: OCPI 2.1.1 — *Locations* module, §2.2 eMSP Interface.
///
/// Every method returns [`ServerError::NotFound`] (→ HTTP 404, OCPI `2003`) when
/// the addressed Location, EVSE, or Connector is unknown.
#[allow(async_fn_in_trait)]
pub trait Locations2111Handler {
    /// Fetch a single Location by its composite key (`GET`).
    async fn get_location(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
    ) -> Result<Location2111, ServerError>;

    /// Create or replace a Location (`PUT`).
    async fn put_location(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        location: Location2111,
    ) -> Result<(), ServerError>;

    /// Apply a JSON merge-patch (RFC 7396) to an existing Location (`PATCH`).
    async fn patch_location(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError>;

    /// Fetch a single EVSE nested in a Location (`GET`).
    async fn get_evse(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
    ) -> Result<Evse2111, ServerError>;

    /// Create or replace an EVSE nested in a Location (`PUT`).
    async fn put_evse(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse: Evse2111,
    ) -> Result<(), ServerError>;

    /// Apply a JSON merge-patch to a nested EVSE (`PATCH`).
    async fn patch_evse(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError>;

    /// Fetch a single Connector nested in an EVSE (`GET`).
    async fn get_connector(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
        connector_id: &str,
    ) -> Result<Connector2111, ServerError>;

    /// Create or replace a Connector nested in an EVSE (`PUT`).
    async fn put_connector(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
        connector: Connector2111,
    ) -> Result<(), ServerError>;

    /// Apply a JSON merge-patch to a nested Connector (`PATCH`).
    async fn patch_connector(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
        connector_id: &str,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError>;
}

// ── Locations2111Config (OCPI 2.1.1) ─────────────────────────────────────────────

/// Thread-safe in-memory **OCPI 2.1.1** Locations store for use with
/// [`http::locations_2_1_1_router`].
///
/// Mirrors [`LocationsConfig`] but stores the 2.1.1 [`Location2111`] shape.
/// Locations are keyed by `"{country_code}/{party_id}/{location_id}"` — the
/// owner identity comes from the URL, since the 2.1.1 object does not carry
/// `country_code`/`party_id`. EVSEs and Connectors are stored nested inside
/// their parent Location. Wrap in `Arc` to share across axum handlers.
pub struct Locations2111Config {
    locations: std::sync::RwLock<std::collections::HashMap<String, Location2111>>,
}

impl std::fmt::Debug for Locations2111Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Locations2111Config")
            .field(
                "location_count",
                &self.locations.read().map(|m| m.len()).unwrap_or(0),
            )
            .finish()
    }
}

impl Locations2111Config {
    /// Create an empty 2.1.1 Locations store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            locations: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    fn composite_key(country_code: &str, party_id: &str, location_id: &str) -> String {
        format!("{country_code}/{party_id}/{location_id}")
    }

    /// Insert or replace a Location, keyed by the URL segments (the 2.1.1 object
    /// has no `country_code`/`party_id` of its own).
    pub fn put(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        location: Location2111,
    ) {
        let key = Self::composite_key(country_code, party_id, location_id);
        self.locations
            .write()
            .expect("lock not poisoned")
            .insert(key, location);
    }

    /// Retrieve a Location by its composite key.
    #[must_use]
    pub fn get(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
    ) -> Option<Location2111> {
        let key = Self::composite_key(country_code, party_id, location_id);
        self.locations
            .read()
            .expect("lock not poisoned")
            .get(&key)
            .cloned()
    }

    /// Retrieve a nested EVSE by its `uid`.
    #[must_use]
    pub fn get_evse(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
    ) -> Option<Evse2111> {
        self.get(country_code, party_id, location_id)?
            .evses
            .into_iter()
            .find(|e| e.uid.as_str() == evse_uid)
    }

    /// Retrieve a nested Connector by its `id`.
    #[must_use]
    pub fn get_connector(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
        connector_id: &str,
    ) -> Option<Connector2111> {
        self.get_evse(country_code, party_id, location_id, evse_uid)?
            .connectors
            .into_iter()
            .find(|c| c.id.as_str() == connector_id)
    }

    /// Apply a JSON merge-patch to an existing Location (`NotFound` if absent).
    pub fn patch_location(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError> {
        let key = Self::composite_key(country_code, party_id, location_id);
        let mut map = self.locations.write().expect("lock not poisoned");
        let current = map.get(&key).ok_or(ServerError::NotFound)?;
        let updated = apply_merge_patch(current, partial)?;
        map.insert(key, updated);
        Ok(())
    }

    /// Upsert a nested EVSE (`NotFound` if the parent Location is unknown).
    pub fn put_evse(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse: Evse2111,
    ) -> Result<(), ServerError> {
        let key = Self::composite_key(country_code, party_id, location_id);
        let mut map = self.locations.write().expect("lock not poisoned");
        let location = map.get_mut(&key).ok_or(ServerError::NotFound)?;
        upsert_by(&mut location.evses, evse, |e| e.uid.as_str().to_owned());
        Ok(())
    }

    /// Merge-patch a nested EVSE (`NotFound` if Location or EVSE is unknown).
    pub fn patch_evse(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError> {
        let key = Self::composite_key(country_code, party_id, location_id);
        let mut map = self.locations.write().expect("lock not poisoned");
        let location = map.get_mut(&key).ok_or(ServerError::NotFound)?;
        let evse = location
            .evses
            .iter_mut()
            .find(|e| e.uid.as_str() == evse_uid)
            .ok_or(ServerError::NotFound)?;
        *evse = apply_merge_patch(evse, partial)?;
        Ok(())
    }

    /// Upsert a nested Connector (`NotFound` if Location or EVSE is unknown).
    pub fn put_connector(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
        connector: Connector2111,
    ) -> Result<(), ServerError> {
        let key = Self::composite_key(country_code, party_id, location_id);
        let mut map = self.locations.write().expect("lock not poisoned");
        let location = map.get_mut(&key).ok_or(ServerError::NotFound)?;
        let evse = location
            .evses
            .iter_mut()
            .find(|e| e.uid.as_str() == evse_uid)
            .ok_or(ServerError::NotFound)?;
        upsert_by(&mut evse.connectors, connector, |c| {
            c.id.as_str().to_owned()
        });
        Ok(())
    }

    /// Merge-patch a nested Connector (`NotFound` if any level is unknown).
    pub fn patch_connector(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
        connector_id: &str,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError> {
        let key = Self::composite_key(country_code, party_id, location_id);
        let mut map = self.locations.write().expect("lock not poisoned");
        let location = map.get_mut(&key).ok_or(ServerError::NotFound)?;
        let evse = location
            .evses
            .iter_mut()
            .find(|e| e.uid.as_str() == evse_uid)
            .ok_or(ServerError::NotFound)?;
        let connector = evse
            .connectors
            .iter_mut()
            .find(|c| c.id.as_str() == connector_id)
            .ok_or(ServerError::NotFound)?;
        *connector = apply_merge_patch(connector, partial)?;
        Ok(())
    }

    /// Return a filtered, paginated slice of the stored 2.1.1 Locations — the
    /// CPO **sender** list (`GET /locations`).
    ///
    /// Filters by `last_updated >= date_from` and (if provided)
    /// `last_updated < date_to`, sorted by `last_updated`. Mirrors
    /// [`LocationsConfig::list`]; the 2.1.1 store keys objects by their owner
    /// segments, but the sender list is flat (a CPO exposing its own
    /// catalogue), so the key is ignored here.
    ///
    /// Returns `(page_items, total_matching_count)`.
    #[must_use]
    pub fn list(
        &self,
        date_from: DateTime<Utc>,
        date_to: Option<DateTime<Utc>>,
        offset: u32,
        limit: u32,
    ) -> (Vec<Location2111>, u32) {
        let map = self.locations.read().expect("lock not poisoned");
        let mut filtered: Vec<&Location2111> = map
            .values()
            .filter(|l| l.last_updated >= date_from && date_to.is_none_or(|dt| l.last_updated < dt))
            .collect();
        filtered.sort_by_key(|l| l.last_updated);
        let total = filtered.len() as u32;
        let page: Vec<Location2111> = filtered
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect();
        (page, total)
    }

    /// Look up a Location by its `id` alone — the 2.1.1 **sender** flat path
    /// (`GET /locations/{location_id}`) carries no owner segments, so a CPO
    /// serving its own catalogue addresses objects by bare id. Returns the
    /// first match.
    #[must_use]
    pub fn get_by_id(&self, location_id: &str) -> Option<Location2111> {
        self.locations
            .read()
            .expect("lock not poisoned")
            .values()
            .find(|l| l.id.as_str() == location_id)
            .cloned()
    }

    /// Flat-path (`GET /locations/{location_id}/{evse_uid}`) EVSE getter.
    #[must_use]
    pub fn get_evse_by_id(&self, location_id: &str, evse_uid: &str) -> Option<Evse2111> {
        self.get_by_id(location_id)?
            .evses
            .into_iter()
            .find(|e| e.uid.as_str() == evse_uid)
    }

    /// Flat-path (`GET /locations/{location_id}/{evse_uid}/{connector_id}`)
    /// Connector getter.
    #[must_use]
    pub fn get_connector_by_id(
        &self,
        location_id: &str,
        evse_uid: &str,
        connector_id: &str,
    ) -> Option<Connector2111> {
        self.get_evse_by_id(location_id, evse_uid)?
            .connectors
            .into_iter()
            .find(|c| c.id.as_str() == connector_id)
    }
}

impl Default for Locations2111Config {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(async_fn_in_trait)]
impl Locations2111Handler for Locations2111Config {
    async fn get_location(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
    ) -> Result<Location2111, ServerError> {
        self.get(country_code, party_id, location_id)
            .ok_or(ServerError::NotFound)
    }

    async fn put_location(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        location: Location2111,
    ) -> Result<(), ServerError> {
        self.put(country_code, party_id, location_id, location);
        Ok(())
    }

    async fn patch_location(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError> {
        self.patch_location(country_code, party_id, location_id, partial)
    }

    async fn get_evse(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
    ) -> Result<Evse2111, ServerError> {
        self.get_evse(country_code, party_id, location_id, evse_uid)
            .ok_or(ServerError::NotFound)
    }

    async fn put_evse(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse: Evse2111,
    ) -> Result<(), ServerError> {
        self.put_evse(country_code, party_id, location_id, evse)
    }

    async fn patch_evse(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError> {
        self.patch_evse(country_code, party_id, location_id, evse_uid, partial)
    }

    async fn get_connector(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
        connector_id: &str,
    ) -> Result<Connector2111, ServerError> {
        self.get_connector(country_code, party_id, location_id, evse_uid, connector_id)
            .ok_or(ServerError::NotFound)
    }

    async fn put_connector(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
        connector: Connector2111,
    ) -> Result<(), ServerError> {
        self.put_connector(country_code, party_id, location_id, evse_uid, connector)
    }

    async fn patch_connector(
        &self,
        country_code: &str,
        party_id: &str,
        location_id: &str,
        evse_uid: &str,
        connector_id: &str,
        partial: ocpi_types::serde_json::Value,
    ) -> Result<(), ServerError> {
        self.patch_connector(
            country_code,
            party_id,
            location_id,
            evse_uid,
            connector_id,
            partial,
        )
    }
}

/// RFC 7396 JSON merge-patch: recursively apply `patch` onto `base`.
fn json_merge(base: &mut ocpi_types::serde_json::Value, patch: ocpi_types::serde_json::Value) {
    match patch {
        ocpi_types::serde_json::Value::Object(patch_map) => {
            if let ocpi_types::serde_json::Value::Object(base_map) = base {
                for (key, val) in patch_map {
                    if val.is_null() {
                        base_map.remove(&key);
                    } else {
                        json_merge(
                            base_map
                                .entry(key)
                                .or_insert(ocpi_types::serde_json::Value::Null),
                            val,
                        );
                    }
                }
            }
        }
        _ => *base = patch,
    }
}

// ── axum integration ──────────────────────────────────────────────────────────

#[cfg(feature = "axum")]
pub mod http {
    //! axum integration: ready-made routers for OCPI receiver endpoints.

    use std::sync::Arc;

    use axum::{
        extract::{Path, Query, State},
        http::{HeaderMap, StatusCode},
        response::{IntoResponse, Response},
        routing::{get, post, put},
        Json, Router,
    };
    use ocpi_types::{
        envelope::{OcpiPaged, OcpiResponse},
        transport::{CredentialToken, PaginatedParams},
        v2_2_1::{
            ActiveChargingProfile, ActiveChargingProfileResult, AuthorizationInfo,
            CancelReservation, Cdr, ChargingPreferences, ChargingPreferencesResponse,
            ChargingProfileResponse, ChargingProfileResult, ClearProfileResult, ClientInfo,
            CommandResponse, CommandResult, CommandType, Connector, Credentials, Evse, Location,
            LocationReferences, ReserveNow, Session, SetChargingProfile, StartSession, StopSession,
            Tariff, Token, TokenType, UnlockConnector,
        },
        version::{VersionDetails, VersionNumber},
        OcpiStatusCode,
    };

    use crate::{
        token_type_2_1_1_str, token_type_str, Cdrs2111Config, Cdrs22Config, CdrsConfig,
        ChargingProfilesConfig, ChargingProfilesHandler, Commands2111Config, Commands2111Handler,
        Commands22Config, Commands22Handler, CommandsConfig, CommandsHandler,
        Credentials2111Config, CredentialsConfig, HubClientInfoConfig, Locations2111Config,
        LocationsConfig, ServerError, Sessions2111Config, SessionsConfig, Tariffs2111Config,
        TariffsConfig, Tokens2111Config, TokensConfig, VersionsConfig,
    };
    // The flat OCPI 2.1.1 credentials object served by `credentials_2_1_1_router`.
    use ocpi_types::v2_1_1::Credentials as Credentials2111;
    // The OCPI 2.1.1 `Tariff` served by `tariffs_2_1_1_router`.
    use ocpi_types::v2_1_1::Tariff as Tariff2111;
    // The OCPI 2.1.1 Session object served by `sessions_2_1_1_router`.
    use ocpi_types::v2_1_1::Session as Session2111;
    // The OCPI 2.1.1 CDR object served by `cdrs_2_1_1_router`.
    use ocpi_types::v2_1_1::Cdr as Cdr2111;
    // The OCPI 2.2 CDR object served by `cdrs_2_2_router`.
    use ocpi_types::v2_2::Cdr as Cdr22;
    // The OCPI 2.1.1 Tokens surface served by `tokens_2_1_1_router`.
    use ocpi_types::v2_1_1::{
        AuthorizationInfo as AuthorizationInfo2111, LocationReferences as LocationReferences2111,
        Token as Token2111, TokenType as TokenType2111,
    };
    // The OCPI 2.1.1 Locations objects served by `locations_2_1_1_router`.
    use ocpi_types::v2_1_1::{
        Connector as Connector2111, Evse as Evse2111, Location as Location2111,
    };
    // The OCPI 2.1.1 Commands surface served by `commands_2_1_1_router`.
    use ocpi_types::v2_1_1::{
        CommandResponse as CommandResponse2111, CommandType as CommandType2111,
        ReserveNow as ReserveNow2111, StartSession as StartSession2111,
        StopSession as StopSession2111, UnlockConnector as UnlockConnector2111,
    };
    // The OCPI 2.2 `StartSession` served by `commands_2_2_router` — no
    // `connector_id`. Every other 2.2 command body is the wire-identical 2.2.1
    // type already imported above.
    use ocpi_types::v2_2::StartSession as StartSession22;

    // ── Versions ──────────────────────────────────────────────────────────────

    /// Build an axum router exposing `GET /versions` and `GET /versions/{version}`.
    ///
    /// Pass a [`VersionsConfig`] populated with the versions and endpoint URLs
    /// your OCPI node supports.
    pub fn versions_router(config: VersionsConfig) -> Router {
        Router::new()
            .route("/versions", get(list_versions))
            .route("/versions/{version}", get(version_details))
            .with_state(Arc::new(config))
    }

    async fn list_versions(State(cfg): State<Arc<VersionsConfig>>) -> Response {
        Json(OcpiResponse::success(cfg.versions.clone())).into_response()
    }

    async fn version_details(
        State(cfg): State<Arc<VersionsConfig>>,
        Path(version_str): Path<String>,
    ) -> Response {
        let version = match version_str.parse::<VersionNumber>() {
            Ok(v) => v,
            Err(_) => {
                return Json(OcpiResponse::<VersionDetails>::error(
                    OcpiStatusCode::InvalidParameters,
                    format!("unknown version: {version_str}"),
                ))
                .into_response();
            }
        };
        // Role-less (OCPI ≤2.1.1) catalogues are served verbatim — their
        // endpoints must omit `role` on the wire. The two maps are keyed by
        // disjoint version numbers, so a legacy hit and a role-bearing hit
        // never collide for the same version.
        if let Some(details) = cfg.legacy_details.get(&version).cloned() {
            return Json(OcpiResponse::success(details)).into_response();
        }
        match cfg.details.get(&version).cloned() {
            Some(details) => Json(OcpiResponse::success(details)).into_response(),
            None => Json(OcpiResponse::<VersionDetails>::error(
                OcpiStatusCode::UnsupportedVersion,
                format!("version {version_str} not supported"),
            ))
            .into_response(),
        }
    }

    // ── Credentials ───────────────────────────────────────────────────────────

    /// Build an axum router for the OCPI credentials endpoints.
    ///
    /// Exposes:
    /// - `GET    /credentials` — return this server's own credentials
    /// - `POST   /credentials` — register a new party; HTTP 405 if already registered
    /// - `PUT    /credentials` — update an existing registration; HTTP 405 if not registered
    /// - `DELETE /credentials` — revoke a registration; HTTP 405 if not registered
    ///
    /// All routes validate the `Authorization: Token <base64>` header.
    /// Pass an `Arc<`[`CredentialsConfig`]`>` so the same store can be shared
    /// with other handlers or inspected by the host application.
    pub fn credentials_router(config: Arc<CredentialsConfig>) -> Router {
        Router::new()
            .route(
                "/credentials",
                get(credentials_get)
                    .post(credentials_post)
                    .put(credentials_put)
                    .delete(credentials_delete),
            )
            .with_state(config)
    }

    /// Extract and decode the Bearer token from `Authorization: Token <base64>`.
    fn extract_token(headers: &HeaderMap) -> Option<String> {
        let value = headers.get("Authorization")?.to_str().ok()?;
        CredentialToken::from_header_value(value).map(|t| t.as_str().to_owned())
    }

    fn credentials_unauthorized() -> Response {
        (
            StatusCode::UNAUTHORIZED,
            Json(OcpiResponse::<Credentials>::error(
                OcpiStatusCode::ClientError,
                "unauthorized",
            )),
        )
            .into_response()
    }

    fn credentials_method_not_allowed(msg: &'static str) -> Response {
        (
            StatusCode::METHOD_NOT_ALLOWED,
            Json(OcpiResponse::<Credentials>::error(
                OcpiStatusCode::ClientError,
                msg,
            )),
        )
            .into_response()
    }

    fn credentials_server_error() -> Response {
        Json(OcpiResponse::<Credentials>::error(
            OcpiStatusCode::ServerError,
            "internal server error",
        ))
        .into_response()
    }

    /// `3001` — the server could not use the registering party's API during the
    /// fetch-back (could not retrieve its `/versions` or version details).
    fn credentials_unable_to_use_client() -> Response {
        Json(OcpiResponse::<Credentials>::error(
            OcpiStatusCode::UnableToUseClientApi,
            "unable to use the client's API",
        ))
        .into_response()
    }

    async fn credentials_get(
        State(cfg): State<Arc<CredentialsConfig>>,
        headers: HeaderMap,
    ) -> Response {
        let token = match extract_token(&headers) {
            Some(t) => t,
            None => return credentials_unauthorized(),
        };
        if !cfg.is_registered(token.as_str()) {
            return credentials_unauthorized();
        }
        Json(OcpiResponse::success(cfg.own_credentials.clone())).into_response()
    }

    async fn credentials_post(
        State(cfg): State<Arc<CredentialsConfig>>,
        headers: HeaderMap,
        Json(body): Json<Credentials>,
    ) -> Response {
        let token = match extract_token(&headers) {
            Some(t) => t,
            None => return credentials_unauthorized(),
        };
        // The registry is keyed by the issued Token C (`own_credentials.token`):
        // per spec §Registration the Sender switches to Token C for every
        // subsequent request, so that — not the bootstrap Token A bearer — is
        // what must authenticate afterwards. Reject re-registration (the Sender
        // should PUT to rotate) before running the potentially expensive
        // fetch-back.
        if cfg.is_registered(cfg.own_credentials.token.as_str()) {
            return credentials_method_not_allowed("already registered");
        }
        // Spec §POST: the receiver fetches the sender's endpoints for the
        // registered version, authenticating with the sender's Token B
        // (`body.token`). Any failure → status code 3001.
        let endpoints = match cfg.fetch_back(&body).await {
            Ok(endpoints) => endpoints,
            Err(_) => return credentials_unable_to_use_client(),
        };
        // Register under the issued Token C, not the bootstrap Token A bearer.
        match cfg.register_with_endpoints(cfg.own_credentials.token.as_str(), body, endpoints) {
            Ok(()) => {
                // Burn the single-use bootstrap Token A: per spec it "MAY no
                // longer be used" once the Sender holds Token C. Guard against a
                // misconfiguration where Token C equals the bearer, which would
                // otherwise undo the registration just made.
                if token.as_str() != cfg.own_credentials.token.as_str() {
                    cfg.invalidate(token.as_str());
                }
                Json(OcpiResponse::success(cfg.own_credentials.clone())).into_response()
            }
            Err(ServerError::AlreadyRegistered) => {
                credentials_method_not_allowed("already registered")
            }
            Err(_) => credentials_server_error(),
        }
    }

    async fn credentials_put(
        State(cfg): State<Arc<CredentialsConfig>>,
        headers: HeaderMap,
        Json(body): Json<Credentials>,
    ) -> Response {
        let token = match extract_token(&headers) {
            Some(t) => t,
            None => return credentials_unauthorized(),
        };
        // The caller authenticates with the registered Token C; reject unknown
        // parties before the fetch-back.
        if !cfg.is_registered(token.as_str()) {
            return credentials_method_not_allowed("not registered");
        }
        // Spec §PUT: re-fetch the sender's endpoints on credential update.
        let endpoints = match cfg.fetch_back(&body).await {
            Ok(endpoints) => endpoints,
            Err(_) => return credentials_unable_to_use_client(),
        };
        // The registration is keyed by Token C (`own_credentials.token`); update
        // under that same key rather than the bearer.
        match cfg.update_with_endpoints(cfg.own_credentials.token.as_str(), body, endpoints) {
            Ok(()) => Json(OcpiResponse::success(cfg.own_credentials.clone())).into_response(),
            Err(ServerError::NotRegistered) => credentials_method_not_allowed("not registered"),
            Err(_) => credentials_server_error(),
        }
    }

    async fn credentials_delete(
        State(cfg): State<Arc<CredentialsConfig>>,
        headers: HeaderMap,
    ) -> Response {
        let token = match extract_token(&headers) {
            Some(t) => t,
            None => return credentials_unauthorized(),
        };
        match cfg.delete(token.as_str()) {
            Ok(()) => Json(OcpiResponse::<Credentials>::success_empty()).into_response(),
            Err(ServerError::NotRegistered) => credentials_method_not_allowed("not registered"),
            Err(_) => credentials_server_error(),
        }
    }

    // ── Credentials (OCPI 2.1.1, flat object) ──────────────────────────────────

    /// Build an axum router for the **OCPI 2.1.1** Credentials module.
    ///
    /// Exposes `GET/POST/PUT/DELETE /credentials` over the flat 2.1.1
    /// [`Credentials2111`] object, running the same Token A→B→C registration
    /// semantics as [`credentials_router`]: the registry is keyed by the issued
    /// *Token C* (`own_credentials.token`) and the bootstrap *Token A* is burned
    /// on a successful `POST`. The 2.1.1 fetch-back is not performed (see
    /// [`Credentials2111Config`]).
    pub fn credentials_2_1_1_router(config: Arc<Credentials2111Config>) -> Router {
        Router::new()
            .route(
                "/credentials",
                get(credentials_2_1_1_get)
                    .post(credentials_2_1_1_post)
                    .put(credentials_2_1_1_put)
                    .delete(credentials_2_1_1_delete),
            )
            .with_state(config)
    }

    fn credentials_2_1_1_unauthorized() -> Response {
        (
            StatusCode::UNAUTHORIZED,
            Json(OcpiResponse::<Credentials2111>::error(
                OcpiStatusCode::ClientError,
                "unauthorized",
            )),
        )
            .into_response()
    }

    fn credentials_2_1_1_method_not_allowed(msg: &'static str) -> Response {
        (
            StatusCode::METHOD_NOT_ALLOWED,
            Json(OcpiResponse::<Credentials2111>::error(
                OcpiStatusCode::ClientError,
                msg,
            )),
        )
            .into_response()
    }

    fn credentials_2_1_1_server_error() -> Response {
        Json(OcpiResponse::<Credentials2111>::error(
            OcpiStatusCode::ServerError,
            "internal server error",
        ))
        .into_response()
    }

    /// `3001` — the server could not use the registering party's API during the
    /// 2.1.1 fetch-back (could not retrieve its `/versions` or version details).
    fn credentials_2_1_1_unable_to_use_client() -> Response {
        Json(OcpiResponse::<Credentials2111>::error(
            OcpiStatusCode::UnableToUseClientApi,
            "unable to use the client's API",
        ))
        .into_response()
    }

    async fn credentials_2_1_1_get(
        State(cfg): State<Arc<Credentials2111Config>>,
        headers: HeaderMap,
    ) -> Response {
        let token = match extract_token(&headers) {
            Some(t) => t,
            None => return credentials_2_1_1_unauthorized(),
        };
        if !cfg.is_registered(token.as_str()) {
            return credentials_2_1_1_unauthorized();
        }
        Json(OcpiResponse::success(cfg.own_credentials.clone())).into_response()
    }

    async fn credentials_2_1_1_post(
        State(cfg): State<Arc<Credentials2111Config>>,
        headers: HeaderMap,
        Json(body): Json<Credentials2111>,
    ) -> Response {
        let token = match extract_token(&headers) {
            Some(t) => t,
            None => return credentials_2_1_1_unauthorized(),
        };
        // The registry is keyed by the issued Token C (`own_credentials.token`):
        // the Sender switches to Token C for every subsequent request, so that —
        // not the bootstrap Token A bearer — is what must authenticate
        // afterwards. Reject re-registration (the Sender should PUT to rotate).
        if cfg.is_registered(cfg.own_credentials.token.as_str()) {
            return credentials_2_1_1_method_not_allowed("already registered");
        }
        // 2.1.1 §Registration: the receiver fetches the sender's role-less
        // endpoints, authenticating with the sender's Token B (`body.token`).
        // Any failure → status code 3001. A no-op (`Ok(None)`) when no fetcher.
        let endpoints = match cfg.fetch_back(&body).await {
            Ok(endpoints) => endpoints,
            Err(_) => return credentials_2_1_1_unable_to_use_client(),
        };
        match cfg.register_with_endpoints(cfg.own_credentials.token.as_str(), body, endpoints) {
            Ok(()) => {
                // Burn the single-use bootstrap Token A once the Sender holds
                // Token C. Guard against a misconfiguration where Token C equals
                // the bearer, which would otherwise undo the registration.
                if token.as_str() != cfg.own_credentials.token.as_str() {
                    cfg.invalidate(token.as_str());
                }
                Json(OcpiResponse::success(cfg.own_credentials.clone())).into_response()
            }
            Err(ServerError::AlreadyRegistered) => {
                credentials_2_1_1_method_not_allowed("already registered")
            }
            Err(_) => credentials_2_1_1_server_error(),
        }
    }

    async fn credentials_2_1_1_put(
        State(cfg): State<Arc<Credentials2111Config>>,
        headers: HeaderMap,
        Json(body): Json<Credentials2111>,
    ) -> Response {
        let token = match extract_token(&headers) {
            Some(t) => t,
            None => return credentials_2_1_1_unauthorized(),
        };
        // The caller authenticates with the registered Token C.
        if !cfg.is_registered(token.as_str()) {
            return credentials_2_1_1_method_not_allowed("not registered");
        }
        // 2.1.1 §PUT: re-fetch the sender's role-less endpoints on update.
        let endpoints = match cfg.fetch_back(&body).await {
            Ok(endpoints) => endpoints,
            Err(_) => return credentials_2_1_1_unable_to_use_client(),
        };
        // The registration is keyed by Token C (`own_credentials.token`); update
        // under that same key rather than the bearer.
        match cfg.update_with_endpoints(cfg.own_credentials.token.as_str(), body, endpoints) {
            Ok(()) => Json(OcpiResponse::success(cfg.own_credentials.clone())).into_response(),
            Err(ServerError::NotRegistered) => {
                credentials_2_1_1_method_not_allowed("not registered")
            }
            Err(_) => credentials_2_1_1_server_error(),
        }
    }

    async fn credentials_2_1_1_delete(
        State(cfg): State<Arc<Credentials2111Config>>,
        headers: HeaderMap,
    ) -> Response {
        let token = match extract_token(&headers) {
            Some(t) => t,
            None => return credentials_2_1_1_unauthorized(),
        };
        match cfg.delete(token.as_str()) {
            Ok(()) => Json(OcpiResponse::<Credentials2111>::success_empty()).into_response(),
            Err(ServerError::NotRegistered) => {
                credentials_2_1_1_method_not_allowed("not registered")
            }
            Err(_) => credentials_2_1_1_server_error(),
        }
    }

    // ── Sessions ──────────────────────────────────────────────────────────────

    const DEFAULT_LIMIT: u32 = 50;

    /// Build an axum router for the OCPI Sessions module.
    ///
    /// Exposes:
    /// - `GET  /sessions` — paginated list (sender interface, CPO)
    /// - `GET  /sessions/{country_code}/{party_id}/{session_id}` — single
    /// - `PUT  /sessions/{country_code}/{party_id}/{session_id}` — upsert
    /// - `PATCH /sessions/{country_code}/{party_id}/{session_id}` — merge-patch
    ///
    /// OCPI routing headers (`OCPI-from/to-party-id/country-code`) are accepted
    /// on all routes; they are not enforced at this layer and can be validated
    /// by middleware in production deployments.
    pub fn sessions_router(config: Arc<SessionsConfig>) -> Router {
        Router::new()
            .route("/sessions", get(sessions_list))
            .route(
                "/sessions/{session_id}/charging_preferences",
                put(sessions_charging_preferences),
            )
            .route(
                "/sessions/{country_code}/{party_id}/{session_id}",
                get(sessions_get).put(sessions_put).patch(sessions_patch),
            )
            .with_state(config)
    }

    async fn sessions_list(
        State(cfg): State<Arc<SessionsConfig>>,
        Query(params): Query<PaginatedParams>,
    ) -> Response {
        use ocpi_types::chrono::TimeZone as _;
        let date_from = params.date_from.unwrap_or_else(|| {
            ocpi_types::Utc
                .with_ymd_and_hms(1970, 1, 1, 0, 0, 0)
                .single()
                .expect("epoch is valid")
        });
        let offset = params.offset.unwrap_or(0);
        let limit = params.limit.unwrap_or(DEFAULT_LIMIT);

        let (items, total) = cfg.list(date_from, params.date_to, offset, limit);
        let page = OcpiPaged::new(items, offset, limit, total);
        let next_offset = page.next_offset();
        let body = page.into_response();

        let mut response = Json(body).into_response();
        let hdrs = response.headers_mut();
        if let Ok(v) = total.to_string().parse() {
            hdrs.insert("x-total-count", v);
        }
        if let Ok(v) = limit.to_string().parse() {
            hdrs.insert("x-limit", v);
        }
        if let Some(next_off) = next_offset {
            let link = format!("</sessions?offset={next_off}&limit={limit}>; rel=\"next\"");
            if let Ok(v) = link.parse() {
                hdrs.insert("link", v);
            }
        }

        response
    }

    async fn sessions_get(
        State(cfg): State<Arc<SessionsConfig>>,
        Path((country_code, party_id, session_id)): Path<(String, String, String)>,
    ) -> Response {
        match cfg.get(&country_code, &party_id, &session_id) {
            Some(session) => Json(OcpiResponse::success(session)).into_response(),
            None => (
                StatusCode::NOT_FOUND,
                Json(OcpiResponse::<Session>::error(
                    OcpiStatusCode::UnknownLocation,
                    format!("session {session_id} not found"),
                )),
            )
                .into_response(),
        }
    }

    async fn sessions_put(
        State(cfg): State<Arc<SessionsConfig>>,
        Path((country_code, party_id, session_id)): Path<(String, String, String)>,
        Json(session): Json<Session>,
    ) -> Response {
        cfg.put(&country_code, &party_id, &session_id, session);
        Json(OcpiResponse::<Session>::success_empty()).into_response()
    }

    async fn sessions_patch(
        State(cfg): State<Arc<SessionsConfig>>,
        Path((country_code, party_id, session_id)): Path<(String, String, String)>,
        Json(partial): Json<ocpi_types::serde_json::Value>,
    ) -> Response {
        match cfg.patch_json(&country_code, &party_id, &session_id, partial) {
            Ok(()) => Json(OcpiResponse::<Session>::success_empty()).into_response(),
            Err(ServerError::NotFound) => (
                StatusCode::NOT_FOUND,
                Json(OcpiResponse::<Session>::error(
                    OcpiStatusCode::UnknownLocation,
                    format!("session {session_id} not found"),
                )),
            )
                .into_response(),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<Session>::error(
                    OcpiStatusCode::ServerError,
                    "internal error",
                )),
            )
                .into_response(),
        }
    }

    /// `PUT /sessions/{session_id}/charging_preferences` — Sender interface.
    ///
    /// The eMSP submits the driver's [`ChargingPreferences`]; the CPO replies
    /// with a [`ChargingPreferencesResponse`] inside the OCPI envelope. An
    /// unknown `session_id` yields OCPI `2003` / HTTP 404.
    ///
    /// Spec: `specs/ocpi/2.2.1/mod_sessions.asciidoc` — §Set: Charging Preferences.
    async fn sessions_charging_preferences(
        State(cfg): State<Arc<SessionsConfig>>,
        Path(session_id): Path<String>,
        Json(preferences): Json<ChargingPreferences>,
    ) -> Response {
        match cfg.set_charging_preferences(&session_id, &preferences) {
            Ok(response) => Json(OcpiResponse::success(response)).into_response(),
            Err(ServerError::NotFound) => (
                StatusCode::NOT_FOUND,
                Json(OcpiResponse::<ChargingPreferencesResponse>::error(
                    OcpiStatusCode::UnknownLocation,
                    format!("session {session_id} not found"),
                )),
            )
                .into_response(),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<ChargingPreferencesResponse>::error(
                    OcpiStatusCode::ServerError,
                    "internal error",
                )),
            )
                .into_response(),
        }
    }

    // ── Sessions (2.1.1) ───────────────────────────────────────────────────────

    /// Build an axum router for the **OCPI 2.1.1** Sessions module.
    ///
    /// Exposes:
    /// - `GET   /sessions` — paginated list (sender interface, CPO)
    /// - `GET   /sessions/{country_code}/{party_id}/{session_id}` — single
    /// - `PUT   /sessions/{country_code}/{party_id}/{session_id}` — upsert
    /// - `PATCH /sessions/{country_code}/{party_id}/{session_id}` — merge-patch
    ///
    /// The path layout is identical to the 2.2.1 [`sessions_router`] — per OCPI
    /// 2.1.1 §9.2.2 Sessions is a client-owned object whose receiver endpoints
    /// carry the `{country_code}/{party_id}/{session_id}` segments. There is
    /// **no** `charging_preferences` route (a 2.2 addition). Only the payload
    /// is the 2.1.1 [`Session2111`] shape.
    pub fn sessions_2_1_1_router(config: Arc<Sessions2111Config>) -> Router {
        Router::new()
            .route("/sessions", get(sessions_2_1_1_list))
            .route(
                "/sessions/{country_code}/{party_id}/{session_id}",
                get(sessions_2_1_1_get)
                    .put(sessions_2_1_1_put)
                    .patch(sessions_2_1_1_patch),
            )
            .with_state(config)
    }

    async fn sessions_2_1_1_list(
        State(cfg): State<Arc<Sessions2111Config>>,
        Query(params): Query<PaginatedParams>,
    ) -> Response {
        use ocpi_types::chrono::TimeZone as _;
        let date_from = params.date_from.unwrap_or_else(|| {
            ocpi_types::Utc
                .with_ymd_and_hms(1970, 1, 1, 0, 0, 0)
                .single()
                .expect("epoch is valid")
        });
        let offset = params.offset.unwrap_or(0);
        let limit = params.limit.unwrap_or(DEFAULT_LIMIT);

        let (items, total) = cfg.list(date_from, params.date_to, offset, limit);
        let page = OcpiPaged::new(items, offset, limit, total);
        let next_offset = page.next_offset();
        let body = page.into_response();

        let mut response = Json(body).into_response();
        let hdrs = response.headers_mut();
        if let Ok(v) = total.to_string().parse() {
            hdrs.insert("x-total-count", v);
        }
        if let Ok(v) = limit.to_string().parse() {
            hdrs.insert("x-limit", v);
        }
        if let Some(next_off) = next_offset {
            let link = format!("</sessions?offset={next_off}&limit={limit}>; rel=\"next\"");
            if let Ok(v) = link.parse() {
                hdrs.insert("link", v);
            }
        }

        response
    }

    async fn sessions_2_1_1_get(
        State(cfg): State<Arc<Sessions2111Config>>,
        Path((country_code, party_id, session_id)): Path<(String, String, String)>,
    ) -> Response {
        match cfg.get(&country_code, &party_id, &session_id) {
            Some(session) => Json(OcpiResponse::success(session)).into_response(),
            None => (
                StatusCode::NOT_FOUND,
                Json(OcpiResponse::<Session2111>::error(
                    OcpiStatusCode::UnknownLocation,
                    format!("session {session_id} not found"),
                )),
            )
                .into_response(),
        }
    }

    async fn sessions_2_1_1_put(
        State(cfg): State<Arc<Sessions2111Config>>,
        Path((country_code, party_id, session_id)): Path<(String, String, String)>,
        Json(session): Json<Session2111>,
    ) -> Response {
        cfg.put(&country_code, &party_id, &session_id, session);
        Json(OcpiResponse::<Session2111>::success_empty()).into_response()
    }

    async fn sessions_2_1_1_patch(
        State(cfg): State<Arc<Sessions2111Config>>,
        Path((country_code, party_id, session_id)): Path<(String, String, String)>,
        Json(partial): Json<ocpi_types::serde_json::Value>,
    ) -> Response {
        match cfg.patch_json(&country_code, &party_id, &session_id, partial) {
            Ok(()) => Json(OcpiResponse::<Session2111>::success_empty()).into_response(),
            Err(ServerError::NotFound) => (
                StatusCode::NOT_FOUND,
                Json(OcpiResponse::<Session2111>::error(
                    OcpiStatusCode::UnknownLocation,
                    format!("session {session_id} not found"),
                )),
            )
                .into_response(),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<Session2111>::error(
                    OcpiStatusCode::ServerError,
                    "internal error",
                )),
            )
                .into_response(),
        }
    }

    // ── CDRs ──────────────────────────────────────────────────────────────────

    /// Build an axum router for the OCPI CDRs module.
    ///
    /// Exposes:
    /// - `GET  /cdrs` — paginated list (sender interface, CPO)
    /// - `GET  /cdrs/{cdr_id}` — single CDR (sender interface, CPO)
    /// - `POST /cdrs` — store a new CDR (receiver interface, eMSP); responds
    ///   `201 Created` with a `Location` header pointing to the stored CDR.
    ///
    /// OCPI routing headers (`OCPI-from/to-party-id/country-code`) are accepted
    /// on all routes; they are not enforced at this layer.
    pub fn cdrs_router(config: Arc<CdrsConfig>) -> Router {
        Router::new()
            .route("/cdrs", get(cdrs_list).post(cdrs_post))
            .route("/cdrs/{cdr_id}", get(cdrs_get))
            .with_state(config)
    }

    async fn cdrs_list(
        State(cfg): State<Arc<CdrsConfig>>,
        Query(params): Query<PaginatedParams>,
    ) -> Response {
        use ocpi_types::chrono::TimeZone as _;
        let date_from = params.date_from.unwrap_or_else(|| {
            ocpi_types::Utc
                .with_ymd_and_hms(1970, 1, 1, 0, 0, 0)
                .single()
                .expect("epoch is valid")
        });
        let offset = params.offset.unwrap_or(0);
        let limit = params.limit.unwrap_or(DEFAULT_LIMIT);

        let (items, total) = cfg.list(date_from, params.date_to, offset, limit);
        let page = OcpiPaged::new(items, offset, limit, total);
        let next_offset = page.next_offset();
        let body = page.into_response();

        let mut response = Json(body).into_response();
        let hdrs = response.headers_mut();
        if let Ok(v) = total.to_string().parse() {
            hdrs.insert("x-total-count", v);
        }
        if let Ok(v) = limit.to_string().parse() {
            hdrs.insert("x-limit", v);
        }
        if let Some(next_off) = next_offset {
            let link = format!("</cdrs?offset={next_off}&limit={limit}>; rel=\"next\"");
            if let Ok(v) = link.parse() {
                hdrs.insert("link", v);
            }
        }

        response
    }

    async fn cdrs_get(State(cfg): State<Arc<CdrsConfig>>, Path(cdr_id): Path<String>) -> Response {
        match cfg.get(&cdr_id) {
            Some(cdr) => Json(OcpiResponse::success(cdr)).into_response(),
            None => (
                StatusCode::NOT_FOUND,
                Json(OcpiResponse::<Cdr>::error(
                    OcpiStatusCode::UnknownLocation,
                    format!("CDR {cdr_id} not found"),
                )),
            )
                .into_response(),
        }
    }

    async fn cdrs_post(State(cfg): State<Arc<CdrsConfig>>, Json(cdr): Json<Cdr>) -> Response {
        let location_url = cfg.store(cdr);
        let mut response = (
            StatusCode::CREATED,
            Json(OcpiResponse::<Cdr>::success_empty()),
        )
            .into_response();
        if let Ok(v) = location_url.parse() {
            response.headers_mut().insert("location", v);
        }
        response
    }

    // ── CDRs (OCPI 2.1.1) ───────────────────────────────────────────────────────

    /// Build an axum router for the **OCPI 2.1.1** CDRs module.
    ///
    /// Exposes the same flat routes as the 2.2.1 [`cdrs_router`] — a CDR is a
    /// server-owned object named via the `Location` header (§10.2.2), so there
    /// are no `{country_code}/{party_id}` segments:
    /// - `GET  /cdrs` — paginated list (sender interface, CPO)
    /// - `GET  /cdrs/{cdr_id}` — single CDR (receiver interface, eMSP)
    /// - `POST /cdrs` — store a new CDR (receiver interface, eMSP); responds
    ///   `201 Created` with a `Location` header pointing to the stored CDR.
    ///
    /// Only the payload is the 2.1.1 [`Cdr2111`] shape.
    pub fn cdrs_2_1_1_router(config: Arc<Cdrs2111Config>) -> Router {
        Router::new()
            .route("/cdrs", get(cdrs_2_1_1_list).post(cdrs_2_1_1_post))
            .route("/cdrs/{cdr_id}", get(cdrs_2_1_1_get))
            .with_state(config)
    }

    async fn cdrs_2_1_1_list(
        State(cfg): State<Arc<Cdrs2111Config>>,
        Query(params): Query<PaginatedParams>,
    ) -> Response {
        use ocpi_types::chrono::TimeZone as _;
        let date_from = params.date_from.unwrap_or_else(|| {
            ocpi_types::Utc
                .with_ymd_and_hms(1970, 1, 1, 0, 0, 0)
                .single()
                .expect("epoch is valid")
        });
        let offset = params.offset.unwrap_or(0);
        let limit = params.limit.unwrap_or(DEFAULT_LIMIT);

        let (items, total) = cfg.list(date_from, params.date_to, offset, limit);
        let page = OcpiPaged::new(items, offset, limit, total);
        let next_offset = page.next_offset();
        let body = page.into_response();

        let mut response = Json(body).into_response();
        let hdrs = response.headers_mut();
        if let Ok(v) = total.to_string().parse() {
            hdrs.insert("x-total-count", v);
        }
        if let Ok(v) = limit.to_string().parse() {
            hdrs.insert("x-limit", v);
        }
        if let Some(next_off) = next_offset {
            let link = format!("</cdrs?offset={next_off}&limit={limit}>; rel=\"next\"");
            if let Ok(v) = link.parse() {
                hdrs.insert("link", v);
            }
        }

        response
    }

    async fn cdrs_2_1_1_get(
        State(cfg): State<Arc<Cdrs2111Config>>,
        Path(cdr_id): Path<String>,
    ) -> Response {
        match cfg.get(&cdr_id) {
            Some(cdr) => Json(OcpiResponse::success(cdr)).into_response(),
            None => (
                StatusCode::NOT_FOUND,
                Json(OcpiResponse::<Cdr2111>::error(
                    OcpiStatusCode::UnknownLocation,
                    format!("CDR {cdr_id} not found"),
                )),
            )
                .into_response(),
        }
    }

    async fn cdrs_2_1_1_post(
        State(cfg): State<Arc<Cdrs2111Config>>,
        Json(cdr): Json<Cdr2111>,
    ) -> Response {
        let location_url = cfg.store(cdr);
        let mut response = (
            StatusCode::CREATED,
            Json(OcpiResponse::<Cdr2111>::success_empty()),
        )
            .into_response();
        if let Ok(v) = location_url.parse() {
            response.headers_mut().insert("location", v);
        }
        response
    }

    // ── CDRs (OCPI 2.2) ─────────────────────────────────────────────────────────

    /// Build an axum router for the **OCPI 2.2** CDRs module.
    ///
    /// Exposes the same flat routes as the 2.2.1 [`cdrs_router`] — a CDR is a
    /// server-owned object named via the `Location` header (§8.2.2), so there
    /// are no `{country_code}/{party_id}` segments:
    /// - `GET  /cdrs` — paginated list (sender interface, CPO)
    /// - `GET  /cdrs/{cdr_id}` — single CDR (receiver interface, eMSP)
    /// - `POST /cdrs` — store a new CDR (receiver interface, eMSP); responds
    ///   `201 Created` with a `Location` header pointing to the stored CDR.
    ///
    /// Only the payload is the 2.2 [`Cdr22`] shape (a `CdrToken` with no
    /// `country_code`/`party_id`, a `Cdr` with no `home_charging_compensation`,
    /// a `CdrLocation` with a required `postal_code` and no `state`).
    pub fn cdrs_2_2_router(config: Arc<Cdrs22Config>) -> Router {
        Router::new()
            .route("/cdrs", get(cdrs_2_2_list).post(cdrs_2_2_post))
            .route("/cdrs/{cdr_id}", get(cdrs_2_2_get))
            .with_state(config)
    }

    async fn cdrs_2_2_list(
        State(cfg): State<Arc<Cdrs22Config>>,
        Query(params): Query<PaginatedParams>,
    ) -> Response {
        use ocpi_types::chrono::TimeZone as _;
        let date_from = params.date_from.unwrap_or_else(|| {
            ocpi_types::Utc
                .with_ymd_and_hms(1970, 1, 1, 0, 0, 0)
                .single()
                .expect("epoch is valid")
        });
        let offset = params.offset.unwrap_or(0);
        let limit = params.limit.unwrap_or(DEFAULT_LIMIT);

        let (items, total) = cfg.list(date_from, params.date_to, offset, limit);
        let page = OcpiPaged::new(items, offset, limit, total);
        let next_offset = page.next_offset();
        let body = page.into_response();

        let mut response = Json(body).into_response();
        let hdrs = response.headers_mut();
        if let Ok(v) = total.to_string().parse() {
            hdrs.insert("x-total-count", v);
        }
        if let Ok(v) = limit.to_string().parse() {
            hdrs.insert("x-limit", v);
        }
        if let Some(next_off) = next_offset {
            let link = format!("</cdrs?offset={next_off}&limit={limit}>; rel=\"next\"");
            if let Ok(v) = link.parse() {
                hdrs.insert("link", v);
            }
        }

        response
    }

    async fn cdrs_2_2_get(
        State(cfg): State<Arc<Cdrs22Config>>,
        Path(cdr_id): Path<String>,
    ) -> Response {
        match cfg.get(&cdr_id) {
            Some(cdr) => Json(OcpiResponse::success(cdr)).into_response(),
            None => (
                StatusCode::NOT_FOUND,
                Json(OcpiResponse::<Cdr22>::error(
                    OcpiStatusCode::UnknownLocation,
                    format!("CDR {cdr_id} not found"),
                )),
            )
                .into_response(),
        }
    }

    async fn cdrs_2_2_post(
        State(cfg): State<Arc<Cdrs22Config>>,
        Json(cdr): Json<Cdr22>,
    ) -> Response {
        let location_url = cfg.store(cdr);
        let mut response = (
            StatusCode::CREATED,
            Json(OcpiResponse::<Cdr22>::success_empty()),
        )
            .into_response();
        if let Ok(v) = location_url.parse() {
            response.headers_mut().insert("location", v);
        }
        response
    }

    // ── Tariffs ───────────────────────────────────────────────────────────────

    /// Build an axum router for the OCPI Tariffs module.
    ///
    /// Exposes:
    /// - `GET  /tariffs` — paginated list (sender interface, CPO)
    /// - `GET  /tariffs/{country_code}/{party_id}/{tariff_id}` — single tariff
    /// - `PUT  /tariffs/{country_code}/{party_id}/{tariff_id}` — upsert
    /// - `DELETE /tariffs/{country_code}/{party_id}/{tariff_id}` — remove
    ///
    /// OCPI routing headers (`OCPI-from/to-party-id/country-code`) are accepted
    /// on all routes; they are not enforced at this layer.
    pub fn tariffs_router(config: Arc<TariffsConfig>) -> Router {
        Router::new()
            .route("/tariffs", get(tariffs_list))
            .route(
                "/tariffs/{country_code}/{party_id}/{tariff_id}",
                get(tariffs_get).put(tariffs_put).delete(tariffs_delete),
            )
            .with_state(config)
    }

    async fn tariffs_list(
        State(cfg): State<Arc<TariffsConfig>>,
        Query(params): Query<PaginatedParams>,
    ) -> Response {
        use ocpi_types::chrono::TimeZone as _;
        let date_from = params.date_from.unwrap_or_else(|| {
            ocpi_types::Utc
                .with_ymd_and_hms(1970, 1, 1, 0, 0, 0)
                .single()
                .expect("epoch is valid")
        });
        let offset = params.offset.unwrap_or(0);
        let limit = params.limit.unwrap_or(DEFAULT_LIMIT);

        let (items, total) = cfg.list(date_from, params.date_to, offset, limit);
        let page = OcpiPaged::new(items, offset, limit, total);
        let next_offset = page.next_offset();
        let body = page.into_response();

        let mut response = Json(body).into_response();
        let hdrs = response.headers_mut();
        if let Ok(v) = total.to_string().parse() {
            hdrs.insert("x-total-count", v);
        }
        if let Ok(v) = limit.to_string().parse() {
            hdrs.insert("x-limit", v);
        }
        if let Some(next_off) = next_offset {
            let link = format!("</tariffs?offset={next_off}&limit={limit}>; rel=\"next\"");
            if let Ok(v) = link.parse() {
                hdrs.insert("link", v);
            }
        }

        response
    }

    async fn tariffs_get(
        State(cfg): State<Arc<TariffsConfig>>,
        Path((country_code, party_id, tariff_id)): Path<(String, String, String)>,
    ) -> Response {
        match cfg.get(&country_code, &party_id, &tariff_id) {
            Some(tariff) => Json(OcpiResponse::success(tariff)).into_response(),
            None => (
                StatusCode::NOT_FOUND,
                Json(OcpiResponse::<Tariff>::error(
                    OcpiStatusCode::UnknownLocation,
                    format!("tariff {tariff_id} not found"),
                )),
            )
                .into_response(),
        }
    }

    async fn tariffs_put(
        State(cfg): State<Arc<TariffsConfig>>,
        Path((country_code, party_id, tariff_id)): Path<(String, String, String)>,
        Json(tariff): Json<Tariff>,
    ) -> Response {
        cfg.put(&country_code, &party_id, &tariff_id, tariff);
        Json(OcpiResponse::<Tariff>::success_empty()).into_response()
    }

    async fn tariffs_delete(
        State(cfg): State<Arc<TariffsConfig>>,
        Path((country_code, party_id, tariff_id)): Path<(String, String, String)>,
    ) -> Response {
        match cfg.delete(&country_code, &party_id, &tariff_id) {
            Ok(()) => Json(OcpiResponse::<Tariff>::success_empty()).into_response(),
            Err(ServerError::NotFound) => (
                StatusCode::NOT_FOUND,
                Json(OcpiResponse::<Tariff>::error(
                    OcpiStatusCode::UnknownLocation,
                    format!("tariff {tariff_id} not found"),
                )),
            )
                .into_response(),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<Tariff>::error(
                    OcpiStatusCode::ServerError,
                    "internal error",
                )),
            )
                .into_response(),
        }
    }

    // ── Tariffs (OCPI 2.1.1) ────────────────────────────────────────────────

    /// Build an axum router for the **OCPI 2.1.1** Tariffs module, the 2.1.1
    /// counterpart to [`tariffs_router`].
    ///
    /// Exposes:
    /// - `GET    /tariffs` — paginated list (Sender interface, CPO; §11.2.1)
    /// - `GET    /tariffs/{country_code}/{party_id}/{tariff_id}` — single tariff
    /// - `PUT    /tariffs/{country_code}/{party_id}/{tariff_id}` — upsert
    /// - `DELETE /tariffs/{country_code}/{party_id}/{tariff_id}` — remove
    ///
    /// Paths are identical to 2.2.1; only the [`Tariff2111`] object shape
    /// differs. The 2.1.1 Receiver `PATCH` (partial updates, §11.2.2) is
    /// deferred for parity with [`tariffs_router`].
    pub fn tariffs_2_1_1_router(config: Arc<Tariffs2111Config>) -> Router {
        Router::new()
            .route("/tariffs", get(tariffs_2_1_1_list))
            .route(
                "/tariffs/{country_code}/{party_id}/{tariff_id}",
                get(tariffs_2_1_1_get)
                    .put(tariffs_2_1_1_put)
                    .delete(tariffs_2_1_1_delete),
            )
            .with_state(config)
    }

    async fn tariffs_2_1_1_list(
        State(cfg): State<Arc<Tariffs2111Config>>,
        Query(params): Query<PaginatedParams>,
    ) -> Response {
        use ocpi_types::chrono::TimeZone as _;
        let date_from = params.date_from.unwrap_or_else(|| {
            ocpi_types::Utc
                .with_ymd_and_hms(1970, 1, 1, 0, 0, 0)
                .single()
                .expect("epoch is valid")
        });
        let offset = params.offset.unwrap_or(0);
        let limit = params.limit.unwrap_or(DEFAULT_LIMIT);

        let (items, total) = cfg.list(date_from, params.date_to, offset, limit);
        let page = OcpiPaged::new(items, offset, limit, total);
        let next_offset = page.next_offset();
        let body = page.into_response();

        let mut response = Json(body).into_response();
        let hdrs = response.headers_mut();
        if let Ok(v) = total.to_string().parse() {
            hdrs.insert("x-total-count", v);
        }
        if let Ok(v) = limit.to_string().parse() {
            hdrs.insert("x-limit", v);
        }
        if let Some(next_off) = next_offset {
            let link = format!("</tariffs?offset={next_off}&limit={limit}>; rel=\"next\"");
            if let Ok(v) = link.parse() {
                hdrs.insert("link", v);
            }
        }

        response
    }

    async fn tariffs_2_1_1_get(
        State(cfg): State<Arc<Tariffs2111Config>>,
        Path((country_code, party_id, tariff_id)): Path<(String, String, String)>,
    ) -> Response {
        match cfg.get(&country_code, &party_id, &tariff_id) {
            Some(tariff) => Json(OcpiResponse::success(tariff)).into_response(),
            None => (
                StatusCode::NOT_FOUND,
                Json(OcpiResponse::<Tariff2111>::error(
                    OcpiStatusCode::UnknownLocation,
                    format!("tariff {tariff_id} not found"),
                )),
            )
                .into_response(),
        }
    }

    async fn tariffs_2_1_1_put(
        State(cfg): State<Arc<Tariffs2111Config>>,
        Path((country_code, party_id, tariff_id)): Path<(String, String, String)>,
        Json(tariff): Json<Tariff2111>,
    ) -> Response {
        cfg.put(&country_code, &party_id, &tariff_id, tariff);
        Json(OcpiResponse::<Tariff2111>::success_empty()).into_response()
    }

    async fn tariffs_2_1_1_delete(
        State(cfg): State<Arc<Tariffs2111Config>>,
        Path((country_code, party_id, tariff_id)): Path<(String, String, String)>,
    ) -> Response {
        match cfg.delete(&country_code, &party_id, &tariff_id) {
            Ok(()) => Json(OcpiResponse::<Tariff2111>::success_empty()).into_response(),
            Err(ServerError::NotFound) => (
                StatusCode::NOT_FOUND,
                Json(OcpiResponse::<Tariff2111>::error(
                    OcpiStatusCode::UnknownLocation,
                    format!("tariff {tariff_id} not found"),
                )),
            )
                .into_response(),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<Tariff2111>::error(
                    OcpiStatusCode::ServerError,
                    "internal error",
                )),
            )
                .into_response(),
        }
    }

    // ── Tokens ────────────────────────────────────────────────────────────────

    /// Build an axum router for the OCPI Tokens module.
    ///
    /// Exposes:
    /// - `GET  /tokens` — paginated list (sender interface, eMSP)
    /// - `GET  /tokens/{country_code}/{party_id}/{token_uid}?type=` — single token
    /// - `PUT  /tokens/{country_code}/{party_id}/{token_uid}?type=` — upsert
    /// - `PATCH /tokens/{country_code}/{party_id}/{token_uid}?type=` — merge-patch
    /// - `POST /tokens/{token_uid}/authorize?type=` — real-time authorization
    ///
    /// OCPI routing headers (`OCPI-from/to-party-id/country-code`) are accepted
    /// on all routes; they are not enforced at this layer.
    pub fn tokens_router(config: Arc<TokensConfig>) -> Router {
        Router::new()
            .route("/tokens", get(tokens_list))
            .route(
                "/tokens/{country_code}/{party_id}/{token_uid}",
                get(tokens_get).put(tokens_put).patch(tokens_patch),
            )
            .route("/tokens/{token_uid}/authorize", post(tokens_authorize))
            .with_state(config)
    }

    #[derive(ocpi_types::serde::Deserialize)]
    #[serde(crate = "ocpi_types::serde")]
    struct TypeQuery {
        #[serde(rename = "type", default = "default_token_type")]
        token_type: TokenType,
    }

    fn default_token_type() -> TokenType {
        TokenType::Rfid
    }

    async fn tokens_list(
        State(cfg): State<Arc<TokensConfig>>,
        Query(params): Query<PaginatedParams>,
    ) -> Response {
        use ocpi_types::chrono::TimeZone as _;
        let date_from = params.date_from.unwrap_or_else(|| {
            ocpi_types::Utc
                .with_ymd_and_hms(1970, 1, 1, 0, 0, 0)
                .single()
                .expect("epoch is valid")
        });
        let offset = params.offset.unwrap_or(0);
        let limit = params.limit.unwrap_or(DEFAULT_LIMIT);

        let (items, total) = cfg.list(date_from, params.date_to, offset, limit);
        let page = OcpiPaged::new(items, offset, limit, total);
        let next_offset = page.next_offset();
        let body = page.into_response();

        let mut response = Json(body).into_response();
        let hdrs = response.headers_mut();
        if let Ok(v) = total.to_string().parse() {
            hdrs.insert("x-total-count", v);
        }
        if let Ok(v) = limit.to_string().parse() {
            hdrs.insert("x-limit", v);
        }
        if let Some(next_off) = next_offset {
            let link = format!("</tokens?offset={next_off}&limit={limit}>; rel=\"next\"");
            if let Ok(v) = link.parse() {
                hdrs.insert("link", v);
            }
        }

        response
    }

    async fn tokens_get(
        State(cfg): State<Arc<TokensConfig>>,
        Path((country_code, party_id, token_uid)): Path<(String, String, String)>,
        Query(q): Query<TypeQuery>,
    ) -> Response {
        match cfg.get(&country_code, &party_id, &token_uid, q.token_type) {
            Some(token) => Json(OcpiResponse::success(token)).into_response(),
            None => (
                StatusCode::NOT_FOUND,
                Json(OcpiResponse::<Token>::error(
                    OcpiStatusCode::UnknownLocation,
                    format!(
                        "token {token_uid}?type={} not found",
                        token_type_str(q.token_type)
                    ),
                )),
            )
                .into_response(),
        }
    }

    async fn tokens_put(
        State(cfg): State<Arc<TokensConfig>>,
        Path((country_code, party_id, token_uid)): Path<(String, String, String)>,
        Query(q): Query<TypeQuery>,
        Json(token): Json<Token>,
    ) -> Response {
        cfg.put(&country_code, &party_id, &token_uid, q.token_type, token);
        Json(OcpiResponse::<Token>::success_empty()).into_response()
    }

    async fn tokens_patch(
        State(cfg): State<Arc<TokensConfig>>,
        Path((country_code, party_id, token_uid)): Path<(String, String, String)>,
        Query(q): Query<TypeQuery>,
        Json(partial): Json<ocpi_types::serde_json::Value>,
    ) -> Response {
        match cfg.patch_json(&country_code, &party_id, &token_uid, q.token_type, partial) {
            Ok(()) => Json(OcpiResponse::<Token>::success_empty()).into_response(),
            Err(ServerError::NotFound) => (
                StatusCode::NOT_FOUND,
                Json(OcpiResponse::<Token>::error(
                    OcpiStatusCode::UnknownLocation,
                    format!(
                        "token {token_uid}?type={} not found",
                        token_type_str(q.token_type)
                    ),
                )),
            )
                .into_response(),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<Token>::error(
                    OcpiStatusCode::ServerError,
                    "internal error",
                )),
            )
                .into_response(),
        }
    }

    async fn tokens_authorize(
        State(cfg): State<Arc<TokensConfig>>,
        Path(token_uid): Path<String>,
        Query(q): Query<TypeQuery>,
        body: Option<Json<LocationReferences>>,
    ) -> Response {
        let location = body.map(|Json(loc)| loc);
        match cfg.authorize(&token_uid, q.token_type, location) {
            Ok(auth_info) => Json(OcpiResponse::success(auth_info)).into_response(),
            Err(ServerError::UnknownToken) => (
                StatusCode::NOT_FOUND,
                Json(OcpiResponse::<AuthorizationInfo>::error(
                    OcpiStatusCode::UnknownToken,
                    format!("token {token_uid} not known"),
                )),
            )
                .into_response(),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<AuthorizationInfo>::error(
                    OcpiStatusCode::ServerError,
                    "internal error",
                )),
            )
                .into_response(),
        }
    }

    // ── Tokens (OCPI 2.1.1) ─────────────────────────────────────────────────────

    /// Build an axum router for the OCPI **2.1.1** Tokens module.
    ///
    /// Exposes (mirrors [`tokens_router`], with the 2.1.1 [`Token2111`] shape):
    /// - `GET   /tokens` — paginated list (sender interface, eMSP)
    /// - `GET   /tokens/{country_code}/{party_id}/{token_uid}` — single
    /// - `PUT   /tokens/{country_code}/{party_id}/{token_uid}` — upsert
    /// - `PATCH /tokens/{country_code}/{party_id}/{token_uid}` — merge-patch
    /// - `POST  /tokens/{token_uid}/authorize` — real-time authorize
    ///
    /// The `?type=` query selects the [`TokenType2111`] (defaults to `RFID`).
    pub fn tokens_2_1_1_router(config: Arc<Tokens2111Config>) -> Router {
        Router::new()
            .route("/tokens", get(tokens_2_1_1_list))
            .route(
                "/tokens/{country_code}/{party_id}/{token_uid}",
                get(tokens_2_1_1_get)
                    .put(tokens_2_1_1_put)
                    .patch(tokens_2_1_1_patch),
            )
            .route(
                "/tokens/{token_uid}/authorize",
                post(tokens_2_1_1_authorize),
            )
            .with_state(config)
    }

    #[derive(ocpi_types::serde::Deserialize)]
    #[serde(crate = "ocpi_types::serde")]
    struct TypeQuery2111 {
        #[serde(rename = "type", default = "default_token_type_2_1_1")]
        token_type: TokenType2111,
    }

    fn default_token_type_2_1_1() -> TokenType2111 {
        TokenType2111::Rfid
    }

    async fn tokens_2_1_1_list(
        State(cfg): State<Arc<Tokens2111Config>>,
        Query(params): Query<PaginatedParams>,
    ) -> Response {
        use ocpi_types::chrono::TimeZone as _;
        let date_from = params.date_from.unwrap_or_else(|| {
            ocpi_types::Utc
                .with_ymd_and_hms(1970, 1, 1, 0, 0, 0)
                .single()
                .expect("epoch is valid")
        });
        let offset = params.offset.unwrap_or(0);
        let limit = params.limit.unwrap_or(DEFAULT_LIMIT);

        let (items, total) = cfg.list(date_from, params.date_to, offset, limit);
        let page = OcpiPaged::new(items, offset, limit, total);
        let next_offset = page.next_offset();
        let body = page.into_response();

        let mut response = Json(body).into_response();
        let hdrs = response.headers_mut();
        if let Ok(v) = total.to_string().parse() {
            hdrs.insert("x-total-count", v);
        }
        if let Ok(v) = limit.to_string().parse() {
            hdrs.insert("x-limit", v);
        }
        if let Some(next_off) = next_offset {
            let link = format!("</tokens?offset={next_off}&limit={limit}>; rel=\"next\"");
            if let Ok(v) = link.parse() {
                hdrs.insert("link", v);
            }
        }

        response
    }

    async fn tokens_2_1_1_get(
        State(cfg): State<Arc<Tokens2111Config>>,
        Path((country_code, party_id, token_uid)): Path<(String, String, String)>,
        Query(q): Query<TypeQuery2111>,
    ) -> Response {
        match cfg.get(&country_code, &party_id, &token_uid, q.token_type) {
            Some(token) => Json(OcpiResponse::success(token)).into_response(),
            None => (
                StatusCode::NOT_FOUND,
                Json(OcpiResponse::<Token2111>::error(
                    OcpiStatusCode::UnknownLocation,
                    format!(
                        "token {token_uid}?type={} not found",
                        token_type_2_1_1_str(q.token_type)
                    ),
                )),
            )
                .into_response(),
        }
    }

    async fn tokens_2_1_1_put(
        State(cfg): State<Arc<Tokens2111Config>>,
        Path((country_code, party_id, token_uid)): Path<(String, String, String)>,
        Query(q): Query<TypeQuery2111>,
        Json(token): Json<Token2111>,
    ) -> Response {
        cfg.put(&country_code, &party_id, &token_uid, q.token_type, token);
        Json(OcpiResponse::<Token2111>::success_empty()).into_response()
    }

    async fn tokens_2_1_1_patch(
        State(cfg): State<Arc<Tokens2111Config>>,
        Path((country_code, party_id, token_uid)): Path<(String, String, String)>,
        Query(q): Query<TypeQuery2111>,
        Json(partial): Json<ocpi_types::serde_json::Value>,
    ) -> Response {
        match cfg.patch_json(&country_code, &party_id, &token_uid, q.token_type, partial) {
            Ok(()) => Json(OcpiResponse::<Token2111>::success_empty()).into_response(),
            Err(ServerError::NotFound) => (
                StatusCode::NOT_FOUND,
                Json(OcpiResponse::<Token2111>::error(
                    OcpiStatusCode::UnknownLocation,
                    format!(
                        "token {token_uid}?type={} not found",
                        token_type_2_1_1_str(q.token_type)
                    ),
                )),
            )
                .into_response(),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<Token2111>::error(
                    OcpiStatusCode::ServerError,
                    "internal error",
                )),
            )
                .into_response(),
        }
    }

    async fn tokens_2_1_1_authorize(
        State(cfg): State<Arc<Tokens2111Config>>,
        Path(token_uid): Path<String>,
        Query(q): Query<TypeQuery2111>,
        body: Option<Json<LocationReferences2111>>,
    ) -> Response {
        let location = body.map(|Json(loc)| loc);
        match cfg.authorize(&token_uid, q.token_type, location) {
            Ok(auth_info) => Json(OcpiResponse::success(auth_info)).into_response(),
            Err(ServerError::UnknownToken) => (
                StatusCode::NOT_FOUND,
                Json(OcpiResponse::<AuthorizationInfo2111>::error(
                    OcpiStatusCode::UnknownToken,
                    format!("token {token_uid} not known"),
                )),
            )
                .into_response(),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<AuthorizationInfo2111>::error(
                    OcpiStatusCode::ServerError,
                    "internal error",
                )),
            )
                .into_response(),
        }
    }

    // ── Commands ──────────────────────────────────────────────────────────────

    /// Build an axum router for the OCPI Commands module.
    ///
    /// Exposes:
    /// - `POST /commands/CANCEL_RESERVATION` — receiver (CPO)
    /// - `POST /commands/RESERVE_NOW` — receiver (CPO)
    /// - `POST /commands/START_SESSION` — receiver (CPO)
    /// - `POST /commands/STOP_SESSION` — receiver (CPO)
    /// - `POST /commands/UNLOCK_CONNECTOR` — receiver (CPO)
    /// - `POST /commands/{command_type}/result` — sender result callback (eMSP)
    ///
    /// The default [`CommandsConfig`] responds `NOT_SUPPORTED` to every command.
    /// Wire a custom implementation for real CPO/OCPP bridging.
    pub fn commands_router(config: Arc<CommandsConfig>) -> Router {
        Router::new()
            .route(
                "/commands/CANCEL_RESERVATION",
                post(cmds_cancel_reservation),
            )
            .route("/commands/RESERVE_NOW", post(cmds_reserve_now))
            .route("/commands/START_SESSION", post(cmds_start_session))
            .route("/commands/STOP_SESSION", post(cmds_stop_session))
            .route("/commands/UNLOCK_CONNECTOR", post(cmds_unlock_connector))
            .route("/commands/{command_type}/result", post(cmds_receive_result))
            .with_state(config)
    }

    async fn cmds_cancel_reservation(
        State(cfg): State<Arc<CommandsConfig>>,
        Json(cmd): Json<CancelReservation>,
    ) -> Response {
        match cfg.handle_cancel_reservation(cmd).await {
            Ok(resp) => Json(OcpiResponse::success(resp)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<CommandResponse>::error(
                    e.status_code(),
                    e.to_string(),
                )),
            )
                .into_response(),
        }
    }

    async fn cmds_reserve_now(
        State(cfg): State<Arc<CommandsConfig>>,
        Json(cmd): Json<ReserveNow>,
    ) -> Response {
        match cfg.handle_reserve_now(cmd).await {
            Ok(resp) => Json(OcpiResponse::success(resp)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<CommandResponse>::error(
                    e.status_code(),
                    e.to_string(),
                )),
            )
                .into_response(),
        }
    }

    async fn cmds_start_session(
        State(cfg): State<Arc<CommandsConfig>>,
        Json(cmd): Json<StartSession>,
    ) -> Response {
        match cfg.handle_start_session(cmd).await {
            Ok(resp) => Json(OcpiResponse::success(resp)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<CommandResponse>::error(
                    e.status_code(),
                    e.to_string(),
                )),
            )
                .into_response(),
        }
    }

    async fn cmds_stop_session(
        State(cfg): State<Arc<CommandsConfig>>,
        Json(cmd): Json<StopSession>,
    ) -> Response {
        match cfg.handle_stop_session(cmd).await {
            Ok(resp) => Json(OcpiResponse::success(resp)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<CommandResponse>::error(
                    e.status_code(),
                    e.to_string(),
                )),
            )
                .into_response(),
        }
    }

    async fn cmds_unlock_connector(
        State(cfg): State<Arc<CommandsConfig>>,
        Json(cmd): Json<UnlockConnector>,
    ) -> Response {
        match cfg.handle_unlock_connector(cmd).await {
            Ok(resp) => Json(OcpiResponse::success(resp)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<CommandResponse>::error(
                    e.status_code(),
                    e.to_string(),
                )),
            )
                .into_response(),
        }
    }

    async fn cmds_receive_result(
        State(cfg): State<Arc<CommandsConfig>>,
        Path(command_type_str): Path<String>,
        Json(result): Json<CommandResult>,
    ) -> Response {
        let command_type = match ocpi_types::serde_json::from_value::<CommandType>(
            ocpi_types::serde_json::Value::String(command_type_str.clone()),
        ) {
            Ok(ct) => ct,
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(OcpiResponse::<CommandResponse>::error(
                        OcpiStatusCode::InvalidParameters,
                        format!("unknown command type: {command_type_str}"),
                    )),
                )
                    .into_response()
            }
        };
        match cfg.receive_command_result(command_type, result).await {
            Ok(()) => Json(OcpiResponse::<CommandResult>::success_empty()).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<CommandResult>::error(
                    e.status_code(),
                    e.to_string(),
                )),
            )
                .into_response(),
        }
    }

    // ── Commands (OCPI 2.1.1) ──────────────────────────────────────────────────

    /// Build an axum router for the OCPI **2.1.1** Commands module.
    ///
    /// Exposes:
    /// - `POST /commands/RESERVE_NOW` — receiver (CPO)
    /// - `POST /commands/START_SESSION` — receiver (CPO)
    /// - `POST /commands/STOP_SESSION` — receiver (CPO)
    /// - `POST /commands/UNLOCK_CONNECTOR` — receiver (CPO)
    /// - `POST /commands/{command_type}/result` — sender result callback (eMSP)
    ///
    /// Mirrors [`commands_router`] but for the 2.1.1 wire shape: there is **no**
    /// `CANCEL_RESERVATION` route (a 2.2 addition), [`StartSession2111`] carries
    /// the full [`Token2111`] object, and the async result callback body is a
    /// [`CommandResponse2111`] (2.1.1 reuses `CommandResponse` for both phases;
    /// there is no distinct `CommandResult`).
    ///
    /// The default [`Commands2111Config`] responds `NOT_SUPPORTED` to every
    /// command. Wire a custom implementation for real CPO/OCPP bridging.
    pub fn commands_2_1_1_router(config: Arc<Commands2111Config>) -> Router {
        Router::new()
            .route("/commands/RESERVE_NOW", post(cmds_2111_reserve_now))
            .route("/commands/START_SESSION", post(cmds_2111_start_session))
            .route("/commands/STOP_SESSION", post(cmds_2111_stop_session))
            .route(
                "/commands/UNLOCK_CONNECTOR",
                post(cmds_2111_unlock_connector),
            )
            .route(
                "/commands/{command_type}/result",
                post(cmds_2111_receive_result),
            )
            .with_state(config)
    }

    async fn cmds_2111_reserve_now(
        State(cfg): State<Arc<Commands2111Config>>,
        Json(cmd): Json<ReserveNow2111>,
    ) -> Response {
        match cfg.handle_reserve_now(cmd).await {
            Ok(resp) => Json(OcpiResponse::success(resp)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<CommandResponse2111>::error(
                    e.status_code(),
                    e.to_string(),
                )),
            )
                .into_response(),
        }
    }

    async fn cmds_2111_start_session(
        State(cfg): State<Arc<Commands2111Config>>,
        Json(cmd): Json<StartSession2111>,
    ) -> Response {
        match cfg.handle_start_session(cmd).await {
            Ok(resp) => Json(OcpiResponse::success(resp)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<CommandResponse2111>::error(
                    e.status_code(),
                    e.to_string(),
                )),
            )
                .into_response(),
        }
    }

    async fn cmds_2111_stop_session(
        State(cfg): State<Arc<Commands2111Config>>,
        Json(cmd): Json<StopSession2111>,
    ) -> Response {
        match cfg.handle_stop_session(cmd).await {
            Ok(resp) => Json(OcpiResponse::success(resp)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<CommandResponse2111>::error(
                    e.status_code(),
                    e.to_string(),
                )),
            )
                .into_response(),
        }
    }

    async fn cmds_2111_unlock_connector(
        State(cfg): State<Arc<Commands2111Config>>,
        Json(cmd): Json<UnlockConnector2111>,
    ) -> Response {
        match cfg.handle_unlock_connector(cmd).await {
            Ok(resp) => Json(OcpiResponse::success(resp)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<CommandResponse2111>::error(
                    e.status_code(),
                    e.to_string(),
                )),
            )
                .into_response(),
        }
    }

    async fn cmds_2111_receive_result(
        State(cfg): State<Arc<Commands2111Config>>,
        Path(command_type_str): Path<String>,
        Json(result): Json<CommandResponse2111>,
    ) -> Response {
        let command_type = match ocpi_types::serde_json::from_value::<CommandType2111>(
            ocpi_types::serde_json::Value::String(command_type_str.clone()),
        ) {
            Ok(ct) => ct,
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(OcpiResponse::<CommandResponse2111>::error(
                        OcpiStatusCode::InvalidParameters,
                        format!("unknown command type: {command_type_str}"),
                    )),
                )
                    .into_response()
            }
        };
        match cfg.receive_command_result(command_type, result).await {
            Ok(()) => Json(OcpiResponse::<CommandResponse2111>::success_empty()).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<CommandResponse2111>::error(
                    e.status_code(),
                    e.to_string(),
                )),
            )
                .into_response(),
        }
    }

    // ── Commands 2.1.1 handler tests (#124) ────────────────────────────────────
    //
    // Drive the `Commands2111Handler` placeholder directly (no live socket):
    // every receiver command must ack `NOT_SUPPORTED`, the async result callback
    // must be accepted, and the router must construct. HTTP-level round-trips
    // belong in the dedicated 2.1.1 e2e issue (#141), per `nightly/LEARNINGS.md`.
    // Spec: OCPI 2.1.1 — *Commands* module (§13).
    #[cfg(test)]
    mod commands_2111_tests {
        use super::*;
        use ocpi_types::common::{CiString36, CiString64, Url};
        use ocpi_types::v2_1_1::{CommandResponseType, StartSession, Token, TokenType};
        use ocpi_types::v2_2_1::WhitelistType;

        /// A spec-shaped 2.1.1 `Token` (full object, `auth_id` keying).
        fn token() -> Token {
            Token {
                uid: CiString36::try_from("12345678905880").unwrap(),
                token_type: TokenType::Rfid,
                auth_id: CiString36::try_from("DE8ACC12E46L89").unwrap(),
                visual_number: None,
                issuer: CiString64::try_from("TheNewMotion").unwrap(),
                valid: true,
                whitelist: WhitelistType::Always,
                language: None,
                last_updated: "2018-12-10T17:25:10Z".parse().unwrap(),
            }
        }

        /// The 2.1.1 `START_SESSION` body carries the **full** `Token` object.
        fn start_session() -> StartSession {
            StartSession {
                response_url: Url::try_from(
                    "https://msp.example/ocpi/2.1.1/commands/START_SESSION/42",
                )
                .unwrap(),
                token: token(),
                location_id: "LOC1".to_string(),
                evse_uid: Some("EVSE1".to_string()),
            }
        }

        #[tokio::test]
        async fn placeholder_acks_start_session_not_supported() {
            let cfg = Commands2111Config::new();
            let resp = cfg.handle_start_session(start_session()).await.unwrap();
            assert_eq!(resp.result, CommandResponseType::NotSupported);
        }

        #[tokio::test]
        async fn placeholder_accepts_async_result_callback() {
            // In 2.1.1 the async callback body is a `CommandResponse` (not a
            // distinct `CommandResult`); the eMSP-side sink accepts it.
            let cfg = Commands2111Config::new();
            let result = Commands2111Config::not_supported_response();
            cfg.receive_command_result(CommandType2111::StartSession, result)
                .await
                .unwrap();
        }

        #[test]
        fn not_supported_response_carries_only_result() {
            let resp = Commands2111Config::not_supported_response();
            assert_eq!(resp.result, CommandResponseType::NotSupported);
            // 2.1.1 CommandResponse has no `timeout`/`message` fields to check —
            // the type itself enforces that. A serde round-trip confirms shape.
            let json = ocpi_types::serde_json::to_string(&resp).unwrap();
            assert_eq!(json, r#"{"result":"NOT_SUPPORTED"}"#);
        }

        #[test]
        fn router_constructs_without_panic() {
            let _router = commands_2_1_1_router(Arc::new(Commands2111Config::new()));
        }
    }

    // ── Commands (OCPI 2.2) ─────────────────────────────────────────────────────

    /// Build an axum router for the OCPI **2.2** Commands module.
    ///
    /// Exposes the full receiver surface:
    /// - `POST /commands/CANCEL_RESERVATION` — receiver (CPO)
    /// - `POST /commands/RESERVE_NOW` — receiver (CPO)
    /// - `POST /commands/START_SESSION` — receiver (CPO)
    /// - `POST /commands/STOP_SESSION` — receiver (CPO)
    /// - `POST /commands/UNLOCK_CONNECTOR` — receiver (CPO)
    /// - `POST /commands/{command_type}/result` — sender result callback (eMSP)
    ///
    /// Mirrors [`commands_router`] (2.2.1) exactly, except `START_SESSION` is
    /// deserialized into the **2.2** [`StartSession22`] — no `connector_id`. Every
    /// other body/response type is the wire-identical 2.2.1 type. Landing the
    /// command in the 2.2 type is the point of this router: a 2.2 Charge Point
    /// never agreed to honour a Sender-pinned connector, so the field simply does
    /// not exist to carry.
    ///
    /// The default [`Commands22Config`] responds `NOT_SUPPORTED` to every command.
    /// Wire a custom implementation for real CPO/OCPP bridging.
    pub fn commands_2_2_router(config: Arc<Commands22Config>) -> Router {
        Router::new()
            .route(
                "/commands/CANCEL_RESERVATION",
                post(cmds_22_cancel_reservation),
            )
            .route("/commands/RESERVE_NOW", post(cmds_22_reserve_now))
            .route("/commands/START_SESSION", post(cmds_22_start_session))
            .route("/commands/STOP_SESSION", post(cmds_22_stop_session))
            .route("/commands/UNLOCK_CONNECTOR", post(cmds_22_unlock_connector))
            .route(
                "/commands/{command_type}/result",
                post(cmds_22_receive_result),
            )
            .with_state(config)
    }

    async fn cmds_22_cancel_reservation(
        State(cfg): State<Arc<Commands22Config>>,
        Json(cmd): Json<CancelReservation>,
    ) -> Response {
        match cfg.handle_cancel_reservation(cmd).await {
            Ok(resp) => Json(OcpiResponse::success(resp)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<CommandResponse>::error(
                    e.status_code(),
                    e.to_string(),
                )),
            )
                .into_response(),
        }
    }

    async fn cmds_22_reserve_now(
        State(cfg): State<Arc<Commands22Config>>,
        Json(cmd): Json<ReserveNow>,
    ) -> Response {
        match cfg.handle_reserve_now(cmd).await {
            Ok(resp) => Json(OcpiResponse::success(resp)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<CommandResponse>::error(
                    e.status_code(),
                    e.to_string(),
                )),
            )
                .into_response(),
        }
    }

    async fn cmds_22_start_session(
        State(cfg): State<Arc<Commands22Config>>,
        Json(cmd): Json<StartSession22>,
    ) -> Response {
        match cfg.handle_start_session(cmd).await {
            Ok(resp) => Json(OcpiResponse::success(resp)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<CommandResponse>::error(
                    e.status_code(),
                    e.to_string(),
                )),
            )
                .into_response(),
        }
    }

    async fn cmds_22_stop_session(
        State(cfg): State<Arc<Commands22Config>>,
        Json(cmd): Json<StopSession>,
    ) -> Response {
        match cfg.handle_stop_session(cmd).await {
            Ok(resp) => Json(OcpiResponse::success(resp)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<CommandResponse>::error(
                    e.status_code(),
                    e.to_string(),
                )),
            )
                .into_response(),
        }
    }

    async fn cmds_22_unlock_connector(
        State(cfg): State<Arc<Commands22Config>>,
        Json(cmd): Json<UnlockConnector>,
    ) -> Response {
        match cfg.handle_unlock_connector(cmd).await {
            Ok(resp) => Json(OcpiResponse::success(resp)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<CommandResponse>::error(
                    e.status_code(),
                    e.to_string(),
                )),
            )
                .into_response(),
        }
    }

    async fn cmds_22_receive_result(
        State(cfg): State<Arc<Commands22Config>>,
        Path(command_type_str): Path<String>,
        Json(result): Json<CommandResult>,
    ) -> Response {
        let command_type = match ocpi_types::serde_json::from_value::<CommandType>(
            ocpi_types::serde_json::Value::String(command_type_str.clone()),
        ) {
            Ok(ct) => ct,
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(OcpiResponse::<CommandResponse>::error(
                        OcpiStatusCode::InvalidParameters,
                        format!("unknown command type: {command_type_str}"),
                    )),
                )
                    .into_response()
            }
        };
        match cfg.receive_command_result(command_type, result).await {
            Ok(()) => Json(OcpiResponse::<CommandResult>::success_empty()).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<CommandResult>::error(
                    e.status_code(),
                    e.to_string(),
                )),
            )
                .into_response(),
        }
    }

    // ── Commands 2.2 handler tests (#165) ──────────────────────────────────────
    //
    // Drive the `Commands22Handler` placeholder directly (no live socket): the
    // 2.2 `START_SESSION` lands in `v2_2::StartSession` (no `connector_id`), a
    // stray `connector_id` from a non-conformant peer does not survive the 2.2
    // deserialize, every receiver command acks `NOT_SUPPORTED`, and the router
    // constructs. Full HTTP client↔server round-trips live in the client crate's
    // `m7_commands_2_2` integration test. Spec: `specs/ocpi/2.2/mod_commands.asciidoc`.
    #[cfg(test)]
    mod commands_22_tests {
        use super::*;
        use ocpi_types::v2_2_1::CommandResponseType;

        /// A spec-shaped 2.2 `START_SESSION` (no `connector_id`).
        fn start_session_json() -> &'static str {
            r#"{
                "response_url": "https://msp.example/ocpi/2.2/commands/START_SESSION/42",
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
            }"#
        }

        #[tokio::test]
        async fn placeholder_acks_start_session_not_supported() {
            let cmd: StartSession22 =
                ocpi_types::serde_json::from_str(start_session_json()).unwrap();
            let cfg = Commands22Config::new();
            let resp = cfg.handle_start_session(cmd).await.unwrap();
            assert_eq!(resp.result, CommandResponseType::NotSupported);
            // The 2.2 CommandResponse keeps the 2.2.1 `timeout`/`message` fields.
            assert_eq!(resp.timeout, 30);
            assert!(resp.message.is_empty());
        }

        #[test]
        fn start_session_2_2_drops_stray_connector_id() {
            // A non-conformant peer pins a `connector_id`; the 2.2 receiver type
            // has no such field, so it never survives into the session.
            let with_connector = start_session_json().replace(
                "\"LOC1\",",
                "\"LOC1\",\n                \"connector_id\": \"1\",",
            );
            let cmd: StartSession22 = ocpi_types::serde_json::from_str(&with_connector).unwrap();
            let out = ocpi_types::serde_json::to_string(&cmd).unwrap();
            assert!(
                !out.contains("connector_id"),
                "2.2 START_SESSION must not resurrect a connector_id: {out}"
            );
        }

        #[tokio::test]
        async fn placeholder_acks_every_receiver_command_not_supported() {
            let cfg = Commands22Config::new();
            let cancel: CancelReservation = ocpi_types::serde_json::from_str(
                r#"{"response_url":"https://msp.example/cmd","reservation_id":"res-1"}"#,
            )
            .unwrap();
            assert_eq!(
                cfg.handle_cancel_reservation(cancel).await.unwrap().result,
                CommandResponseType::NotSupported
            );
            let stop: StopSession = ocpi_types::serde_json::from_str(
                r#"{"response_url":"https://msp.example/cmd","session_id":"sess-1"}"#,
            )
            .unwrap();
            assert_eq!(
                cfg.handle_stop_session(stop).await.unwrap().result,
                CommandResponseType::NotSupported
            );
            let unlock: UnlockConnector = ocpi_types::serde_json::from_str(
                r#"{"response_url":"https://msp.example/cmd","location_id":"LOC1","evse_uid":"EVSE1","connector_id":"1"}"#,
            )
            .unwrap();
            assert_eq!(
                cfg.handle_unlock_connector(unlock).await.unwrap().result,
                CommandResponseType::NotSupported
            );
        }

        #[tokio::test]
        async fn placeholder_accepts_async_result_callback() {
            // In 2.2 the async callback body is a distinct `CommandResult`.
            let cfg = Commands22Config::new();
            let result: CommandResult =
                ocpi_types::serde_json::from_str(r#"{"result":"ACCEPTED"}"#).unwrap();
            cfg.receive_command_result(CommandType::StartSession, result)
                .await
                .unwrap();
        }

        #[test]
        fn router_constructs_without_panic() {
            let _router = commands_2_2_router(Arc::new(Commands22Config::new()));
        }
    }

    // ── ChargingProfiles ────────────────────────────────────────────────────────

    /// Query parameters for the receiver `GET /chargingprofiles/{session_id}`.
    #[derive(ocpi_types::serde::Deserialize)]
    #[serde(crate = "ocpi_types::serde")]
    struct GetActiveProfileQuery {
        duration: u32,
        response_url: String,
    }

    /// Query parameter for the receiver `DELETE /chargingprofiles/{session_id}`.
    #[derive(ocpi_types::serde::Deserialize)]
    #[serde(crate = "ocpi_types::serde")]
    struct ClearProfileQuery {
        response_url: String,
    }

    /// Build an axum router for the OCPI ChargingProfiles module.
    ///
    /// This is a **Functional Module** — OCPI routing headers are required on all
    /// calls (attach them at the host layer; the router does not enforce them).
    ///
    /// Receiver interface (typically CPO):
    /// - `GET    /chargingprofiles/{session_id}?duration={n}&response_url={url}`
    /// - `PUT    /chargingprofiles/{session_id}` — body [`SetChargingProfile`]
    /// - `DELETE /chargingprofiles/{session_id}?response_url={url}`
    ///
    /// Sender interface (typically SCSP/eMSP — async result callbacks):
    /// - `POST   /chargingprofiles/{session_id}/activeprofile` — [`ActiveChargingProfileResult`]
    /// - `POST   /chargingprofiles/{session_id}/result` — [`ChargingProfileResult`]
    /// - `POST   /chargingprofiles/{session_id}/clearprofile` — [`ClearProfileResult`]
    ///
    /// The default [`ChargingProfilesConfig`] responds `NOT_SUPPORTED` to every
    /// receiver request and accepts every result callback.
    ///
    /// The Sender's proactive `ActiveChargingProfile`-update `PUT` is **not**
    /// mounted here — it shares the `PUT /chargingprofiles/{session_id}` path
    /// with the receiver `SetChargingProfile` `PUT` but carries a different body
    /// and is implemented by a different market role. Mount
    /// [`charging_profiles_sender_router`] for the SCSP/eMSP Sender interface.
    pub fn charging_profiles_router(config: Arc<ChargingProfilesConfig>) -> Router {
        Router::new()
            .route(
                "/chargingprofiles/{session_id}",
                get(cp_get_active)
                    .put(cp_set_profile)
                    .delete(cp_clear_profile),
            )
            .route(
                "/chargingprofiles/{session_id}/activeprofile",
                post(cp_receive_active_result),
            )
            .route(
                "/chargingprofiles/{session_id}/result",
                post(cp_receive_profile_result),
            )
            .route(
                "/chargingprofiles/{session_id}/clearprofile",
                post(cp_receive_clear_result),
            )
            .with_state(config)
    }

    async fn cp_get_active(
        State(cfg): State<Arc<ChargingProfilesConfig>>,
        Path(session_id): Path<String>,
        Query(q): Query<GetActiveProfileQuery>,
    ) -> Response {
        match cfg
            .get_active_profile(&session_id, q.duration, &q.response_url)
            .await
        {
            Ok(resp) => Json(OcpiResponse::success(resp)).into_response(),
            Err(e) => cp_error_response(&e),
        }
    }

    async fn cp_set_profile(
        State(cfg): State<Arc<ChargingProfilesConfig>>,
        Path(session_id): Path<String>,
        Json(profile): Json<SetChargingProfile>,
    ) -> Response {
        match cfg.set_charging_profile(&session_id, profile).await {
            Ok(resp) => Json(OcpiResponse::success(resp)).into_response(),
            Err(e) => cp_error_response(&e),
        }
    }

    async fn cp_clear_profile(
        State(cfg): State<Arc<ChargingProfilesConfig>>,
        Path(session_id): Path<String>,
        Query(q): Query<ClearProfileQuery>,
    ) -> Response {
        match cfg
            .clear_charging_profile(&session_id, &q.response_url)
            .await
        {
            Ok(resp) => Json(OcpiResponse::success(resp)).into_response(),
            Err(e) => cp_error_response(&e),
        }
    }

    /// Shared error mapping for the receiver methods (all return `ChargingProfileResponse`).
    fn cp_error_response(e: &ServerError) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(OcpiResponse::<ChargingProfileResponse>::error(
                e.status_code(),
                e.to_string(),
            )),
        )
            .into_response()
    }

    async fn cp_receive_active_result(
        State(cfg): State<Arc<ChargingProfilesConfig>>,
        Path(_session_id): Path<String>,
        Json(result): Json<ActiveChargingProfileResult>,
    ) -> Response {
        match cfg.receive_active_profile_result(result).await {
            Ok(()) => {
                Json(OcpiResponse::<ActiveChargingProfileResult>::success_empty()).into_response()
            }
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<ActiveChargingProfileResult>::error(
                    e.status_code(),
                    e.to_string(),
                )),
            )
                .into_response(),
        }
    }

    async fn cp_receive_profile_result(
        State(cfg): State<Arc<ChargingProfilesConfig>>,
        Path(_session_id): Path<String>,
        Json(result): Json<ChargingProfileResult>,
    ) -> Response {
        match cfg.receive_charging_profile_result(result).await {
            Ok(()) => Json(OcpiResponse::<ChargingProfileResult>::success_empty()).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<ChargingProfileResult>::error(
                    e.status_code(),
                    e.to_string(),
                )),
            )
                .into_response(),
        }
    }

    async fn cp_receive_clear_result(
        State(cfg): State<Arc<ChargingProfilesConfig>>,
        Path(_session_id): Path<String>,
        Json(result): Json<ClearProfileResult>,
    ) -> Response {
        match cfg.receive_clear_profile_result(result).await {
            Ok(()) => Json(OcpiResponse::<ClearProfileResult>::success_empty()).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<ClearProfileResult>::error(
                    e.status_code(),
                    e.to_string(),
                )),
            )
                .into_response(),
        }
    }

    /// Build an axum router for the OCPI ChargingProfiles module **Sender
    /// interface** (typically implemented by an SCSP/eMSP).
    ///
    /// The Sender interface receives the three asynchronous Charge Point result
    /// callbacks *and* the proactive `ActiveChargingProfile` updates the Receiver
    /// (typically CPO) pushes:
    /// - `PUT  /chargingprofiles/{session_id}` — body [`ActiveChargingProfile`]
    /// - `POST /chargingprofiles/{session_id}/activeprofile` — [`ActiveChargingProfileResult`]
    /// - `POST /chargingprofiles/{session_id}/result` — [`ChargingProfileResult`]
    /// - `POST /chargingprofiles/{session_id}/clearprofile` — [`ClearProfileResult`]
    ///
    /// This is a **separate** router from [`charging_profiles_router`] (the
    /// Receiver/CPO interface) on purpose: the `PUT /chargingprofiles/{session_id}`
    /// path is shared, but on the Receiver it carries a [`SetChargingProfile`] and
    /// on the Sender an [`ActiveChargingProfile`]. The two interfaces belong to
    /// different market roles, so a CPO mounts [`charging_profiles_router`] and an
    /// SCSP/eMSP mounts this one — they are never mounted on the same path prefix.
    ///
    /// The default [`ChargingProfilesConfig`] accepts (no-ops) every callback and
    /// update.
    ///
    /// Spec: `mod_charging_profiles.asciidoc` §Sender Interface.
    pub fn charging_profiles_sender_router(config: Arc<ChargingProfilesConfig>) -> Router {
        Router::new()
            .route(
                "/chargingprofiles/{session_id}",
                put(cp_receive_active_update),
            )
            .route(
                "/chargingprofiles/{session_id}/activeprofile",
                post(cp_receive_active_result),
            )
            .route(
                "/chargingprofiles/{session_id}/result",
                post(cp_receive_profile_result),
            )
            .route(
                "/chargingprofiles/{session_id}/clearprofile",
                post(cp_receive_clear_result),
            )
            .with_state(config)
    }

    async fn cp_receive_active_update(
        State(cfg): State<Arc<ChargingProfilesConfig>>,
        Path(session_id): Path<String>,
        Json(profile): Json<ActiveChargingProfile>,
    ) -> Response {
        match cfg
            .receive_active_profile_update(&session_id, profile)
            .await
        {
            Ok(()) => Json(OcpiResponse::<ActiveChargingProfile>::success_empty()).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OcpiResponse::<ActiveChargingProfile>::error(
                    e.status_code(),
                    e.to_string(),
                )),
            )
                .into_response(),
        }
    }

    // ── HubClientInfo ─────────────────────────────────────────────────────────

    /// Build an axum router for the OCPI HubClientInfo module.
    ///
    /// This is a **Configuration Module** — OCPI routing headers are NOT
    /// required on these endpoints.
    ///
    /// Exposes:
    /// - `GET  /clientinfo` — paginated list (Sender/Hub interface)
    /// - `GET  /clientinfo/{country_code}/{party_id}` — single entry (Receiver)
    /// - `PUT  /clientinfo/{country_code}/{party_id}` — upsert (Receiver)
    pub fn hub_client_info_router(config: Arc<HubClientInfoConfig>) -> Router {
        Router::new()
            .route("/clientinfo", get(hub_client_info_list))
            .route(
                "/clientinfo/{country_code}/{party_id}",
                get(hub_client_info_get).put(hub_client_info_put),
            )
            .with_state(config)
    }

    async fn hub_client_info_list(
        State(cfg): State<Arc<HubClientInfoConfig>>,
        Query(params): Query<PaginatedParams>,
    ) -> Response {
        use ocpi_types::chrono::TimeZone as _;
        let date_from = params.date_from.unwrap_or_else(|| {
            ocpi_types::Utc
                .with_ymd_and_hms(1970, 1, 1, 0, 0, 0)
                .single()
                .expect("epoch is valid")
        });
        let offset = params.offset.unwrap_or(0);
        let limit = params.limit.unwrap_or(DEFAULT_LIMIT);

        let (items, total) = cfg.list(date_from, params.date_to, offset, limit);
        let page = OcpiPaged::new(items, offset, limit, total);
        let next_offset = page.next_offset();
        let body = page.into_response();

        let mut response = Json(body).into_response();
        let hdrs = response.headers_mut();
        if let Ok(v) = total.to_string().parse() {
            hdrs.insert("x-total-count", v);
        }
        if let Ok(v) = limit.to_string().parse() {
            hdrs.insert("x-limit", v);
        }
        if let Some(next_off) = next_offset {
            let link = format!("</clientinfo?offset={next_off}&limit={limit}>; rel=\"next\"");
            if let Ok(v) = link.parse() {
                hdrs.insert("link", v);
            }
        }

        response
    }

    async fn hub_client_info_get(
        State(cfg): State<Arc<HubClientInfoConfig>>,
        Path((country_code, party_id)): Path<(String, String)>,
    ) -> Response {
        match cfg.get(&country_code, &party_id) {
            Some(info) => Json(OcpiResponse::success(info)).into_response(),
            None => (
                StatusCode::NOT_FOUND,
                Json(OcpiResponse::<ClientInfo>::error(
                    OcpiStatusCode::UnknownLocation,
                    format!("no ClientInfo for {country_code}/{party_id}"),
                )),
            )
                .into_response(),
        }
    }

    async fn hub_client_info_put(
        State(cfg): State<Arc<HubClientInfoConfig>>,
        Path((country_code, party_id)): Path<(String, String)>,
        Json(info): Json<ClientInfo>,
    ) -> Response {
        cfg.put(&country_code, &party_id, info);
        Json(OcpiResponse::<ClientInfo>::success_empty()).into_response()
    }

    // ── Locations ─────────────────────────────────────────────────────────────

    /// Build an axum router for the OCPI Locations module (receiver interface).
    ///
    /// Exposes:
    /// - `GET   /locations` — paginated list (sender interface, CPO)
    /// - `GET/PUT/PATCH /locations/{country_code}/{party_id}/{location_id}`
    /// - `GET/PUT/PATCH /locations/{country_code}/{party_id}/{location_id}/{evse_uid}`
    /// - `GET/PUT/PATCH /locations/{country_code}/{party_id}/{location_id}/{evse_uid}/{connector_id}`
    ///
    /// EVSEs and Connectors are nested inside their parent Location; the
    /// sub-object routes locate the parent first. `PUT`/`PATCH` to a sub-object
    /// of an unknown Location (or EVSE) return `404`. OCPI routing headers
    /// (`OCPI-from/to-party-id/country-code`) are accepted on all routes; they
    /// are not enforced at this layer and can be validated by middleware.
    ///
    /// Spec: `specs/ocpi/2.2.1/mod_locations.asciidoc` — §Receiver Interface.
    pub fn locations_router(config: Arc<LocationsConfig>) -> Router {
        Router::new()
            .route("/locations", get(locations_list))
            .route(
                "/locations/{country_code}/{party_id}/{location_id}",
                get(location_get).put(location_put).patch(location_patch),
            )
            .route(
                "/locations/{country_code}/{party_id}/{location_id}/{evse_uid}",
                get(evse_get).put(evse_put).patch(evse_patch),
            )
            .route(
                "/locations/{country_code}/{party_id}/{location_id}/{evse_uid}/{connector_id}",
                get(connector_get).put(connector_put).patch(connector_patch),
            )
            .with_state(config)
    }

    /// `404 Not Found` with OCPI status `2003` (Unknown Location).
    fn location_not_found<T: ocpi_types::serde::Serialize>(msg: String) -> Response {
        (
            StatusCode::NOT_FOUND,
            Json(OcpiResponse::<T>::error(
                OcpiStatusCode::UnknownLocation,
                msg,
            )),
        )
            .into_response()
    }

    /// Map a [`ServerError`] from a `PUT`/`PATCH` into a `204`-style empty success
    /// or a `404`, for the response type `T`.
    fn write_result<T: ocpi_types::serde::Serialize>(
        result: Result<(), ServerError>,
        missing: &str,
    ) -> Response {
        match result {
            Ok(()) => Json(OcpiResponse::<T>::success_empty()).into_response(),
            Err(ServerError::NotFound) => location_not_found::<T>(format!("unknown {missing}")),
            Err(e) => {
                Json(OcpiResponse::<T>::error(e.status_code(), e.to_string())).into_response()
            }
        }
    }

    async fn locations_list(
        State(cfg): State<Arc<LocationsConfig>>,
        Query(params): Query<PaginatedParams>,
    ) -> Response {
        use ocpi_types::chrono::TimeZone as _;
        let date_from = params.date_from.unwrap_or_else(|| {
            ocpi_types::Utc
                .with_ymd_and_hms(1970, 1, 1, 0, 0, 0)
                .single()
                .expect("epoch is valid")
        });
        let offset = params.offset.unwrap_or(0);
        let limit = params.limit.unwrap_or(DEFAULT_LIMIT);

        let (items, total) = cfg.list(date_from, params.date_to, offset, limit);
        let page = OcpiPaged::new(items, offset, limit, total);
        let next_offset = page.next_offset();
        let body = page.into_response();

        let mut response = Json(body).into_response();
        let hdrs = response.headers_mut();
        if let Ok(v) = total.to_string().parse() {
            hdrs.insert("x-total-count", v);
        }
        if let Ok(v) = limit.to_string().parse() {
            hdrs.insert("x-limit", v);
        }
        if let Some(next_off) = next_offset {
            let link = format!("</locations?offset={next_off}&limit={limit}>; rel=\"next\"");
            if let Ok(v) = link.parse() {
                hdrs.insert("link", v);
            }
        }

        response
    }

    async fn location_get(
        State(cfg): State<Arc<LocationsConfig>>,
        Path((country_code, party_id, location_id)): Path<(String, String, String)>,
    ) -> Response {
        match cfg.get(&country_code, &party_id, &location_id) {
            Some(location) => Json(OcpiResponse::success(location)).into_response(),
            None => location_not_found::<Location>(format!(
                "no Location {country_code}/{party_id}/{location_id}"
            )),
        }
    }

    async fn location_put(
        State(cfg): State<Arc<LocationsConfig>>,
        Path((_country_code, _party_id, _location_id)): Path<(String, String, String)>,
        Json(location): Json<Location>,
    ) -> Response {
        cfg.put(location);
        Json(OcpiResponse::<Location>::success_empty()).into_response()
    }

    async fn location_patch(
        State(cfg): State<Arc<LocationsConfig>>,
        Path((country_code, party_id, location_id)): Path<(String, String, String)>,
        Json(partial): Json<ocpi_types::serde_json::Value>,
    ) -> Response {
        write_result::<Location>(
            cfg.patch_location(&country_code, &party_id, &location_id, partial),
            "Location",
        )
    }

    async fn evse_get(
        State(cfg): State<Arc<LocationsConfig>>,
        Path((country_code, party_id, location_id, evse_uid)): Path<(
            String,
            String,
            String,
            String,
        )>,
    ) -> Response {
        match cfg.get_evse(&country_code, &party_id, &location_id, &evse_uid) {
            Some(evse) => Json(OcpiResponse::success(evse)).into_response(),
            None => location_not_found::<Evse>(format!("no EVSE {evse_uid} in {location_id}")),
        }
    }

    async fn evse_put(
        State(cfg): State<Arc<LocationsConfig>>,
        Path((country_code, party_id, location_id, _evse_uid)): Path<(
            String,
            String,
            String,
            String,
        )>,
        Json(evse): Json<Evse>,
    ) -> Response {
        write_result::<Evse>(
            cfg.put_evse(&country_code, &party_id, &location_id, evse),
            "Location",
        )
    }

    async fn evse_patch(
        State(cfg): State<Arc<LocationsConfig>>,
        Path((country_code, party_id, location_id, evse_uid)): Path<(
            String,
            String,
            String,
            String,
        )>,
        Json(partial): Json<ocpi_types::serde_json::Value>,
    ) -> Response {
        write_result::<Evse>(
            cfg.patch_evse(&country_code, &party_id, &location_id, &evse_uid, partial),
            "Location or EVSE",
        )
    }

    async fn connector_get(
        State(cfg): State<Arc<LocationsConfig>>,
        Path((country_code, party_id, location_id, evse_uid, connector_id)): Path<(
            String,
            String,
            String,
            String,
            String,
        )>,
    ) -> Response {
        match cfg.get_connector(
            &country_code,
            &party_id,
            &location_id,
            &evse_uid,
            &connector_id,
        ) {
            Some(connector) => Json(OcpiResponse::success(connector)).into_response(),
            None => location_not_found::<Connector>(format!(
                "no Connector {connector_id} in {evse_uid}"
            )),
        }
    }

    async fn connector_put(
        State(cfg): State<Arc<LocationsConfig>>,
        Path((country_code, party_id, location_id, evse_uid, _connector_id)): Path<(
            String,
            String,
            String,
            String,
            String,
        )>,
        Json(connector): Json<Connector>,
    ) -> Response {
        write_result::<Connector>(
            cfg.put_connector(&country_code, &party_id, &location_id, &evse_uid, connector),
            "Location or EVSE",
        )
    }

    async fn connector_patch(
        State(cfg): State<Arc<LocationsConfig>>,
        Path((country_code, party_id, location_id, evse_uid, connector_id)): Path<(
            String,
            String,
            String,
            String,
            String,
        )>,
        Json(partial): Json<ocpi_types::serde_json::Value>,
    ) -> Response {
        write_result::<Connector>(
            cfg.patch_connector(
                &country_code,
                &party_id,
                &location_id,
                &evse_uid,
                &connector_id,
                partial,
            ),
            "Location, EVSE, or Connector",
        )
    }

    // ── Locations (OCPI 2.1.1) ─────────────────────────────────────────────────

    /// Build an axum router for the OCPI **2.1.1** Locations module **receiver**
    /// interface (eMSP side).
    ///
    /// Exposes GET/PUT/PATCH at all three object levels on the client-owned path
    /// `/locations/{country_code}/{party_id}/{location_id}[/{evse_uid}[/{connector_id}]]`
    /// — transport-identical to the 2.2.1 [`locations_router`], only the
    /// [`Location2111`] object shape differs. There is no sender `GET /locations`
    /// list route here; that is the CPO sender role, served client-side by
    /// `OcpiClient::get_locations_2_1_1`.
    ///
    /// Spec: OCPI 2.1.1 — *Locations* module, §2.2 eMSP Interface.
    pub fn locations_2_1_1_router(config: Arc<Locations2111Config>) -> Router {
        Router::new()
            .route(
                "/locations/{country_code}/{party_id}/{location_id}",
                get(location_2_1_1_get)
                    .put(location_2_1_1_put)
                    .patch(location_2_1_1_patch),
            )
            .route(
                "/locations/{country_code}/{party_id}/{location_id}/{evse_uid}",
                get(evse_2_1_1_get)
                    .put(evse_2_1_1_put)
                    .patch(evse_2_1_1_patch),
            )
            .route(
                "/locations/{country_code}/{party_id}/{location_id}/{evse_uid}/{connector_id}",
                get(connector_2_1_1_get)
                    .put(connector_2_1_1_put)
                    .patch(connector_2_1_1_patch),
            )
            .with_state(config)
    }

    async fn location_2_1_1_get(
        State(cfg): State<Arc<Locations2111Config>>,
        Path((country_code, party_id, location_id)): Path<(String, String, String)>,
    ) -> Response {
        match cfg.get(&country_code, &party_id, &location_id) {
            Some(location) => Json(OcpiResponse::success(location)).into_response(),
            None => location_not_found::<Location2111>(format!(
                "no Location {country_code}/{party_id}/{location_id}"
            )),
        }
    }

    async fn location_2_1_1_put(
        State(cfg): State<Arc<Locations2111Config>>,
        Path((country_code, party_id, location_id)): Path<(String, String, String)>,
        Json(location): Json<Location2111>,
    ) -> Response {
        cfg.put(&country_code, &party_id, &location_id, location);
        Json(OcpiResponse::<Location2111>::success_empty()).into_response()
    }

    async fn location_2_1_1_patch(
        State(cfg): State<Arc<Locations2111Config>>,
        Path((country_code, party_id, location_id)): Path<(String, String, String)>,
        Json(partial): Json<ocpi_types::serde_json::Value>,
    ) -> Response {
        write_result::<Location2111>(
            cfg.patch_location(&country_code, &party_id, &location_id, partial),
            "Location",
        )
    }

    async fn evse_2_1_1_get(
        State(cfg): State<Arc<Locations2111Config>>,
        Path((country_code, party_id, location_id, evse_uid)): Path<(
            String,
            String,
            String,
            String,
        )>,
    ) -> Response {
        match cfg.get_evse(&country_code, &party_id, &location_id, &evse_uid) {
            Some(evse) => Json(OcpiResponse::success(evse)).into_response(),
            None => location_not_found::<Evse2111>(format!("no EVSE {evse_uid} in {location_id}")),
        }
    }

    async fn evse_2_1_1_put(
        State(cfg): State<Arc<Locations2111Config>>,
        Path((country_code, party_id, location_id, _evse_uid)): Path<(
            String,
            String,
            String,
            String,
        )>,
        Json(evse): Json<Evse2111>,
    ) -> Response {
        write_result::<Evse2111>(
            cfg.put_evse(&country_code, &party_id, &location_id, evse),
            "Location",
        )
    }

    async fn evse_2_1_1_patch(
        State(cfg): State<Arc<Locations2111Config>>,
        Path((country_code, party_id, location_id, evse_uid)): Path<(
            String,
            String,
            String,
            String,
        )>,
        Json(partial): Json<ocpi_types::serde_json::Value>,
    ) -> Response {
        write_result::<Evse2111>(
            cfg.patch_evse(&country_code, &party_id, &location_id, &evse_uid, partial),
            "Location or EVSE",
        )
    }

    async fn connector_2_1_1_get(
        State(cfg): State<Arc<Locations2111Config>>,
        Path((country_code, party_id, location_id, evse_uid, connector_id)): Path<(
            String,
            String,
            String,
            String,
            String,
        )>,
    ) -> Response {
        match cfg.get_connector(
            &country_code,
            &party_id,
            &location_id,
            &evse_uid,
            &connector_id,
        ) {
            Some(connector) => Json(OcpiResponse::success(connector)).into_response(),
            None => location_not_found::<Connector2111>(format!(
                "no Connector {connector_id} in {evse_uid}"
            )),
        }
    }

    async fn connector_2_1_1_put(
        State(cfg): State<Arc<Locations2111Config>>,
        Path((country_code, party_id, location_id, evse_uid, _connector_id)): Path<(
            String,
            String,
            String,
            String,
            String,
        )>,
        Json(connector): Json<Connector2111>,
    ) -> Response {
        write_result::<Connector2111>(
            cfg.put_connector(&country_code, &party_id, &location_id, &evse_uid, connector),
            "Location or EVSE",
        )
    }

    async fn connector_2_1_1_patch(
        State(cfg): State<Arc<Locations2111Config>>,
        Path((country_code, party_id, location_id, evse_uid, connector_id)): Path<(
            String,
            String,
            String,
            String,
            String,
        )>,
        Json(partial): Json<ocpi_types::serde_json::Value>,
    ) -> Response {
        write_result::<Connector2111>(
            cfg.patch_connector(
                &country_code,
                &party_id,
                &location_id,
                &evse_uid,
                &connector_id,
                partial,
            ),
            "Location, EVSE, or Connector",
        )
    }

    // ── Locations sender (OCPI 2.1.1) ──────────────────────────────────────────

    /// Build an axum router for the OCPI **2.1.1** Locations module **sender**
    /// interface (CPO side).
    ///
    /// Exposes the CPO's own Location catalogue on the **flat** sender path
    /// (§2.1 CPO Interface) — no `{country_code}/{party_id}` owner segments,
    /// since the CPO owns everything it serves here:
    /// - `GET /locations` — paginated list (`X-Total-Count`/`X-Limit`/`Link`)
    /// - `GET /locations/{location_id}`
    /// - `GET /locations/{location_id}/{evse_uid}`
    /// - `GET /locations/{location_id}/{evse_uid}/{connector_id}`
    ///
    /// This mirrors the 2.2.1 sender `GET /locations` wired into
    /// [`locations_router`] and pairs with the client
    /// `OcpiClient::{get_locations,get_location,get_evse,get_connector}_2_1_1`.
    /// It is a **separate** router from the receiver [`locations_2_1_1_router`]
    /// (whose 3-segment owner path would otherwise collide with this router's
    /// 3-segment connector path) — a CPO mounts this on its sender interface,
    /// an eMSP mounts the receiver on its eMSP interface; they are never the
    /// same server path.
    ///
    /// Spec: OCPI 2.1.1 — *Locations* module, §2.1 CPO (Sender) Interface.
    pub fn locations_2_1_1_sender_router(config: Arc<Locations2111Config>) -> Router {
        Router::new()
            .route("/locations", get(locations_2_1_1_list))
            .route("/locations/{location_id}", get(location_2_1_1_sender_get))
            .route(
                "/locations/{location_id}/{evse_uid}",
                get(evse_2_1_1_sender_get),
            )
            .route(
                "/locations/{location_id}/{evse_uid}/{connector_id}",
                get(connector_2_1_1_sender_get),
            )
            .with_state(config)
    }

    async fn locations_2_1_1_list(
        State(cfg): State<Arc<Locations2111Config>>,
        Query(params): Query<PaginatedParams>,
    ) -> Response {
        use ocpi_types::chrono::TimeZone as _;
        let date_from = params.date_from.unwrap_or_else(|| {
            ocpi_types::Utc
                .with_ymd_and_hms(1970, 1, 1, 0, 0, 0)
                .single()
                .expect("epoch is valid")
        });
        let offset = params.offset.unwrap_or(0);
        let limit = params.limit.unwrap_or(DEFAULT_LIMIT);

        let (items, total) = cfg.list(date_from, params.date_to, offset, limit);
        let page = OcpiPaged::new(items, offset, limit, total);
        let next_offset = page.next_offset();
        let body = page.into_response();

        let mut response = Json(body).into_response();
        let hdrs = response.headers_mut();
        if let Ok(v) = total.to_string().parse() {
            hdrs.insert("x-total-count", v);
        }
        if let Ok(v) = limit.to_string().parse() {
            hdrs.insert("x-limit", v);
        }
        if let Some(next_off) = next_offset {
            let link = format!("</locations?offset={next_off}&limit={limit}>; rel=\"next\"");
            if let Ok(v) = link.parse() {
                hdrs.insert("link", v);
            }
        }

        response
    }

    async fn location_2_1_1_sender_get(
        State(cfg): State<Arc<Locations2111Config>>,
        Path(location_id): Path<String>,
    ) -> Response {
        match cfg.get_by_id(&location_id) {
            Some(location) => Json(OcpiResponse::success(location)).into_response(),
            None => location_not_found::<Location2111>(format!("no Location {location_id}")),
        }
    }

    async fn evse_2_1_1_sender_get(
        State(cfg): State<Arc<Locations2111Config>>,
        Path((location_id, evse_uid)): Path<(String, String)>,
    ) -> Response {
        match cfg.get_evse_by_id(&location_id, &evse_uid) {
            Some(evse) => Json(OcpiResponse::success(evse)).into_response(),
            None => location_not_found::<Evse2111>(format!("no EVSE {evse_uid} in {location_id}")),
        }
    }

    async fn connector_2_1_1_sender_get(
        State(cfg): State<Arc<Locations2111Config>>,
        Path((location_id, evse_uid, connector_id)): Path<(String, String, String)>,
    ) -> Response {
        match cfg.get_connector_by_id(&location_id, &evse_uid, &connector_id) {
            Some(connector) => Json(OcpiResponse::success(connector)).into_response(),
            None => location_not_found::<Connector2111>(format!(
                "no Connector {connector_id} in {evse_uid}"
            )),
        }
    }

    // ── Versions handler tests (#99) ───────────────────────────────────────────
    //
    // Drive the `version_details` axum handler directly (no live socket) to
    // assert that `GET /versions/2.1.1` serves the role-less 2.1.1 catalogue
    // while `GET /versions/2.2.1` stays role-bearing, and that an unknown
    // version maps to OCPI `UnsupportedVersion`.
    // Spec: OCPI 2.1.1 — *Version details endpoint*; OCPI 2.2.1 `version` module.
    #[cfg(test)]
    mod versions_tests {
        use super::*;
        use ocpi_types::{
            v2_1_1::{Endpoint as LegacyEndpoint, VersionDetails as LegacyDetails},
            version::{Endpoint, InterfaceRole, ModuleID, Version},
            Url,
        };

        fn config() -> VersionsConfig {
            let mut cfg = VersionsConfig::new();
            cfg.add_version(
                Version {
                    version: VersionNumber::V2_2_1,
                    url: Url::try_from("https://example.com/ocpi/2.2.1").unwrap(),
                },
                VersionDetails {
                    version: VersionNumber::V2_2_1,
                    endpoints: vec![Endpoint {
                        identifier: ModuleID::Credentials,
                        role: InterfaceRole::Sender,
                        url: Url::try_from("https://example.com/ocpi/2.2.1/credentials").unwrap(),
                    }],
                },
            );
            cfg.add_legacy_version(
                Version {
                    version: VersionNumber::V2_1_1,
                    url: Url::try_from("https://example.com/ocpi/2.1.1").unwrap(),
                },
                LegacyDetails {
                    version: VersionNumber::V2_1_1,
                    endpoints: vec![LegacyEndpoint {
                        identifier: ModuleID::Locations,
                        url: Url::try_from("https://example.com/ocpi/2.1.1/locations").unwrap(),
                    }],
                },
            );
            cfg
        }

        async fn body_string(resp: Response) -> String {
            let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap();
            String::from_utf8(bytes.to_vec()).unwrap()
        }

        #[tokio::test]
        async fn list_versions_advertises_both() {
            let cfg = Arc::new(config());
            let resp = list_versions(State(cfg)).await;
            assert_eq!(resp.status(), StatusCode::OK);
            let body = body_string(resp).await;
            assert!(body.contains("2.1.1"), "list must advertise 2.1.1: {body}");
            assert!(body.contains("2.2.1"), "list must advertise 2.2.1: {body}");
        }

        #[tokio::test]
        async fn version_details_2_1_1_is_role_less() {
            let cfg = Arc::new(config());
            let resp = version_details(State(cfg), Path("2.1.1".to_owned())).await;
            assert_eq!(resp.status(), StatusCode::OK);
            let body = body_string(resp).await;
            // A faithful 2.1.1 details document carries no `role` key.
            assert!(!body.contains("role"), "2.1.1 details leaked role: {body}");
            assert!(body.contains("locations"));
        }

        #[tokio::test]
        async fn version_details_2_2_1_stays_role_bearing() {
            let cfg = Arc::new(config());
            let resp = version_details(State(cfg), Path("2.2.1".to_owned())).await;
            assert_eq!(resp.status(), StatusCode::OK);
            let body = body_string(resp).await;
            assert!(
                body.contains("role"),
                "2.2.1 details must carry role: {body}"
            );
        }

        #[tokio::test]
        async fn version_details_unknown_is_unsupported() {
            // "2.0" parses to a real VersionNumber but is registered in neither
            // map, so it falls through to UnsupportedVersion rather than the
            // InvalidParameters path taken by an unparseable string.
            let cfg = Arc::new(config());
            let resp = version_details(State(cfg), Path("2.0".to_owned())).await;
            let body = body_string(resp).await;
            // OCPI status_code 3002 = UnsupportedVersion — never a silent drop.
            assert!(body.contains("3002"), "expected UnsupportedVersion: {body}");
        }
    }

    // ── Credentials handler tests (#76) ────────────────────────────────────────
    //
    // Drive the axum handlers directly (no live socket) to assert the OCPI 2.2.1
    // registration token semantics: the registry is keyed by the issued Token C
    // (`own_credentials.token`), and the bootstrap Token A is burned on a
    // successful POST. Spec: `specs/ocpi/2.2.1/credentials.asciidoc` §Registration.
    #[cfg(test)]
    mod credentials_tests {
        use super::*;
        use ocpi_types::{
            common::{BusinessDetails, CiString2, CiString3, Role},
            v2_2_1::CredentialsRole,
            Url,
        };

        fn creds(token: &str) -> Credentials {
            Credentials {
                token: token.to_owned(),
                url: Url::try_from("https://example.com/ocpi/versions").unwrap(),
                roles: vec![CredentialsRole {
                    role: Role::Cpo,
                    business_details: BusinessDetails {
                        name: "Test Party".into(),
                        website: None,
                        logo: None,
                    },
                    party_id: CiString3::try_from("EXA").unwrap(),
                    country_code: CiString2::try_from("NL").unwrap(),
                }],
            }
        }

        fn auth(raw_token: &str) -> HeaderMap {
            let mut headers = HeaderMap::new();
            let value = CredentialToken::new(raw_token).to_header_value();
            headers.insert("Authorization", value.parse().expect("valid header value"));
            headers
        }

        // The server hands back Token C; the sender bootstraps with Token A and
        // offers its own Token B in the POST body (used only for the fetch-back).
        fn config() -> Arc<CredentialsConfig> {
            Arc::new(CredentialsConfig::new(creds("TOKEN_C")))
        }

        #[tokio::test]
        async fn post_registers_under_token_c_and_burns_token_a() {
            let cfg = config();
            let resp =
                credentials_post(State(cfg.clone()), auth("TOKEN_A"), Json(creds("TOKEN_B"))).await;
            assert_eq!(resp.status(), StatusCode::OK);
            // Registration is keyed by the issued Token C, not the bearer.
            assert!(cfg.is_registered("TOKEN_C"));
            // The bootstrap Token A is burned; the sender's Token B is the
            // receiver→sender direction and never authenticates the receiver.
            assert!(!cfg.is_registered("TOKEN_A"));
            assert!(!cfg.is_registered("TOKEN_B"));
        }

        #[tokio::test]
        async fn get_authenticates_with_token_c_and_rejects_token_a() {
            let cfg = config();
            credentials_post(State(cfg.clone()), auth("TOKEN_A"), Json(creds("TOKEN_B"))).await;

            // Token C → 200.
            let ok = credentials_get(State(cfg.clone()), auth("TOKEN_C")).await;
            assert_eq!(ok.status(), StatusCode::OK);

            // Burned Token A → 401.
            let unauth = credentials_get(State(cfg.clone()), auth("TOKEN_A")).await;
            assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);
        }

        #[tokio::test]
        async fn get_before_registration_is_401() {
            let cfg = config();
            let resp = credentials_get(State(cfg.clone()), auth("TOKEN_C")).await;
            assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        }

        #[tokio::test]
        async fn re_post_after_registration_is_405() {
            let cfg = config();
            credentials_post(State(cfg.clone()), auth("TOKEN_A"), Json(creds("TOKEN_B"))).await;
            // Even a fresh bootstrap token cannot re-register once Token C is
            // issued — the sender must PUT to rotate.
            let resp = credentials_post(
                State(cfg.clone()),
                auth("TOKEN_A2"),
                Json(creds("TOKEN_B2")),
            )
            .await;
            assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);
            assert!(cfg.is_registered("TOKEN_C"));
        }

        #[tokio::test]
        async fn put_authenticates_with_token_c_and_rejects_burned_token_a() {
            let cfg = config();
            credentials_post(State(cfg.clone()), auth("TOKEN_A"), Json(creds("TOKEN_B"))).await;

            // PUT presenting Token C (the registered key) rotates the sender's
            // credentials and stays registered under Token C.
            let rotated = credentials_put(
                State(cfg.clone()),
                auth("TOKEN_C"),
                Json(creds("TOKEN_B_NEW")),
            )
            .await;
            assert_eq!(rotated.status(), StatusCode::OK);
            assert!(cfg.is_registered("TOKEN_C"));

            // PUT presenting the burned Token A is rejected as not registered.
            let stale = credentials_put(
                State(cfg.clone()),
                auth("TOKEN_A"),
                Json(creds("TOKEN_B_NEW")),
            )
            .await;
            assert_eq!(stale.status(), StatusCode::METHOD_NOT_ALLOWED);
        }

        #[tokio::test]
        async fn delete_with_token_c_unregisters() {
            let cfg = config();
            credentials_post(State(cfg.clone()), auth("TOKEN_A"), Json(creds("TOKEN_B"))).await;
            let resp = credentials_delete(State(cfg.clone()), auth("TOKEN_C")).await;
            assert_eq!(resp.status(), StatusCode::OK);
            assert!(!cfg.is_registered("TOKEN_C"));
        }
    }

    // ── 2.1.1 Credentials handler tests (#112) ─────────────────────────────────
    //
    // Same Token A→B→C semantics as the 2.2.1 tests above, but driven against
    // the flat 2.1.1 object and `credentials_2_1_1_*` handlers. The 2.1.1 store
    // performs no fetch-back, so a `POST` registers directly under Token C.
    #[cfg(test)]
    mod credentials_2_1_1_tests {
        use super::*;
        use crate::{
            Endpoint2111, FetchError, FetchFuture, LegacyVersionDetails, LegacyVersionFetcher,
        };
        use ocpi_types::version::{Version, VersionNumber};
        use ocpi_types::{
            common::{BusinessDetails, CiString2, CiString3},
            serde_json, ModuleID, OcpiResponse, OcpiStatusCode, Url,
        };

        fn creds(token: &str) -> Credentials2111 {
            Credentials2111 {
                token: token.to_owned(),
                url: Url::try_from("https://example.com/ocpi/versions").unwrap(),
                business_details: BusinessDetails {
                    name: "Test Party".into(),
                    website: None,
                    logo: None,
                },
                party_id: CiString3::try_from("EXA").unwrap(),
                country_code: CiString2::try_from("NL").unwrap(),
            }
        }

        fn auth(raw_token: &str) -> HeaderMap {
            let mut headers = HeaderMap::new();
            let value = CredentialToken::new(raw_token).to_header_value();
            headers.insert("Authorization", value.parse().expect("valid header value"));
            headers
        }

        fn config() -> Arc<Credentials2111Config> {
            Arc::new(Credentials2111Config::new(creds("TOKEN_C")))
        }

        #[tokio::test]
        async fn post_registers_under_token_c_and_burns_token_a() {
            let cfg = config();
            let resp =
                credentials_2_1_1_post(State(cfg.clone()), auth("TOKEN_A"), Json(creds("TOKEN_B")))
                    .await;
            assert_eq!(resp.status(), StatusCode::OK);
            assert!(cfg.is_registered("TOKEN_C"));
            assert!(!cfg.is_registered("TOKEN_A"));
            assert!(!cfg.is_registered("TOKEN_B"));
        }

        #[tokio::test]
        async fn get_authenticates_with_token_c_and_rejects_token_a() {
            let cfg = config();
            credentials_2_1_1_post(State(cfg.clone()), auth("TOKEN_A"), Json(creds("TOKEN_B")))
                .await;

            let ok = credentials_2_1_1_get(State(cfg.clone()), auth("TOKEN_C")).await;
            assert_eq!(ok.status(), StatusCode::OK);

            let unauth = credentials_2_1_1_get(State(cfg.clone()), auth("TOKEN_A")).await;
            assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);
        }

        #[tokio::test]
        async fn get_before_registration_is_401() {
            let cfg = config();
            let resp = credentials_2_1_1_get(State(cfg.clone()), auth("TOKEN_C")).await;
            assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        }

        #[tokio::test]
        async fn re_post_after_registration_is_405() {
            let cfg = config();
            credentials_2_1_1_post(State(cfg.clone()), auth("TOKEN_A"), Json(creds("TOKEN_B")))
                .await;
            let resp = credentials_2_1_1_post(
                State(cfg.clone()),
                auth("TOKEN_A2"),
                Json(creds("TOKEN_B2")),
            )
            .await;
            assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);
            assert!(cfg.is_registered("TOKEN_C"));
        }

        #[tokio::test]
        async fn put_authenticates_with_token_c_and_rejects_burned_token_a() {
            let cfg = config();
            credentials_2_1_1_post(State(cfg.clone()), auth("TOKEN_A"), Json(creds("TOKEN_B")))
                .await;

            let rotated = credentials_2_1_1_put(
                State(cfg.clone()),
                auth("TOKEN_C"),
                Json(creds("TOKEN_B_NEW")),
            )
            .await;
            assert_eq!(rotated.status(), StatusCode::OK);
            assert!(cfg.is_registered("TOKEN_C"));

            let stale = credentials_2_1_1_put(
                State(cfg.clone()),
                auth("TOKEN_A"),
                Json(creds("TOKEN_B_NEW")),
            )
            .await;
            assert_eq!(stale.status(), StatusCode::METHOD_NOT_ALLOWED);
        }

        #[tokio::test]
        async fn delete_with_token_c_unregisters() {
            let cfg = config();
            credentials_2_1_1_post(State(cfg.clone()), auth("TOKEN_A"), Json(creds("TOKEN_B")))
                .await;
            let resp = credentials_2_1_1_delete(State(cfg.clone()), auth("TOKEN_C")).await;
            assert_eq!(resp.status(), StatusCode::OK);
            assert!(!cfg.is_registered("TOKEN_C"));
        }

        #[tokio::test]
        async fn put_before_registration_is_405() {
            let cfg = config();
            let resp =
                credentials_2_1_1_put(State(cfg.clone()), auth("TOKEN_C"), Json(creds("TOKEN_B")))
                    .await;
            assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);
        }

        // ── Fetch-back (#115) ──────────────────────────────────────────────

        /// A canned role-less [`LegacyVersionFetcher`] returning a fixed 2.1.1
        /// catalogue (no HTTP). The live transport path is covered by the e2e
        /// test in `crates/ocpi-client/tests/m7_credentials_2_1_1_fetch_back.rs`.
        struct StubLegacyFetcher {
            details: LegacyVersionDetails,
            fail: bool,
        }

        impl LegacyVersionFetcher for StubLegacyFetcher {
            fn fetch_versions<'a>(
                &'a self,
                _url: &'a str,
                _token: &'a str,
            ) -> FetchFuture<'a, Vec<Version>> {
                let version = self.details.version;
                let fail = self.fail;
                Box::pin(async move {
                    if fail {
                        return Err(FetchError::Transport("unreachable".into()));
                    }
                    Ok(vec![Version {
                        version,
                        url: Url::try_from("https://party.example/versions/2.1.1").unwrap(),
                    }])
                })
            }

            fn fetch_version_details<'a>(
                &'a self,
                _url: &'a str,
                _token: &'a str,
            ) -> FetchFuture<'a, LegacyVersionDetails> {
                let details = self.details.clone();
                Box::pin(async move { Ok(details) })
            }
        }

        fn stub_details() -> LegacyVersionDetails {
            LegacyVersionDetails {
                version: VersionNumber::V2_1_1,
                endpoints: vec![Endpoint2111 {
                    identifier: ModuleID::Locations,
                    url: Url::try_from("https://party.example/2.1.1/locations").unwrap(),
                }],
            }
        }

        fn config_with_fetcher(fail: bool) -> Arc<Credentials2111Config> {
            Arc::new(Credentials2111Config::new_with_fetcher(
                creds("TOKEN_C"),
                vec![VersionNumber::V2_1_1],
                Arc::new(StubLegacyFetcher {
                    details: stub_details(),
                    fail,
                }),
            ))
        }

        #[tokio::test]
        async fn post_with_fetcher_stores_role_less_endpoints() {
            let cfg = config_with_fetcher(false);
            let resp =
                credentials_2_1_1_post(State(cfg.clone()), auth("TOKEN_A"), Json(creds("TOKEN_B")))
                    .await;
            assert_eq!(resp.status(), StatusCode::OK);
            let stored = cfg
                .get_endpoints("TOKEN_C")
                .expect("fetch-back stores endpoints under Token C");
            assert_eq!(stored.len(), 1);
            assert_eq!(stored[0].identifier, ModuleID::Locations);
        }

        #[tokio::test]
        async fn post_with_failed_fetch_back_is_3001_and_unregistered() {
            let cfg = config_with_fetcher(true);
            let resp =
                credentials_2_1_1_post(State(cfg.clone()), auth("TOKEN_A"), Json(creds("TOKEN_B")))
                    .await;
            // 3001 is carried in the OCPI envelope with an HTTP 200 body.
            assert_eq!(resp.status(), StatusCode::OK);
            let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap();
            let env: OcpiResponse<Credentials2111> = serde_json::from_slice(&body).unwrap();
            assert_eq!(env.status_code, OcpiStatusCode::UnableToUseClientApi);
            assert!(
                !cfg.is_registered("TOKEN_C"),
                "a failed fetch-back must not register the party"
            );
        }

        #[tokio::test]
        async fn post_without_fetcher_registers_with_no_endpoints() {
            let cfg = config(); // built with `new` — no fetcher
            credentials_2_1_1_post(State(cfg.clone()), auth("TOKEN_A"), Json(creds("TOKEN_B")))
                .await;
            assert!(cfg.is_registered("TOKEN_C"));
            assert!(
                cfg.get_endpoints("TOKEN_C").is_none(),
                "no fetcher → no stored catalogue (the #112 path)"
            );
        }

        #[test]
        fn new_with_fetcher_debug_surfaces_fetch_back_flag() {
            let cfg = config_with_fetcher(false);
            assert!(format!("{cfg:?}").contains("fetch_back: true"));
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ocpi_types::chrono::TimeZone as _;
    use ocpi_types::{
        common::{CiString2, CiString3, CiString36, GeoLocation, Role},
        v2_2_1::{
            AllowedType, AuthMethod, Cdr, CdrDimension, CdrDimensionType, CdrLocation, CdrToken,
            ChargingPeriod, ClientInfo, ConnectionStatus, Connector, ConnectorFormat,
            ConnectorType, Evse, Location, PowerType, PriceComponent, Session, SessionStatus,
            Status, Tariff, TariffDimensionType, TariffElement, Token, TokenType, WhitelistType,
        },
        OcpiStatusCode,
    };

    fn make_session(id: &str, ts: DateTime<Utc>) -> Session {
        use ocpi_types::common::{CiString2, CiString3, CiString36};
        Session {
            country_code: CiString2::try_from("NL").unwrap(),
            party_id: CiString3::try_from("CPO").unwrap(),
            id: CiString36::try_from(id).unwrap(),
            start_date_time: ts,
            end_date_time: None,
            kwh: 0.0,
            cdr_token: CdrToken {
                country_code: CiString2::try_from("NL").unwrap(),
                party_id: CiString3::try_from("MSP").unwrap(),
                uid: CiString36::try_from("RFID001").unwrap(),
                token_type: TokenType::Rfid,
                contract_id: CiString36::try_from("NL-MSP-0001").unwrap(),
            },
            auth_method: AuthMethod::Whitelist,
            authorization_reference: None,
            location_id: CiString36::try_from("LOC1").unwrap(),
            evse_uid: CiString36::try_from("EVSE1").unwrap(),
            connector_id: CiString36::try_from("1").unwrap(),
            meter_id: None,
            currency: "EUR".to_string(),
            charging_periods: vec![],
            total_cost: None,
            status: SessionStatus::Active,
            last_updated: ts,
        }
    }

    #[test]
    fn sessions_config_put_and_get_roundtrip() {
        let cfg = SessionsConfig::new();
        let ts = Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap();
        let s = make_session("S001", ts);
        cfg.put("NL", "CPO", "S001", s.clone());
        let got = cfg.get("NL", "CPO", "S001").unwrap();
        assert_eq!(got.id.as_str(), "S001");
        assert_eq!(got.kwh, 0.0);
    }

    #[test]
    fn sessions_config_get_missing_returns_none() {
        let cfg = SessionsConfig::new();
        assert!(cfg.get("NL", "CPO", "MISSING").is_none());
    }

    #[test]
    fn sessions_config_list_filters_by_date_from() {
        let cfg = SessionsConfig::new();
        let t1 = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap();
        cfg.put("NL", "CPO", "S001", make_session("S001", t1));
        cfg.put("NL", "CPO", "S002", make_session("S002", t2));

        let cutoff = Utc.with_ymd_and_hms(2024, 3, 1, 0, 0, 0).unwrap();
        let (items, total) = cfg.list(cutoff, None, 0, 50);
        assert_eq!(total, 1);
        assert_eq!(items[0].id.as_str(), "S002");
    }

    #[test]
    fn sessions_config_list_filters_by_date_to() {
        let cfg = SessionsConfig::new();
        let t1 = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap();
        cfg.put("NL", "CPO", "S001", make_session("S001", t1));
        cfg.put("NL", "CPO", "S002", make_session("S002", t2));

        let from = Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap();
        let to = Utc.with_ymd_and_hms(2024, 3, 1, 0, 0, 0).unwrap();
        let (items, total) = cfg.list(from, Some(to), 0, 50);
        assert_eq!(total, 1);
        assert_eq!(items[0].id.as_str(), "S001");
    }

    fn prefs(profile_type: ProfileType, departure: bool, energy: bool) -> ChargingPreferences {
        ChargingPreferences {
            profile_type,
            departure_time: departure.then(|| Utc.with_ymd_and_hms(2024, 6, 1, 18, 0, 0).unwrap()),
            energy_need: energy.then_some(30.0),
            discharge_allowed: None,
        }
    }

    #[test]
    fn charging_preferences_unknown_session_is_not_found() {
        let cfg = SessionsConfig::new();
        let err = cfg
            .set_charging_preferences("S404", &prefs(ProfileType::Regular, false, false))
            .unwrap_err();
        assert!(matches!(err, ServerError::NotFound));
    }

    #[test]
    fn charging_preferences_regular_is_accepted_without_planning_input() {
        let cfg = SessionsConfig::new();
        let ts = Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap();
        cfg.put("NL", "CPO", "S001", make_session("S001", ts));
        let resp = cfg
            .set_charging_preferences("S001", &prefs(ProfileType::Regular, false, false))
            .unwrap();
        assert_eq!(resp, ChargingPreferencesResponse::Accepted);
    }

    #[test]
    fn charging_preferences_smart_profile_requires_departure_then_energy() {
        let cfg = SessionsConfig::new();
        let ts = Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap();
        cfg.put("NL", "CPO", "S001", make_session("S001", ts));

        // No departure_time → CPO needs it to plan.
        let resp = cfg
            .set_charging_preferences("S001", &prefs(ProfileType::Cheap, false, false))
            .unwrap();
        assert_eq!(resp, ChargingPreferencesResponse::DepartureRequired);

        // Departure but no energy_need → CHEAP/GREEN need an energy target.
        let resp = cfg
            .set_charging_preferences("S001", &prefs(ProfileType::Green, true, false))
            .unwrap();
        assert_eq!(resp, ChargingPreferencesResponse::EnergyNeedRequired);

        // Both present → accepted.
        let resp = cfg
            .set_charging_preferences("S001", &prefs(ProfileType::Cheap, true, true))
            .unwrap();
        assert_eq!(resp, ChargingPreferencesResponse::Accepted);

        // FAST needs only a departure window, not an energy target.
        let resp = cfg
            .set_charging_preferences("S001", &prefs(ProfileType::Fast, true, false))
            .unwrap();
        assert_eq!(resp, ChargingPreferencesResponse::Accepted);
    }

    #[test]
    fn sessions_config_list_pagination() {
        let cfg = SessionsConfig::new();
        let epoch = Utc.with_ymd_and_hms(1970, 1, 1, 0, 0, 0).unwrap();
        for i in 0u32..5 {
            let ts = epoch + ocpi_types::chrono::Duration::seconds(i64::from(i));
            cfg.put(
                "NL",
                "CPO",
                &format!("S{i:03}"),
                make_session(&format!("S{i:03}"), ts),
            );
        }

        let (page, total) = cfg.list(epoch, None, 2, 2);
        assert_eq!(total, 5);
        assert_eq!(page.len(), 2);
        // sorted by last_updated, so offset=2 picks the 3rd & 4th
        assert_eq!(page[0].id.as_str(), "S002");
        assert_eq!(page[1].id.as_str(), "S003");
    }

    #[test]
    fn sessions_config_patch_updates_kwh() {
        let cfg = SessionsConfig::new();
        let ts = Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap();
        cfg.put("NL", "CPO", "S001", make_session("S001", ts));
        let patch = ocpi_types::serde_json::json!({"kwh": 12.5});
        cfg.patch_json("NL", "CPO", "S001", patch).unwrap();
        let updated = cfg.get("NL", "CPO", "S001").unwrap();
        assert_eq!(updated.kwh, 12.5);
    }

    #[test]
    fn sessions_config_patch_missing_returns_not_found() {
        let cfg = SessionsConfig::new();
        let patch = ocpi_types::serde_json::json!({"kwh": 5.0});
        let err = cfg.patch_json("NL", "CPO", "MISSING", patch).unwrap_err();
        assert!(matches!(err, ServerError::NotFound));
        assert_eq!(err.status_code(), OcpiStatusCode::UnknownLocation);
    }

    #[test]
    fn not_found_maps_to_unknown_location() {
        assert_eq!(
            ServerError::NotFound.status_code(),
            OcpiStatusCode::UnknownLocation
        );
    }

    #[test]
    fn unauthorized_maps_to_client_error() {
        assert_eq!(
            ServerError::Unauthorized.status_code(),
            OcpiStatusCode::ClientError
        );
    }

    #[test]
    fn not_implemented_maps_to_server_error() {
        assert_eq!(
            ServerError::NotImplemented("credentials").status_code(),
            OcpiStatusCode::ServerError
        );
    }

    #[test]
    fn already_registered_maps_to_client_error() {
        assert_eq!(
            ServerError::AlreadyRegistered.status_code(),
            OcpiStatusCode::ClientError
        );
    }

    #[test]
    fn not_registered_maps_to_client_error() {
        assert_eq!(
            ServerError::NotRegistered.status_code(),
            OcpiStatusCode::ClientError
        );
    }

    #[test]
    fn versions_config_default_is_empty() {
        let cfg = VersionsConfig::default();
        assert!(cfg.versions.is_empty());
        assert!(cfg.details.is_empty());
    }

    #[test]
    fn versions_config_add_and_lookup() {
        use ocpi_types::{
            version::{Endpoint, InterfaceRole, ModuleID, Version, VersionDetails, VersionNumber},
            Url,
        };

        let mut cfg = VersionsConfig::new();
        let details = VersionDetails {
            version: VersionNumber::V2_2_1,
            endpoints: vec![Endpoint {
                identifier: ModuleID::Credentials,
                role: InterfaceRole::Sender,
                url: Url::try_from("https://example.com/ocpi/2.2.1/credentials").unwrap(),
            }],
        };
        cfg.add_version(
            Version {
                version: VersionNumber::V2_2_1,
                url: Url::try_from("https://example.com/ocpi/2.2.1").unwrap(),
            },
            details.clone(),
        );
        assert_eq!(cfg.versions.len(), 1);
        assert_eq!(cfg.versions[0].version, VersionNumber::V2_2_1);
        assert_eq!(cfg.details.get(&VersionNumber::V2_2_1).unwrap(), &details);
    }

    #[test]
    fn versions_config_advertises_2_1_1_and_2_2_1() {
        use ocpi_types::{
            v2_1_1::{Endpoint as LegacyEndpoint, VersionDetails as LegacyDetails},
            version::{Endpoint, InterfaceRole, ModuleID, Version, VersionDetails, VersionNumber},
            Url,
        };

        let mut cfg = VersionsConfig::new();
        cfg.add_version(
            Version {
                version: VersionNumber::V2_2_1,
                url: Url::try_from("https://example.com/ocpi/2.2.1").unwrap(),
            },
            VersionDetails {
                version: VersionNumber::V2_2_1,
                endpoints: vec![Endpoint {
                    identifier: ModuleID::Credentials,
                    role: InterfaceRole::Sender,
                    url: Url::try_from("https://example.com/ocpi/2.2.1/credentials").unwrap(),
                }],
            },
        );
        cfg.add_legacy_version(
            Version {
                version: VersionNumber::V2_1_1,
                url: Url::try_from("https://example.com/ocpi/2.1.1").unwrap(),
            },
            LegacyDetails {
                version: VersionNumber::V2_1_1,
                endpoints: vec![LegacyEndpoint {
                    identifier: ModuleID::Credentials,
                    url: Url::try_from("https://example.com/ocpi/2.1.1/credentials").unwrap(),
                }],
            },
        );

        // Both versions are advertised in GET /versions.
        let advertised: Vec<VersionNumber> = cfg.versions.iter().map(|v| v.version).collect();
        assert!(advertised.contains(&VersionNumber::V2_1_1));
        assert!(advertised.contains(&VersionNumber::V2_2_1));

        // The 2.1.1 catalogue lives in the role-less map and serializes without
        // a `role` field; the 2.2.1 catalogue stays role-bearing.
        let legacy = cfg.legacy_details.get(&VersionNumber::V2_1_1).unwrap();
        let legacy_json = ocpi_types::serde_json::to_string(legacy).unwrap();
        assert!(
            !legacy_json.contains("role"),
            "2.1.1 details: {legacy_json}"
        );

        let role_bearing = cfg.details.get(&VersionNumber::V2_2_1).unwrap();
        let role_json = ocpi_types::serde_json::to_string(role_bearing).unwrap();
        assert!(role_json.contains("role"), "2.2.1 details: {role_json}");
    }

    #[test]
    fn versions_config_missing_version_is_unsupported() {
        // The ServerError returned for a missing version maps to UnsupportedVersion (3002).
        let cfg = VersionsConfig::new();
        assert!(!cfg.details.contains_key(&VersionNumber::V2_2_1));
        // Verify the error code that would be returned
        let err = ServerError::Ocpi(ocpi_types::OcpiError::Status(
            OcpiStatusCode::UnsupportedVersion,
        ));
        assert_eq!(err.status_code(), OcpiStatusCode::UnsupportedVersion);
    }

    // ── CdrsConfig tests ──────────────────────────────────────────────────────

    fn make_cdr(id: &str, ts: DateTime<Utc>) -> Cdr {
        use ocpi_types::common::{CiString2, CiString3, CiString36, CiString39, CiString48};
        Cdr {
            country_code: CiString2::try_from("NL").unwrap(),
            party_id: CiString3::try_from("CPO").unwrap(),
            id: CiString39::try_from(id).unwrap(),
            start_date_time: ts,
            end_date_time: ts,
            session_id: None,
            cdr_token: CdrToken {
                country_code: CiString2::try_from("NL").unwrap(),
                party_id: CiString3::try_from("MSP").unwrap(),
                uid: CiString36::try_from("RFID001").unwrap(),
                token_type: TokenType::Rfid,
                contract_id: CiString36::try_from("NL-MSP-0001").unwrap(),
            },
            auth_method: AuthMethod::Whitelist,
            authorization_reference: None,
            cdr_location: CdrLocation {
                id: CiString36::try_from("LOC1").unwrap(),
                name: None,
                address: "Test St 1".into(),
                city: "Amsterdam".into(),
                postal_code: None,
                state: None,
                country: "NLD".into(),
                coordinates: ocpi_types::common::GeoLocation {
                    latitude: "52.370216".into(),
                    longitude: "4.895168".into(),
                },
                evse_uid: CiString36::try_from("EVSE1").unwrap(),
                evse_id: CiString48::try_from("NL*CPO*E001").unwrap(),
                connector_id: CiString36::try_from("1").unwrap(),
                connector_standard: ConnectorType::Iec62196T2,
                connector_format: ConnectorFormat::Socket,
                connector_power_type: PowerType::Ac3Phase,
            },
            meter_id: None,
            currency: "EUR".into(),
            tariffs: vec![],
            charging_periods: vec![ChargingPeriod {
                start_date_time: ts,
                dimensions: vec![CdrDimension {
                    dimension_type: CdrDimensionType::Energy,
                    volume: 10.0,
                }],
                tariff_id: None,
            }],
            signed_data: None,
            total_cost: ocpi_types::common::Price {
                excl_vat: 2.50,
                incl_vat: None,
            },
            total_fixed_cost: None,
            total_energy: 10.0,
            total_energy_cost: None,
            total_time: 0.5,
            total_time_cost: None,
            total_parking_time: None,
            total_parking_cost: None,
            total_reservation_cost: None,
            remark: None,
            invoice_reference_id: None,
            credit: None,
            credit_reference_id: None,
            home_charging_compensation: None,
            last_updated: ts,
        }
    }

    #[test]
    fn cdrs_config_store_and_get_roundtrip() {
        let cfg = CdrsConfig::new("https://example.com/ocpi/2.2.1");
        let ts = Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap();
        let cdr = make_cdr("CDR001", ts);
        let url = cfg.store(cdr.clone());
        assert_eq!(url, "https://example.com/ocpi/2.2.1/cdrs/CDR001");
        let got = cfg.get("CDR001").unwrap();
        assert_eq!(got.id.as_str(), "CDR001");
    }

    #[test]
    fn cdrs_config_get_missing_returns_none() {
        let cfg = CdrsConfig::new("https://example.com/ocpi/2.2.1");
        assert!(cfg.get("MISSING").is_none());
    }

    #[test]
    fn cdrs_config_list_filters_by_date_from() {
        let cfg = CdrsConfig::new("https://example.com/ocpi/2.2.1");
        let t1 = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap();
        cfg.store(make_cdr("CDR001", t1));
        cfg.store(make_cdr("CDR002", t2));

        let cutoff = Utc.with_ymd_and_hms(2024, 3, 1, 0, 0, 0).unwrap();
        let (items, total) = cfg.list(cutoff, None, 0, 50);
        assert_eq!(total, 1);
        assert_eq!(items[0].id.as_str(), "CDR002");
    }

    #[test]
    fn cdrs_config_list_filters_by_date_to() {
        let cfg = CdrsConfig::new("https://example.com/ocpi/2.2.1");
        let t1 = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap();
        cfg.store(make_cdr("CDR001", t1));
        cfg.store(make_cdr("CDR002", t2));

        let from = Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap();
        let to = Utc.with_ymd_and_hms(2024, 3, 1, 0, 0, 0).unwrap();
        let (items, total) = cfg.list(from, Some(to), 0, 50);
        assert_eq!(total, 1);
        assert_eq!(items[0].id.as_str(), "CDR001");
    }

    #[test]
    fn cdrs_config_list_pagination() {
        let cfg = CdrsConfig::new("https://example.com/ocpi/2.2.1");
        let epoch = Utc.with_ymd_and_hms(1970, 1, 1, 0, 0, 0).unwrap();
        for i in 0u32..5 {
            let ts = epoch + ocpi_types::chrono::Duration::seconds(i64::from(i));
            cfg.store(make_cdr(&format!("CDR{i:03}"), ts));
        }

        let (page, total) = cfg.list(epoch, None, 2, 2);
        assert_eq!(total, 5);
        assert_eq!(page.len(), 2);
        assert_eq!(page[0].id.as_str(), "CDR002");
        assert_eq!(page[1].id.as_str(), "CDR003");
    }

    #[test]
    fn cdrs_config_url_trailing_slash_normalised() {
        let cfg = CdrsConfig::new("https://example.com/ocpi/2.2.1/");
        let ts = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let url = cfg.store(make_cdr("CDR001", ts));
        assert_eq!(url, "https://example.com/ocpi/2.2.1/cdrs/CDR001");
    }

    // ── Cdrs22Config tests (OCPI 2.2) ─────────────────────────────────────────

    // Build a spec-shaped OCPI 2.2 CDR: its `CdrToken` has no `country_code`/
    // `party_id`, its `CdrLocation` carries a required `postal_code` and no
    // `state`, and the `Cdr` has no `home_charging_compensation`.
    fn make_cdr_2_2(id: &str, ts: DateTime<Utc>) -> Cdr22 {
        use ocpi_types::common::{CiString2, CiString3, CiString36, CiString39, CiString48};
        use ocpi_types::v2_2::{
            AuthMethod as AuthMethod22, CdrLocation as CdrLocation22, CdrToken as CdrToken22,
            ChargingPeriod as ChargingPeriod22, ConnectorFormat as ConnectorFormat22,
            ConnectorType as ConnectorType22, PowerType as PowerType22, TokenType as TokenType22,
        };
        use ocpi_types::v2_2_1::{CdrDimension, CdrDimensionType};
        Cdr22 {
            country_code: CiString2::try_from("NL").unwrap(),
            party_id: CiString3::try_from("CPO").unwrap(),
            id: CiString39::try_from(id).unwrap(),
            start_date_time: ts,
            end_date_time: ts,
            session_id: None,
            cdr_token: CdrToken22 {
                uid: CiString36::try_from("RFID001").unwrap(),
                token_type: TokenType22::Rfid,
                contract_id: CiString36::try_from("NL-MSP-0001").unwrap(),
            },
            auth_method: AuthMethod22::Whitelist,
            authorization_reference: None,
            cdr_location: CdrLocation22 {
                id: CiString36::try_from("LOC1").unwrap(),
                name: None,
                address: "Test St 1".into(),
                city: "Amsterdam".into(),
                postal_code: "1000 AA".into(),
                country: "NLD".into(),
                coordinates: ocpi_types::common::GeoLocation {
                    latitude: "52.370216".into(),
                    longitude: "4.895168".into(),
                },
                evse_uid: CiString36::try_from("EVSE1").unwrap(),
                evse_id: CiString48::try_from("NL*CPO*E001").unwrap(),
                connector_id: CiString36::try_from("1").unwrap(),
                connector_standard: ConnectorType22::Iec62196T2,
                connector_format: ConnectorFormat22::Socket,
                connector_power_type: PowerType22::Ac3Phase,
            },
            meter_id: None,
            currency: "EUR".into(),
            tariffs: vec![],
            charging_periods: vec![ChargingPeriod22 {
                start_date_time: ts,
                dimensions: vec![CdrDimension {
                    dimension_type: CdrDimensionType::Energy,
                    volume: 10.0,
                }],
                tariff_id: None,
            }],
            signed_data: None,
            total_cost: ocpi_types::common::Price {
                excl_vat: 2.50,
                incl_vat: None,
            },
            total_fixed_cost: None,
            total_energy: 10.0,
            total_energy_cost: None,
            total_time: 0.5,
            total_time_cost: None,
            total_parking_time: None,
            total_parking_cost: None,
            total_reservation_cost: None,
            remark: None,
            invoice_reference_id: None,
            credit: None,
            credit_reference_id: None,
            last_updated: ts,
        }
    }

    #[test]
    fn cdrs_22_config_store_and_get_roundtrip() {
        let cfg = Cdrs22Config::new("https://example.com/ocpi/2.2");
        let ts = Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap();
        let cdr = make_cdr_2_2("CDR001", ts);
        let url = cfg.store(cdr.clone());
        assert_eq!(url, "https://example.com/ocpi/2.2/cdrs/CDR001");
        let got = cfg.get("CDR001").unwrap();
        assert_eq!(got.id.as_str(), "CDR001");
        // The 2.2 delta shape survives the store round-trip.
        assert_eq!(got.cdr_location.postal_code, "1000 AA");
    }

    #[test]
    fn cdrs_22_config_get_missing_returns_none() {
        let cfg = Cdrs22Config::new("https://example.com/ocpi/2.2");
        assert!(cfg.get("MISSING").is_none());
    }

    #[test]
    fn cdrs_22_config_list_filters_and_paginates() {
        let cfg = Cdrs22Config::new("https://example.com/ocpi/2.2");
        let epoch = Utc.with_ymd_and_hms(1970, 1, 1, 0, 0, 0).unwrap();
        for i in 0u32..5 {
            let ts = epoch + ocpi_types::chrono::Duration::seconds(i64::from(i));
            cfg.store(make_cdr_2_2(&format!("CDR{i:03}"), ts));
        }

        // date_from filter drops the earliest two.
        let cutoff = epoch + ocpi_types::chrono::Duration::seconds(2);
        let (items, total) = cfg.list(cutoff, None, 0, 50);
        assert_eq!(total, 3);
        assert_eq!(items[0].id.as_str(), "CDR002");

        // Pagination: offset 2, limit 2 over the full set.
        let (page, total_all) = cfg.list(epoch, None, 2, 2);
        assert_eq!(total_all, 5);
        assert_eq!(page.len(), 2);
        assert_eq!(page[0].id.as_str(), "CDR002");
        assert_eq!(page[1].id.as_str(), "CDR003");
    }

    #[tokio::test]
    async fn cdrs_22_handler_post_then_get() {
        let cfg = Cdrs22Config::new("https://example.com/ocpi/2.2");
        let ts = Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap();
        let url = Cdrs22Handler::post_cdr(&cfg, make_cdr_2_2("CDR777", ts))
            .await
            .unwrap();
        assert_eq!(url, "https://example.com/ocpi/2.2/cdrs/CDR777");
        let got = Cdrs22Handler::get_cdr(&cfg, "CDR777").await.unwrap();
        assert_eq!(got.id.as_str(), "CDR777");
        // A missing CDR is a NotFound, not a panic.
        assert!(matches!(
            Cdrs22Handler::get_cdr(&cfg, "NOPE").await,
            Err(ServerError::NotFound)
        ));
    }

    #[test]
    fn cdr_2_2_delta_shape_round_trips_unmangled() {
        // A 2.2 partner's CDR JSON: the token has no country_code/party_id, the
        // location's postal_code is present, and there is no
        // home_charging_compensation. It must deserialize into Cdr22 and
        // re-serialize without ever gaining a 2.2.1-only field.
        use ocpi_types::serde_json;
        let ts = Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap();
        let cdr = make_cdr_2_2("CDR001", ts);
        let json = serde_json::to_string(&cdr).unwrap();
        assert!(
            !json.contains("home_charging_compensation"),
            "2.2 Cdr must not emit the 2.2.1-only home_charging_compensation: {json}"
        );
        // The token object must not carry the 2.2.1-added owner fields.
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let token = &value["cdr_token"];
        assert!(token.get("country_code").is_none());
        assert!(token.get("party_id").is_none());
        // Faithful round-trip back into the 2.2 struct.
        let back: Cdr22 = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id.as_str(), "CDR001");
        assert_eq!(back.cdr_location.postal_code, "1000 AA");
    }

    // ── TariffsConfig tests ───────────────────────────────────────────────────

    fn make_tariff(id: &str, ts: DateTime<Utc>) -> Tariff {
        use ocpi_types::common::{CiString2, CiString3, CiString36};
        Tariff {
            country_code: CiString2::try_from("NL").unwrap(),
            party_id: CiString3::try_from("CPO").unwrap(),
            id: CiString36::try_from(id).unwrap(),
            currency: "EUR".to_string(),
            tariff_type: None,
            tariff_alt_text: vec![],
            tariff_alt_url: None,
            min_price: None,
            max_price: None,
            elements: vec![TariffElement {
                price_components: vec![PriceComponent {
                    component_type: TariffDimensionType::Energy,
                    price: 0.25,
                    vat: None,
                    step_size: 1,
                }],
                restrictions: None,
            }],
            start_date_time: None,
            end_date_time: None,
            energy_mix: None,
            last_updated: ts,
        }
    }

    #[test]
    fn tariffs_config_put_and_get_roundtrip() {
        let cfg = TariffsConfig::new();
        let ts = Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap();
        let t = make_tariff("TARIFF001", ts);
        cfg.put("NL", "CPO", "TARIFF001", t);
        let got = cfg.get("NL", "CPO", "TARIFF001").unwrap();
        assert_eq!(got.id.as_str(), "TARIFF001");
        assert_eq!(got.currency, "EUR");
    }

    #[test]
    fn tariffs_config_get_missing_returns_none() {
        let cfg = TariffsConfig::new();
        assert!(cfg.get("NL", "CPO", "MISSING").is_none());
    }

    #[test]
    fn tariffs_config_delete_removes_tariff() {
        let cfg = TariffsConfig::new();
        let ts = Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap();
        cfg.put("NL", "CPO", "T001", make_tariff("T001", ts));
        cfg.delete("NL", "CPO", "T001").unwrap();
        assert!(cfg.get("NL", "CPO", "T001").is_none());
    }

    #[test]
    fn tariffs_config_delete_unknown_returns_not_found() {
        let cfg = TariffsConfig::new();
        let err = cfg.delete("NL", "CPO", "MISSING").unwrap_err();
        assert!(matches!(err, ServerError::NotFound));
    }

    #[test]
    fn tariffs_config_list_filters_by_date_from() {
        let cfg = TariffsConfig::new();
        let t1 = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap();
        cfg.put("NL", "CPO", "T001", make_tariff("T001", t1));
        cfg.put("NL", "CPO", "T002", make_tariff("T002", t2));

        let cutoff = Utc.with_ymd_and_hms(2024, 3, 1, 0, 0, 0).unwrap();
        let (items, total) = cfg.list(cutoff, None, 0, 50);
        assert_eq!(total, 1);
        assert_eq!(items[0].id.as_str(), "T002");
    }

    #[test]
    fn tariffs_config_list_filters_by_date_to() {
        let cfg = TariffsConfig::new();
        let t1 = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap();
        cfg.put("NL", "CPO", "T001", make_tariff("T001", t1));
        cfg.put("NL", "CPO", "T002", make_tariff("T002", t2));

        let from = Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap();
        let to = Utc.with_ymd_and_hms(2024, 3, 1, 0, 0, 0).unwrap();
        let (items, total) = cfg.list(from, Some(to), 0, 50);
        assert_eq!(total, 1);
        assert_eq!(items[0].id.as_str(), "T001");
    }

    #[test]
    fn tariffs_config_list_pagination() {
        let cfg = TariffsConfig::new();
        let epoch = Utc.with_ymd_and_hms(1970, 1, 1, 0, 0, 0).unwrap();
        for i in 0u32..5 {
            let ts = epoch + ocpi_types::chrono::Duration::seconds(i64::from(i));
            cfg.put(
                "NL",
                "CPO",
                &format!("T{i:03}"),
                make_tariff(&format!("T{i:03}"), ts),
            );
        }

        let (page, total) = cfg.list(epoch, None, 2, 2);
        assert_eq!(total, 5);
        assert_eq!(page.len(), 2);
        assert_eq!(page[0].id.as_str(), "T002");
        assert_eq!(page[1].id.as_str(), "T003");
    }

    // ── TokensConfig tests ────────────────────────────────────────────────────

    fn make_token(uid: &str, ts: DateTime<Utc>, valid: bool) -> Token {
        use ocpi_types::common::{CiString2, CiString3, CiString36};
        Token {
            country_code: CiString2::try_from("NL").unwrap(),
            party_id: CiString3::try_from("MSP").unwrap(),
            uid: CiString36::try_from(uid).unwrap(),
            token_type: TokenType::Rfid,
            contract_id: CiString36::try_from("NL-MSP-0001").unwrap(),
            visual_number: None,
            issuer: "TestIssuer".to_string(),
            group_id: None,
            valid,
            whitelist: WhitelistType::Always,
            language: None,
            default_profile_type: None,
            energy_contract: None,
            last_updated: ts,
        }
    }

    #[test]
    fn tokens_config_put_and_get_roundtrip() {
        let cfg = TokensConfig::new();
        let ts = Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap();
        let token = make_token("TOKEN001", ts, true);
        cfg.put("NL", "MSP", "TOKEN001", TokenType::Rfid, token);
        let got = cfg.get("NL", "MSP", "TOKEN001", TokenType::Rfid).unwrap();
        assert_eq!(got.uid.as_str(), "TOKEN001");
        assert!(got.valid);
    }

    #[test]
    fn tokens_config_get_missing_returns_none() {
        let cfg = TokensConfig::new();
        assert!(cfg.get("NL", "MSP", "MISSING", TokenType::Rfid).is_none());
    }

    #[test]
    fn tokens_config_get_wrong_type_returns_none() {
        let cfg = TokensConfig::new();
        let ts = Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap();
        cfg.put(
            "NL",
            "MSP",
            "TOKEN001",
            TokenType::Rfid,
            make_token("TOKEN001", ts, true),
        );
        assert!(cfg
            .get("NL", "MSP", "TOKEN001", TokenType::AppUser)
            .is_none());
    }

    #[test]
    fn tokens_config_patch_updates_valid_field() {
        let cfg = TokensConfig::new();
        let ts = Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap();
        cfg.put(
            "NL",
            "MSP",
            "TOKEN001",
            TokenType::Rfid,
            make_token("TOKEN001", ts, true),
        );
        let patch =
            ocpi_types::serde_json::json!({"valid": false, "last_updated": "2024-06-02T00:00:00Z"});
        cfg.patch_json("NL", "MSP", "TOKEN001", TokenType::Rfid, patch)
            .unwrap();
        let updated = cfg.get("NL", "MSP", "TOKEN001", TokenType::Rfid).unwrap();
        assert!(!updated.valid);
    }

    #[test]
    fn tokens_config_patch_missing_returns_not_found() {
        let cfg = TokensConfig::new();
        let patch = ocpi_types::serde_json::json!({"valid": false});
        let err = cfg
            .patch_json("NL", "MSP", "MISSING", TokenType::Rfid, patch)
            .unwrap_err();
        assert!(matches!(err, ServerError::NotFound));
    }

    #[test]
    fn tokens_config_list_filters_by_date_from() {
        let cfg = TokensConfig::new();
        let t1 = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap();
        cfg.put(
            "NL",
            "MSP",
            "T001",
            TokenType::Rfid,
            make_token("T001", t1, true),
        );
        cfg.put(
            "NL",
            "MSP",
            "T002",
            TokenType::Rfid,
            make_token("T002", t2, true),
        );

        let cutoff = Utc.with_ymd_and_hms(2024, 3, 1, 0, 0, 0).unwrap();
        let (items, total) = cfg.list(cutoff, None, 0, 50);
        assert_eq!(total, 1);
        assert_eq!(items[0].uid.as_str(), "T002");
    }

    #[test]
    fn tokens_config_list_pagination() {
        let cfg = TokensConfig::new();
        let epoch = Utc.with_ymd_and_hms(1970, 1, 1, 0, 0, 0).unwrap();
        for i in 0u32..5 {
            let ts = epoch + ocpi_types::chrono::Duration::seconds(i64::from(i));
            let uid = format!("T{i:03}");
            cfg.put(
                "NL",
                "MSP",
                &uid,
                TokenType::Rfid,
                make_token(&uid, ts, true),
            );
        }

        let (page, total) = cfg.list(epoch, None, 2, 2);
        assert_eq!(total, 5);
        assert_eq!(page.len(), 2);
        assert_eq!(page[0].uid.as_str(), "T002");
        assert_eq!(page[1].uid.as_str(), "T003");
    }

    #[test]
    fn tokens_config_authorize_valid_token_returns_allowed() {
        let cfg = TokensConfig::new();
        let ts = Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap();
        cfg.put(
            "NL",
            "MSP",
            "RFID001",
            TokenType::Rfid,
            make_token("RFID001", ts, true),
        );
        let result = cfg.authorize("RFID001", TokenType::Rfid, None).unwrap();
        assert_eq!(result.allowed, AllowedType::Allowed);
        assert_eq!(result.token.uid.as_str(), "RFID001");
        assert!(result.location.is_none());
    }

    #[test]
    fn tokens_config_authorize_invalid_token_returns_blocked() {
        let cfg = TokensConfig::new();
        let ts = Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap();
        cfg.put(
            "NL",
            "MSP",
            "RFID002",
            TokenType::Rfid,
            make_token("RFID002", ts, false),
        );
        let result = cfg.authorize("RFID002", TokenType::Rfid, None).unwrap();
        assert_eq!(result.allowed, AllowedType::Blocked);
    }

    #[test]
    fn tokens_config_authorize_unknown_token_returns_unknown_token_error() {
        let cfg = TokensConfig::new();
        let err = cfg.authorize("UNKNOWN", TokenType::Rfid, None).unwrap_err();
        assert!(matches!(err, ServerError::UnknownToken));
        assert_eq!(err.status_code(), OcpiStatusCode::UnknownToken);
    }

    // ── Tokens2111Config tests (OCPI 2.1.1) ───────────────────────────────────

    fn make_token_2_1_1(uid: &str, ts: DateTime<Utc>, valid: bool) -> ocpi_types::v2_1_1::Token {
        use ocpi_types::common::{CiString36, CiString64};
        ocpi_types::v2_1_1::Token {
            uid: CiString36::try_from(uid).unwrap(),
            token_type: ocpi_types::v2_1_1::TokenType::Rfid,
            auth_id: CiString36::try_from("DE8ACC12E46L89").unwrap(),
            visual_number: None,
            issuer: CiString64::try_from("TestIssuer").unwrap(),
            valid,
            whitelist: WhitelistType::Always,
            language: None,
            last_updated: ts,
        }
    }

    #[test]
    fn tokens_2_1_1_config_put_and_get_roundtrip() {
        use ocpi_types::v2_1_1::TokenType;
        let cfg = Tokens2111Config::new();
        let ts = Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap();
        cfg.put(
            "NL",
            "TNM",
            "T001",
            TokenType::Rfid,
            make_token_2_1_1("T001", ts, true),
        );
        let got = cfg.get("NL", "TNM", "T001", TokenType::Rfid).unwrap();
        assert_eq!(got.uid.as_str(), "T001");
        assert_eq!(got.auth_id.as_str(), "DE8ACC12E46L89");
        assert!(got.valid);
        // A different type at the same uid is a distinct key.
        assert!(cfg.get("NL", "TNM", "T001", TokenType::Other).is_none());
        assert!(cfg.get("NL", "TNM", "MISSING", TokenType::Rfid).is_none());
    }

    #[test]
    fn tokens_2_1_1_config_patch_updates_valid_field() {
        use ocpi_types::v2_1_1::TokenType;
        let cfg = Tokens2111Config::new();
        let ts = Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap();
        cfg.put(
            "NL",
            "TNM",
            "T001",
            TokenType::Rfid,
            make_token_2_1_1("T001", ts, true),
        );
        let patch =
            ocpi_types::serde_json::json!({"valid": false, "last_updated": "2024-06-02T00:00:00Z"});
        cfg.patch_json("NL", "TNM", "T001", TokenType::Rfid, patch)
            .unwrap();
        assert!(!cfg.get("NL", "TNM", "T001", TokenType::Rfid).unwrap().valid);

        let err = cfg
            .patch_json(
                "NL",
                "TNM",
                "MISSING",
                TokenType::Rfid,
                ocpi_types::serde_json::json!({"valid": false}),
            )
            .unwrap_err();
        assert!(matches!(err, ServerError::NotFound));
    }

    #[test]
    fn tokens_2_1_1_config_list_pagination() {
        use ocpi_types::v2_1_1::TokenType;
        let cfg = Tokens2111Config::new();
        let epoch = Utc.with_ymd_and_hms(1970, 1, 1, 0, 0, 0).unwrap();
        for i in 0u32..5 {
            let ts = epoch + ocpi_types::chrono::Duration::seconds(i64::from(i));
            let uid = format!("T{i:03}");
            cfg.put(
                "NL",
                "TNM",
                &uid,
                TokenType::Rfid,
                make_token_2_1_1(&uid, ts, true),
            );
        }
        let (page, total) = cfg.list(epoch, None, 2, 2);
        assert_eq!(total, 5);
        assert_eq!(page.len(), 2);
        assert_eq!(page[0].uid.as_str(), "T002");
        assert_eq!(page[1].uid.as_str(), "T003");
    }

    #[test]
    fn tokens_2_1_1_config_authorize_valid_echoes_location_with_connector_ids() {
        use ocpi_types::v2_1_1::{LocationReferences, TokenType};
        let cfg = Tokens2111Config::new();
        let ts = Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap();
        cfg.put(
            "NL",
            "TNM",
            "RFID001",
            TokenType::Rfid,
            make_token_2_1_1("RFID001", ts, true),
        );
        let loc = LocationReferences {
            location_id: "LOC1".try_into().unwrap(),
            evse_uids: vec!["3256".try_into().unwrap()],
            connector_ids: vec!["1".try_into().unwrap()],
        };
        let result = cfg
            .authorize("RFID001", TokenType::Rfid, Some(loc))
            .unwrap();
        assert_eq!(result.allowed, AllowedType::Allowed);
        // 2.1.1 AuthorizationInfo echoes the location (with its 2.1.1-only
        // connector_ids) and carries no `token` field (type-level guarantee).
        let echoed = result.location.expect("allowed authorize echoes location");
        assert_eq!(echoed.location_id.as_str(), "LOC1");
        assert_eq!(echoed.connector_ids.len(), 1);
    }

    #[test]
    fn tokens_2_1_1_config_authorize_invalid_blocked_and_unknown_errors() {
        use ocpi_types::v2_1_1::TokenType;
        let cfg = Tokens2111Config::new();
        let ts = Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap();
        cfg.put(
            "NL",
            "TNM",
            "RFID002",
            TokenType::Rfid,
            make_token_2_1_1("RFID002", ts, false),
        );
        let blocked = cfg.authorize("RFID002", TokenType::Rfid, None).unwrap();
        assert_eq!(blocked.allowed, AllowedType::Blocked);
        assert!(blocked.location.is_none());

        let err = cfg.authorize("UNKNOWN", TokenType::Rfid, None).unwrap_err();
        assert!(matches!(err, ServerError::UnknownToken));
        assert_eq!(err.status_code(), OcpiStatusCode::UnknownToken);
    }

    // ── Locations2111Config tests (OCPI 2.1.1) ────────────────────────────────

    /// A spec-derived 2.1.1 Location fixture (ported from §8.3.1.1, trimmed to a
    /// single EVSE + Connector). Note the required `type`, no
    /// `country_code`/`party_id` on the object, and a singular `tariff_id`.
    fn sample_location_2_1_1() -> ocpi_types::v2_1_1::Location {
        let json = r#"{
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
                "last_updated": "2015-06-28T08:12:01Z"
            }],
            "last_updated": "2015-06-29T20:39:09Z"
        }"#;
        ocpi_types::serde_json::from_str(json).unwrap()
    }

    #[test]
    fn locations_2_1_1_config_put_get_and_nested_lookup() {
        let cfg = Locations2111Config::new();
        cfg.put("BE", "BEC", "LOC1", sample_location_2_1_1());

        let got = cfg.get("BE", "BEC", "LOC1").unwrap();
        assert_eq!(got.id.as_str(), "LOC1");
        assert_eq!(got.evses.len(), 1);

        // Nested EVSE + Connector are addressable by uid / id.
        let evse = cfg.get_evse("BE", "BEC", "LOC1", "3256").unwrap();
        assert_eq!(evse.uid.as_str(), "3256");
        let connector = cfg.get_connector("BE", "BEC", "LOC1", "3256", "1").unwrap();
        assert_eq!(connector.id.as_str(), "1");
        assert_eq!(connector.tariff_id.as_ref().unwrap().as_str(), "11");

        // Wrong owner segments or unknown ids miss.
        assert!(cfg.get("NL", "TNM", "LOC1").is_none());
        assert!(cfg.get_evse("BE", "BEC", "LOC1", "9999").is_none());
        assert!(cfg
            .get_connector("BE", "BEC", "LOC1", "3256", "9")
            .is_none());

        // Pushing a second EVSE upserts into the existing Location.
        let evse: ocpi_types::v2_1_1::Evse =
            ocpi_types::serde_json::from_value(ocpi_types::serde_json::json!({
                "uid": "3257", "status": "AVAILABLE", "connectors": [],
                "last_updated": "2016-01-01T00:00:00Z"
            }))
            .unwrap();
        cfg.put_evse("BE", "BEC", "LOC1", evse).unwrap();
        assert_eq!(cfg.get("BE", "BEC", "LOC1").unwrap().evses.len(), 2);
    }

    #[test]
    fn locations_2_1_1_config_patch_location_and_nested() {
        let cfg = Locations2111Config::new();
        cfg.put("BE", "BEC", "LOC1", sample_location_2_1_1());

        // Merge-patch a top-level Location field.
        cfg.patch_location(
            "BE",
            "BEC",
            "LOC1",
            ocpi_types::serde_json::json!({ "city": "Brugge" }),
        )
        .unwrap();
        assert_eq!(cfg.get("BE", "BEC", "LOC1").unwrap().city, "Brugge");

        // Merge-patch a nested EVSE status.
        cfg.patch_evse(
            "BE",
            "BEC",
            "LOC1",
            "3256",
            ocpi_types::serde_json::json!({ "status": "CHARGING" }),
        )
        .unwrap();
        assert_eq!(
            cfg.get_evse("BE", "BEC", "LOC1", "3256").unwrap().status,
            ocpi_types::v2_1_1::Status::Charging
        );

        // Merge-patch a nested Connector field.
        cfg.patch_connector(
            "BE",
            "BEC",
            "LOC1",
            "3256",
            "1",
            ocpi_types::serde_json::json!({ "amperage": 32 }),
        )
        .unwrap();
        assert_eq!(
            cfg.get_connector("BE", "BEC", "LOC1", "3256", "1")
                .unwrap()
                .amperage,
            32
        );
    }

    #[test]
    fn locations_2_1_1_config_missing_targets_are_not_found() {
        let cfg = Locations2111Config::new();
        // Patch/put against an absent Location or EVSE surfaces NotFound.
        let err = cfg
            .patch_location(
                "BE",
                "BEC",
                "NOPE",
                ocpi_types::serde_json::json!({ "city": "X" }),
            )
            .unwrap_err();
        assert!(matches!(err, ServerError::NotFound));

        cfg.put("BE", "BEC", "LOC1", sample_location_2_1_1());
        let evse_json = ocpi_types::serde_json::json!({
            "uid": "7", "status": "AVAILABLE", "connectors": [], "last_updated": "2015-06-28T08:12:01Z"
        });
        let evse: ocpi_types::v2_1_1::Evse = ocpi_types::serde_json::from_value(evse_json).unwrap();
        // Putting an EVSE into a missing Location is NotFound.
        let err = cfg.put_evse("BE", "BEC", "MISSING", evse).unwrap_err();
        assert!(matches!(err, ServerError::NotFound));
    }

    /// Build a minimal 2.1.1 Location with the given `id` / `last_updated`
    /// for the sender-list tests (`GET /locations`, #142).
    fn location_2_1_1_at(id: &str, last_updated: &str) -> ocpi_types::v2_1_1::Location {
        ocpi_types::serde_json::from_value(ocpi_types::serde_json::json!({
            "id": id,
            "type": "ON_STREET",
            "address": "F.Rooseveltlaan 3A",
            "city": "Gent",
            "postal_code": "9000",
            "country": "BEL",
            "coordinates": { "latitude": "51.047590", "longitude": "3.729940" },
            "evses": [],
            "last_updated": last_updated,
        }))
        .unwrap()
    }

    #[test]
    fn locations_2_1_1_config_list_filters_and_paginates() {
        let cfg = Locations2111Config::new();
        cfg.put(
            "BE",
            "BEC",
            "L1",
            location_2_1_1_at("L1", "2020-01-01T00:00:00Z"),
        );
        cfg.put(
            "BE",
            "BEC",
            "L2",
            location_2_1_1_at("L2", "2021-01-01T00:00:00Z"),
        );
        cfg.put(
            "BE",
            "BEC",
            "L3",
            location_2_1_1_at("L3", "2022-01-01T00:00:00Z"),
        );

        // No filter: all three, sorted by last_updated.
        let epoch = "1970-01-01T00:00:00Z".parse().unwrap();
        let (all, total) = cfg.list(epoch, None, 0, 50);
        assert_eq!(total, 3);
        assert_eq!(
            all.iter().map(|l| l.id.as_str()).collect::<Vec<_>>(),
            ["L1", "L2", "L3"]
        );

        // date_from excludes the oldest; page size caps the slice but total is
        // the full filtered count.
        let from = "2021-01-01T00:00:00Z".parse().unwrap();
        let (page, total) = cfg.list(from, None, 0, 1);
        assert_eq!(total, 2);
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].id.as_str(), "L2");

        // offset walks to the next page.
        let (page, _) = cfg.list(from, None, 1, 1);
        assert_eq!(page[0].id.as_str(), "L3");
    }

    #[test]
    fn locations_2_1_1_config_flat_sender_getters() {
        let cfg = Locations2111Config::new();
        cfg.put("BE", "BEC", "LOC1", sample_location_2_1_1());

        // Flat lookup by bare id (no owner segments) resolves all three levels.
        assert_eq!(cfg.get_by_id("LOC1").unwrap().id.as_str(), "LOC1");
        assert_eq!(
            cfg.get_evse_by_id("LOC1", "3256").unwrap().uid.as_str(),
            "3256"
        );
        assert_eq!(
            cfg.get_connector_by_id("LOC1", "3256", "1")
                .unwrap()
                .id
                .as_str(),
            "1"
        );

        // Unknown ids at each level miss.
        assert!(cfg.get_by_id("NOPE").is_none());
        assert!(cfg.get_evse_by_id("LOC1", "9999").is_none());
        assert!(cfg.get_connector_by_id("LOC1", "3256", "9").is_none());
    }

    // ── CommandsConfig tests ──────────────────────────────────────────────────

    #[test]
    fn commands_config_not_supported_response_has_correct_fields() {
        let resp = CommandsConfig::not_supported_response();
        assert_eq!(resp.result, CommandResponseType::NotSupported);
        assert_eq!(resp.timeout, 30);
        assert!(resp.message.is_empty());
    }

    #[test]
    fn commands_config_new_constructs_without_panic() {
        let _cfg = CommandsConfig::new();
    }

    // ── ChargingProfilesConfig tests ──────────────────────────────────────────

    #[test]
    fn charging_profiles_config_not_supported_response_has_correct_fields() {
        let resp = ChargingProfilesConfig::not_supported_response();
        assert_eq!(
            resp.result,
            ocpi_types::v2_2_1::ChargingProfileResponseType::NotSupported
        );
        assert_eq!(resp.timeout, 0);
    }

    #[test]
    fn charging_profiles_config_new_constructs_without_panic() {
        let _cfg = ChargingProfilesConfig::new();
    }

    /// Build a minimal `ActiveChargingProfile` for the Sender-update tests.
    fn make_active_charging_profile() -> ocpi_types::v2_2_1::ActiveChargingProfile {
        use ocpi_types::v2_2_1::{
            ActiveChargingProfile, ChargingProfile, ChargingProfilePeriod, ChargingRateUnit,
        };
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

    #[tokio::test]
    async fn charging_profiles_config_receive_active_profile_update_no_ops() {
        let cfg = ChargingProfilesConfig::new();
        let result = cfg
            .receive_active_profile_update("SESSION-1", make_active_charging_profile())
            .await;
        assert!(
            result.is_ok(),
            "the placeholder Sender PUT handler must accept (no-op) the update"
        );
    }

    #[test]
    fn charging_profiles_sender_router_constructs_without_panic() {
        let _router = http::charging_profiles_sender_router(std::sync::Arc::new(
            ChargingProfilesConfig::new(),
        ));
    }

    // ── HubClientInfoConfig tests ─────────────────────────────────────────────

    fn make_client_info(cc: &str, party: &str, role: Role, ts: DateTime<Utc>) -> ClientInfo {
        ClientInfo {
            country_code: CiString2::try_from(cc).unwrap(),
            party_id: CiString3::try_from(party).unwrap(),
            role,
            status: ConnectionStatus::Connected,
            last_updated: ts,
        }
    }

    #[test]
    fn hub_client_info_config_put_and_get_roundtrip() {
        let cfg = HubClientInfoConfig::new();
        let ts = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let info = make_client_info("NL", "CPO", Role::Cpo, ts);
        cfg.put("NL", "CPO", info.clone());
        let got = cfg.get("NL", "CPO").unwrap();
        assert_eq!(got.country_code.as_str(), "NL");
        assert_eq!(got.party_id.as_str(), "CPO");
        assert_eq!(got.role, Role::Cpo);
        assert_eq!(got.status, ConnectionStatus::Connected);
    }

    #[test]
    fn hub_client_info_config_get_missing_returns_none() {
        let cfg = HubClientInfoConfig::new();
        assert!(cfg.get("DE", "MSP").is_none());
    }

    #[test]
    fn hub_client_info_config_put_overwrites_existing() {
        let cfg = HubClientInfoConfig::new();
        let t1 = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap();
        cfg.put("NL", "CPO", make_client_info("NL", "CPO", Role::Cpo, t1));
        cfg.put(
            "NL",
            "CPO",
            ClientInfo {
                status: ConnectionStatus::Offline,
                last_updated: t2,
                ..make_client_info("NL", "CPO", Role::Cpo, t2)
            },
        );
        let got = cfg.get("NL", "CPO").unwrap();
        assert_eq!(got.status, ConnectionStatus::Offline);
        assert_eq!(got.last_updated, t2);
    }

    #[test]
    fn hub_client_info_config_list_pagination() {
        let cfg = HubClientInfoConfig::new();
        let epoch = Utc.with_ymd_and_hms(1970, 1, 1, 0, 0, 0).unwrap();
        let t1 = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap();
        let t3 = Utc.with_ymd_and_hms(2024, 3, 1, 0, 0, 0).unwrap();
        cfg.put("NL", "CPO", make_client_info("NL", "CPO", Role::Cpo, t1));
        cfg.put("DE", "MSP", make_client_info("DE", "MSP", Role::Emsp, t2));
        cfg.put("FR", "HUB", make_client_info("FR", "HUB", Role::Hub, t3));

        let (page, total) = cfg.list(epoch, None, 0, 2);
        assert_eq!(total, 3);
        assert_eq!(page.len(), 2);

        let (page2, total2) = cfg.list(epoch, None, 2, 2);
        assert_eq!(total2, 3);
        assert_eq!(page2.len(), 1);
    }

    #[test]
    fn hub_client_info_config_list_filter_by_date_from() {
        let cfg = HubClientInfoConfig::new();
        let t1 = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap();
        cfg.put("NL", "CPO", make_client_info("NL", "CPO", Role::Cpo, t1));
        cfg.put("DE", "MSP", make_client_info("DE", "MSP", Role::Emsp, t2));

        let cutoff = Utc.with_ymd_and_hms(2024, 3, 1, 0, 0, 0).unwrap();
        let (items, total) = cfg.list(cutoff, None, 0, 50);
        assert_eq!(total, 1);
        assert_eq!(items[0].party_id.as_str(), "MSP");
    }

    // ── CredentialsConfig ─────────────────────────────────────────────────────

    fn make_credentials(token: &str) -> Credentials {
        use ocpi_types::{common::BusinessDetails, v2_2_1::CredentialsRole, Url};
        Credentials {
            token: token.to_owned(),
            url: Url::try_from("https://example.com/ocpi/versions").unwrap(),
            roles: vec![CredentialsRole {
                role: Role::Cpo,
                business_details: BusinessDetails {
                    name: "Test CPO".into(),
                    website: None,
                    logo: None,
                },
                party_id: CiString3::try_from("EXA").unwrap(),
                country_code: CiString2::try_from("NL").unwrap(),
            }],
        }
    }

    #[test]
    fn credentials_config_new_is_empty() {
        let cfg = CredentialsConfig::new(make_credentials("SERVER_TOKEN"));
        assert!(!cfg.is_registered("TOKEN_A"));
    }

    #[test]
    fn credentials_config_register_and_lookup() {
        let cfg = CredentialsConfig::new(make_credentials("SERVER_TOKEN"));
        cfg.register("TOKEN_A", make_credentials("PARTY_TOKEN"))
            .unwrap();
        assert!(cfg.is_registered("TOKEN_A"));
        assert!(!cfg.is_registered("TOKEN_B"));
    }

    #[test]
    fn credentials_config_double_register_is_error() {
        let cfg = CredentialsConfig::new(make_credentials("SERVER_TOKEN"));
        cfg.register("TOKEN_A", make_credentials("P")).unwrap();
        let err = cfg.register("TOKEN_A", make_credentials("P2")).unwrap_err();
        assert!(matches!(err, ServerError::AlreadyRegistered));
    }

    #[test]
    fn credentials_config_update_unknown_is_error() {
        let cfg = CredentialsConfig::new(make_credentials("SERVER_TOKEN"));
        let err = cfg.update("UNKNOWN", make_credentials("P")).unwrap_err();
        assert!(matches!(err, ServerError::NotRegistered));
    }

    #[test]
    fn credentials_config_update_known_succeeds() {
        let cfg = CredentialsConfig::new(make_credentials("SERVER_TOKEN"));
        cfg.register("TOKEN_A", make_credentials("P1")).unwrap();
        cfg.update("TOKEN_A", make_credentials("P2")).unwrap();
        assert!(cfg.is_registered("TOKEN_A"));
    }

    #[test]
    fn credentials_config_delete_unknown_is_error() {
        let cfg = CredentialsConfig::new(make_credentials("SERVER_TOKEN"));
        let err = cfg.delete("UNKNOWN").unwrap_err();
        assert!(matches!(err, ServerError::NotRegistered));
    }

    #[test]
    fn credentials_config_delete_known_removes() {
        let cfg = CredentialsConfig::new(make_credentials("SERVER_TOKEN"));
        cfg.register("TOKEN_A", make_credentials("P")).unwrap();
        cfg.delete("TOKEN_A").unwrap();
        assert!(!cfg.is_registered("TOKEN_A"));
    }

    #[test]
    fn credentials_config_own_credentials_preserved() {
        let cfg = CredentialsConfig::new(make_credentials("SERVER_TOKEN"));
        assert_eq!(cfg.own_credentials.token, "SERVER_TOKEN");
        assert_eq!(cfg.own_credentials.roles.len(), 1);
    }

    // ── Registration fetch-back (#33) ──────────────────────────────────────

    use ocpi_types::version::{
        Endpoint, InterfaceRole, ModuleID, Version, VersionDetails, VersionNumber,
    };

    fn ep_url(s: &str) -> ocpi_types::Url {
        ocpi_types::Url::try_from(s).unwrap()
    }

    fn make_endpoints() -> Vec<Endpoint> {
        vec![
            Endpoint {
                identifier: ModuleID::Credentials,
                role: InterfaceRole::Sender,
                url: ep_url("https://party.example/ocpi/2.2.1/credentials"),
            },
            Endpoint {
                identifier: ModuleID::Locations,
                role: InterfaceRole::Sender,
                url: ep_url("https://party.example/ocpi/2.2.1/locations"),
            },
        ]
    }

    /// A canned [`VersionFetcher`] used to prove the boxed-future trait is
    /// implementable. The async path itself is exercised by the M2 e2e smoke
    /// test (#23), which brings in the async test harness.
    struct TestFetcher {
        details: VersionDetails,
    }

    impl VersionFetcher for TestFetcher {
        fn fetch_versions<'a>(
            &'a self,
            _url: &'a str,
            _token: &'a str,
        ) -> FetchFuture<'a, Vec<Version>> {
            let version = self.details.version;
            let url = ep_url("https://party.example/ocpi/2.2.1");
            Box::pin(async move { Ok(vec![Version { version, url }]) })
        }

        fn fetch_version_details<'a>(
            &'a self,
            _url: &'a str,
            _token: &'a str,
        ) -> FetchFuture<'a, VersionDetails> {
            let details = self.details.clone();
            Box::pin(async move { Ok(details) })
        }
    }

    #[test]
    fn select_best_version_picks_highest_mutual() {
        let remote = vec![
            Version {
                version: VersionNumber::V2_1_1,
                url: ep_url("https://r/2.1.1"),
            },
            Version {
                version: VersionNumber::V2_2_1,
                url: ep_url("https://r/2.2.1"),
            },
        ];
        let supported = [VersionNumber::V2_1_1, VersionNumber::V2_2_1];
        let chosen = select_best_version(&remote, &supported).unwrap();
        assert_eq!(chosen.version, VersionNumber::V2_2_1);
    }

    #[test]
    fn select_best_version_no_overlap_is_none() {
        let remote = vec![Version {
            version: VersionNumber::V2_0,
            url: ep_url("https://r/2.0"),
        }];
        let supported = [VersionNumber::V2_2_1];
        assert!(select_best_version(&remote, &supported).is_none());
    }

    #[test]
    fn register_with_endpoints_stores_catalogue() {
        let cfg = CredentialsConfig::new(make_credentials("SERVER_TOKEN"));
        cfg.register_with_endpoints("TOKEN_A", make_credentials("P"), Some(make_endpoints()))
            .unwrap();
        let stored = cfg.get_endpoints("TOKEN_A").expect("endpoints stored");
        assert_eq!(stored.len(), 2);
        assert_eq!(stored[0].identifier, ModuleID::Credentials);
    }

    #[test]
    fn register_without_fetchback_has_no_endpoints() {
        let cfg = CredentialsConfig::new(make_credentials("SERVER_TOKEN"));
        cfg.register("TOKEN_A", make_credentials("P")).unwrap();
        assert!(cfg.get_endpoints("TOKEN_A").is_none());
    }

    #[test]
    fn get_endpoints_unknown_token_is_none() {
        let cfg = CredentialsConfig::new(make_credentials("SERVER_TOKEN"));
        assert!(cfg.get_endpoints("NOPE").is_none());
    }

    #[test]
    fn update_with_endpoints_replaces_catalogue() {
        let cfg = CredentialsConfig::new(make_credentials("SERVER_TOKEN"));
        cfg.register("TOKEN_A", make_credentials("P")).unwrap();
        assert!(cfg.get_endpoints("TOKEN_A").is_none());
        cfg.update_with_endpoints("TOKEN_A", make_credentials("P2"), Some(make_endpoints()))
            .unwrap();
        assert_eq!(cfg.get_endpoints("TOKEN_A").unwrap().len(), 2);
    }

    #[test]
    fn new_with_fetcher_constructs_and_is_empty() {
        let fetcher = std::sync::Arc::new(TestFetcher {
            details: VersionDetails {
                version: VersionNumber::V2_2_1,
                endpoints: make_endpoints(),
            },
        });
        let cfg = CredentialsConfig::new_with_fetcher(
            make_credentials("SERVER_TOKEN"),
            vec![VersionNumber::V2_2_1],
            fetcher,
        );
        assert!(!cfg.is_registered("TOKEN_A"));
        // Debug surfaces the fetch-back flag without leaking internals.
        assert!(format!("{cfg:?}").contains("fetch_back: true"));
    }

    #[test]
    fn fetch_back_future_is_send() {
        // Compile-time guarantee: the fetch-back future is `Send`, so it can be
        // awaited inside an axum handler (whose future must be `Send`).
        fn assert_send<T: Send>(_: &T) {}
        let fetcher = std::sync::Arc::new(TestFetcher {
            details: VersionDetails {
                version: VersionNumber::V2_2_1,
                endpoints: make_endpoints(),
            },
        });
        let cfg = CredentialsConfig::new_with_fetcher(
            make_credentials("SERVER_TOKEN"),
            vec![VersionNumber::V2_2_1],
            fetcher,
        );
        let creds = make_credentials("P");
        let fut = cfg.fetch_back(&creds);
        assert_send(&fut);
    }

    // ── Locations ───────────────────────────────────────────────────────────

    fn make_connector(id: &str, ts: DateTime<Utc>) -> Connector {
        Connector {
            id: CiString36::try_from(id).unwrap(),
            standard: ConnectorType::Iec62196T2,
            format: ConnectorFormat::Socket,
            power_type: PowerType::Ac3Phase,
            max_voltage: 400,
            max_amperage: 16,
            max_electric_power: None,
            tariff_ids: Vec::new(),
            terms_and_conditions: None,
            last_updated: ts,
        }
    }

    fn make_evse(uid: &str, status: Status, ts: DateTime<Utc>) -> Evse {
        Evse {
            uid: CiString36::try_from(uid).unwrap(),
            evse_id: None,
            status,
            status_schedule: Vec::new(),
            capabilities: Vec::new(),
            connectors: vec![make_connector("1", ts)],
            floor_level: None,
            coordinates: None,
            physical_reference: None,
            directions: Vec::new(),
            parking_restrictions: Vec::new(),
            images: Vec::new(),
            last_updated: ts,
        }
    }

    fn make_location(id: &str, ts: DateTime<Utc>) -> Location {
        Location {
            country_code: CiString2::try_from("NL").unwrap(),
            party_id: CiString3::try_from("CPO").unwrap(),
            id: CiString36::try_from(id).unwrap(),
            publish: true,
            publish_allowed_to: Vec::new(),
            name: None,
            address: "F.Rooseveltlaan 3A".into(),
            city: "Gent".into(),
            postal_code: Some("9000".into()),
            state: None,
            country: "BEL".into(),
            coordinates: GeoLocation {
                latitude: "51.047599".into(),
                longitude: "3.729944".into(),
            },
            related_locations: Vec::new(),
            parking_type: None,
            evses: vec![make_evse("EVSE1", Status::Available, ts)],
            directions: Vec::new(),
            operator: None,
            suboperator: None,
            owner: None,
            facilities: Vec::new(),
            time_zone: "Europe/Amsterdam".into(),
            opening_times: None,
            charging_when_closed: None,
            images: Vec::new(),
            energy_mix: None,
            last_updated: ts,
        }
    }

    #[test]
    fn locations_config_put_and_get_roundtrip() {
        let cfg = LocationsConfig::new();
        let ts = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        cfg.put(make_location("LOC1", ts));
        let got = cfg.get("NL", "CPO", "LOC1").unwrap();
        assert_eq!(got.id.as_str(), "LOC1");
        assert_eq!(got.evses.len(), 1);
    }

    #[test]
    fn locations_config_get_missing_returns_none() {
        let cfg = LocationsConfig::new();
        assert!(cfg.get("NL", "CPO", "MISSING").is_none());
    }

    #[test]
    fn locations_config_list_filters_by_date_from() {
        let cfg = LocationsConfig::new();
        let old = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
        let new = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        cfg.put(make_location("LOC1", old));
        cfg.put(make_location("LOC2", new));
        let cutoff = Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap();
        let (items, total) = cfg.list(cutoff, None, 0, 50);
        assert_eq!(total, 1);
        assert_eq!(items[0].id.as_str(), "LOC2");
    }

    #[test]
    fn locations_config_list_pagination() {
        let cfg = LocationsConfig::new();
        let base = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        for i in 0..5 {
            let ts = base + ocpi_types::chrono::Duration::seconds(i);
            cfg.put(make_location(&format!("LOC{i}"), ts));
        }
        let epoch = Utc.with_ymd_and_hms(1970, 1, 1, 0, 0, 0).unwrap();
        let (page, total) = cfg.list(epoch, None, 2, 2);
        assert_eq!(total, 5);
        assert_eq!(page.len(), 2);
        assert_eq!(page[0].id.as_str(), "LOC2");
        assert_eq!(page[1].id.as_str(), "LOC3");
    }

    #[test]
    fn locations_config_patch_updates_field() {
        let cfg = LocationsConfig::new();
        let ts = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        cfg.put(make_location("LOC1", ts));
        let patch = ocpi_types::serde_json::json!({ "city": "Amsterdam" });
        cfg.patch_location("NL", "CPO", "LOC1", patch).unwrap();
        let updated = cfg.get("NL", "CPO", "LOC1").unwrap();
        assert_eq!(updated.city, "Amsterdam");
        // Sibling data (the nested EVSE) survives the merge-patch.
        assert_eq!(updated.evses.len(), 1);
    }

    #[test]
    fn locations_config_patch_missing_returns_not_found() {
        let cfg = LocationsConfig::new();
        let patch = ocpi_types::serde_json::json!({ "city": "Amsterdam" });
        let err = cfg
            .patch_location("NL", "CPO", "MISSING", patch)
            .unwrap_err();
        assert!(matches!(err, ServerError::NotFound));
    }

    #[test]
    fn locations_config_get_evse_and_connector() {
        let cfg = LocationsConfig::new();
        let ts = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        cfg.put(make_location("LOC1", ts));
        let evse = cfg.get_evse("NL", "CPO", "LOC1", "EVSE1").unwrap();
        assert_eq!(evse.uid.as_str(), "EVSE1");
        let connector = cfg
            .get_connector("NL", "CPO", "LOC1", "EVSE1", "1")
            .unwrap();
        assert_eq!(connector.id.as_str(), "1");
        assert!(cfg.get_evse("NL", "CPO", "LOC1", "NOPE").is_none());
        assert!(cfg
            .get_connector("NL", "CPO", "LOC1", "EVSE1", "NOPE")
            .is_none());
    }

    #[test]
    fn locations_config_put_evse_upserts_within_location() {
        let cfg = LocationsConfig::new();
        let ts = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        cfg.put(make_location("LOC1", ts));
        // New EVSE is appended.
        cfg.put_evse(
            "NL",
            "CPO",
            "LOC1",
            make_evse("EVSE2", Status::Charging, ts),
        )
        .unwrap();
        let loc = cfg.get("NL", "CPO", "LOC1").unwrap();
        assert_eq!(loc.evses.len(), 2);
        // Re-PUT the same uid replaces it rather than duplicating.
        cfg.put_evse(
            "NL",
            "CPO",
            "LOC1",
            make_evse("EVSE1", Status::Inoperative, ts),
        )
        .unwrap();
        let loc = cfg.get("NL", "CPO", "LOC1").unwrap();
        assert_eq!(loc.evses.len(), 2);
        let evse1 = cfg.get_evse("NL", "CPO", "LOC1", "EVSE1").unwrap();
        assert_eq!(evse1.status, Status::Inoperative);
    }

    #[test]
    fn locations_config_put_evse_unknown_location_returns_not_found() {
        let cfg = LocationsConfig::new();
        let ts = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let err = cfg
            .put_evse(
                "NL",
                "CPO",
                "MISSING",
                make_evse("EVSE1", Status::Available, ts),
            )
            .unwrap_err();
        assert!(matches!(err, ServerError::NotFound));
    }

    #[test]
    fn locations_config_patch_evse_updates_status() {
        let cfg = LocationsConfig::new();
        let ts = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        cfg.put(make_location("LOC1", ts));
        let patch = ocpi_types::serde_json::json!({ "status": "CHARGING" });
        cfg.patch_evse("NL", "CPO", "LOC1", "EVSE1", patch).unwrap();
        let evse = cfg.get_evse("NL", "CPO", "LOC1", "EVSE1").unwrap();
        assert_eq!(evse.status, Status::Charging);
        // The nested connector is untouched by the EVSE merge-patch.
        assert_eq!(evse.connectors.len(), 1);
    }

    #[test]
    fn locations_config_patch_evse_unknown_returns_not_found() {
        let cfg = LocationsConfig::new();
        let ts = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        cfg.put(make_location("LOC1", ts));
        let patch = ocpi_types::serde_json::json!({ "status": "CHARGING" });
        let err = cfg
            .patch_evse("NL", "CPO", "LOC1", "NOPE", patch)
            .unwrap_err();
        assert!(matches!(err, ServerError::NotFound));
    }

    #[test]
    fn locations_config_put_and_patch_connector() {
        let cfg = LocationsConfig::new();
        let ts = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        cfg.put(make_location("LOC1", ts));
        // Append a second connector to the EVSE.
        cfg.put_connector("NL", "CPO", "LOC1", "EVSE1", make_connector("2", ts))
            .unwrap();
        let evse = cfg.get_evse("NL", "CPO", "LOC1", "EVSE1").unwrap();
        assert_eq!(evse.connectors.len(), 2);
        // Merge-patch the connector's amperage.
        let patch = ocpi_types::serde_json::json!({ "max_amperage": 32 });
        cfg.patch_connector("NL", "CPO", "LOC1", "EVSE1", "1", patch)
            .unwrap();
        let connector = cfg
            .get_connector("NL", "CPO", "LOC1", "EVSE1", "1")
            .unwrap();
        assert_eq!(connector.max_amperage, 32);
    }

    #[test]
    fn locations_config_put_connector_unknown_evse_returns_not_found() {
        let cfg = LocationsConfig::new();
        let ts = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        cfg.put(make_location("LOC1", ts));
        let err = cfg
            .put_connector("NL", "CPO", "LOC1", "NOPE", make_connector("1", ts))
            .unwrap_err();
        assert!(matches!(err, ServerError::NotFound));
    }
}

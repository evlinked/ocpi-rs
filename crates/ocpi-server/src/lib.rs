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
        AuthorizationInfo, CancelReservation, Cdr, ClientInfo, CommandResponse,
        CommandResponseType, CommandResult, CommandType, Connector, Credentials, Evse, Location,
        LocationReferences, ReserveNow, Session, StartSession, StopSession, Tariff, Token,
        TokenType, UnlockConnector,
    },
    version::{Endpoint, Version, VersionDetails, VersionNumber},
    DateTime, OcpiStatusCode, Utc,
};

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
    /// Endpoint catalogue keyed by version number.
    pub details: std::collections::HashMap<VersionNumber, VersionDetails>,
}

impl VersionsConfig {
    /// Create an empty registry; add entries with
    /// [`add_version`](Self::add_version).
    #[must_use]
    pub fn new() -> Self {
        Self {
            versions: Vec::new(),
            details: std::collections::HashMap::new(),
        }
    }

    /// Register a version and its endpoint catalogue.
    pub fn add_version(&mut self, entry: Version, details: VersionDetails) {
        self.versions.push(entry);
        self.details.insert(details.version, details);
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
        routing::{get, post},
        Json, Router,
    };
    use ocpi_types::{
        envelope::{OcpiPaged, OcpiResponse},
        transport::{CredentialToken, PaginatedParams},
        v2_2_1::{
            AuthorizationInfo, CancelReservation, Cdr, ClientInfo, CommandResponse, CommandResult,
            CommandType, Connector, Credentials, Evse, Location, LocationReferences, ReserveNow,
            Session, StartSession, StopSession, Tariff, Token, TokenType, UnlockConnector,
        },
        version::{VersionDetails, VersionNumber},
        OcpiStatusCode,
    };

    use crate::{
        token_type_str, CdrsConfig, CommandsConfig, CommandsHandler, CredentialsConfig,
        HubClientInfoConfig, LocationsConfig, ServerError, SessionsConfig, TariffsConfig,
        TokensConfig, VersionsConfig,
    };

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
        // Reject re-registration before running the (potentially expensive)
        // fetch-back.
        if cfg.is_registered(token.as_str()) {
            return credentials_method_not_allowed("already registered");
        }
        // Spec §POST: the receiver fetches the sender's endpoints for the
        // registered version. Any failure → status code 3001.
        let endpoints = match cfg.fetch_back(&body).await {
            Ok(endpoints) => endpoints,
            Err(_) => return credentials_unable_to_use_client(),
        };
        match cfg.register_with_endpoints(token.as_str(), body, endpoints) {
            Ok(()) => Json(OcpiResponse::success(cfg.own_credentials.clone())).into_response(),
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
        // Reject updates from unknown parties before the fetch-back.
        if !cfg.is_registered(token.as_str()) {
            return credentials_method_not_allowed("not registered");
        }
        // Spec §PUT: re-fetch the sender's endpoints on credential update.
        let endpoints = match cfg.fetch_back(&body).await {
            Ok(endpoints) => endpoints,
            Err(_) => return credentials_unable_to_use_client(),
        };
        match cfg.update_with_endpoints(token.as_str(), body, endpoints) {
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

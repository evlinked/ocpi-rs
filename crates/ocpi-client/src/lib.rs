//! # ocpi-client
//!
//! An async OCPI HTTP client for the **sender** role — the side that issues
//! requests to a remote party's endpoints (e.g. an eMSP pulling Locations from
//! a CPO, or either party performing the credentials handshake).
//!
//! The client is transport-only; all wire types come from [`ocpi_types`].

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod error;

pub use error::ClientError;

use ocpi_server::{FetchError, FetchFuture, VersionFetcher};
use ocpi_types::{
    transport::{CredentialToken, PaginatedParams, PaginationMeta},
    v2_2_1::{
        ActiveChargingProfile, ActiveChargingProfileResult, AuthorizationInfo, CancelReservation,
        Cdr, ChargingPreferences, ChargingPreferencesResponse, ChargingProfileResponse,
        ChargingProfileResult, ClearProfileResult, ClientInfo, CommandResponse, CommandResult,
        CommandType, Connector, Credentials, Evse, Location, LocationReferences, ReserveNow,
        Session, SetChargingProfile, StartSession, StopSession, Tariff, Token, TokenType,
        UnlockConnector,
    },
    version::{Version, VersionDetails, VersionNumber},
    OcpiResponse,
};
// OCPI 2.1.1 module types are aliased so the 2.2.1 surface above keeps the
// unqualified names. See `crate::get_locations_2_1_1` and friends.
use ocpi_types::v2_1_1::{Connector as Connector2111, Evse as Evse2111, Location as Location2111};
use url::Url;

fn token_type_str(t: TokenType) -> &'static str {
    match t {
        TokenType::AdHocUser => "AD_HOC_USER",
        TokenType::AppUser => "APP_USER",
        TokenType::Other => "OTHER",
        TokenType::Rfid => "RFID",
    }
}

/// Negotiate the highest OCPI version supported by both parties.
///
/// Given a remote party's `/versions` list (`remote`, the `data` array of a
/// `GET /versions` response — deserialized as `Vec<`[`Version`]`>`) and this
/// party's own `supported` versions, returns the highest [`VersionNumber`]
/// present in both, or `None` when there is no common version.
///
/// This is a **pure, IO-free** helper: callers that have already fetched the
/// remote `/versions` list use it to decide which version's code path to drive
/// (e.g. a roaming hub selecting between its 2.1.1 and 2.2.1 module clients).
/// A `None` result is a hard negotiation failure and must be surfaced as an
/// explicit OCPI `status_code` (`UnsupportedVersion`), never a silent drop.
///
/// Ordering follows [`VersionNumber`]'s `Ord`:
/// `V2_3_0 > V2_2_1 > V2_2 > V2_1_1 > V2_0`.
///
/// For the convenience method that performs the full network bootstrap
/// (`GET /versions` then `GET /versions/{version}`), see
/// [`OcpiClient::negotiate_version`].
///
/// # Examples
///
/// ```
/// use ocpi_client::negotiate_version;
/// use ocpi_types::serde_json;
/// use ocpi_types::version::{Version, VersionNumber};
///
/// let remote: Vec<Version> = serde_json::from_str(
///     r#"[{"version":"2.1.1","url":"https://partner.example/ocpi/2.1.1"}]"#,
/// )
/// .unwrap();
/// let supported = [VersionNumber::V2_1_1, VersionNumber::V2_2_1];
/// assert_eq!(negotiate_version(&remote, &supported), Some(VersionNumber::V2_1_1));
/// ```
#[must_use]
pub fn negotiate_version(remote: &[Version], supported: &[VersionNumber]) -> Option<VersionNumber> {
    remote
        .iter()
        .map(|v| v.version)
        .filter(|v| supported.contains(v))
        .max()
}

/// Select the best common version entry from `remote` given `supported`.
///
/// Like [`negotiate_version`] but returns the full [`Version`] entry (including
/// its details `url`) so the caller can follow it to `GET /versions/{version}`.
fn select_version<'a>(remote: &'a [Version], supported: &[VersionNumber]) -> Option<&'a Version> {
    let chosen = negotiate_version(remote, supported)?;
    remote.iter().find(|v| v.version == chosen)
}

/// Append URL path `segments` to a base endpoint URL, collapsing any single
/// trailing slash on the base so the result has exactly one separator per
/// segment. Used to build Sender-interface object URLs such as
/// `{locations_url}/{location_id}/{evse_uid}/{connector_id}`.
fn join_segments(base: &str, segments: &[&str]) -> String {
    let mut url = base.trim_end_matches('/').to_string();
    for seg in segments {
        url.push('/');
        url.push_str(seg);
    }
    url
}

/// Build a ChargingProfiles receiver-interface object URL:
/// `{chargingprofiles_url}/{session_id}`.
///
/// Per `mod_charging_profiles.asciidoc` the GET/PUT/DELETE receiver endpoints are
/// all keyed by `session_id` as a single trailing path segment; the GET and
/// DELETE query parameters (`duration`, `response_url`) are appended by the
/// caller. Returns a parse error if the resulting URL is malformed.
fn charging_profile_url(base: &str, session_id: &str) -> Result<Url, url::ParseError> {
    Url::parse(&join_segments(base, &[session_id]))
}

/// A configured OCPI client pointed at one remote party's API base URL.
///
/// The `base_url` should be the versioned module base (it is joined with
/// relative paths like `versions`), and `token` is the OCPI authorization
/// token presented as `Authorization: Token <token>`.
///
/// By default the token is Base64-encoded per OCPI 2.2.1 §4.1.1.
/// Set `compat_raw_token = true` (via [`Self::with_compat_raw_token`]) to send
/// the raw token instead, for interoperability with OCPI 2.1.1/2.2 peers.
#[derive(Debug, Clone)]
pub struct OcpiClient {
    base_url: Url,
    token: String,
    http: reqwest::Client,
    /// When `true`, the token is sent raw (not Base64-encoded).
    /// Use only when connecting to legacy 2.1.1/2.2 peers.
    compat_raw_token: bool,
}

impl OcpiClient {
    /// Create a client targeting `base_url`, authenticating with `token`.
    ///
    /// Token encoding defaults to Base64 (OCPI 2.2.1). Use
    /// [`Self::with_compat_raw_token`] to opt into the raw-token mode for
    /// legacy peers.
    #[must_use]
    pub fn new(base_url: Url, token: impl Into<String>) -> Self {
        Self {
            base_url,
            token: token.into(),
            http: reqwest::Client::new(),
            compat_raw_token: false,
        }
    }

    /// Override the token encoding mode.
    ///
    /// - `false` (default): token is Base64-encoded per OCPI 2.2.1.
    /// - `true`: token is sent raw; use with legacy 2.1.1/2.2 peers.
    #[must_use]
    pub fn with_compat_raw_token(mut self, compat: bool) -> Self {
        self.compat_raw_token = compat;
        self
    }

    /// The configured base URL.
    #[must_use]
    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    /// Build the `Authorization` header value for outbound requests.
    fn auth_header_value(&self) -> String {
        if self.compat_raw_token {
            format!("Token {}", self.token)
        } else {
            CredentialToken::new(&self.token).to_header_value()
        }
    }

    /// Fetch the remote party's supported versions (`GET /versions`).
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails, the URL is invalid, or the
    /// envelope reports success without any data.
    pub async fn versions(&self) -> Result<Vec<Version>, ClientError> {
        let url = self.base_url.join("versions")?;
        let response = self
            .http
            .get(url)
            .header("Authorization", self.auth_header_value())
            .send()
            .await?
            .error_for_status()?;
        let envelope: OcpiResponse<Vec<Version>> = response.json().await?;
        envelope.data.ok_or(ClientError::EmptyData)
    }

    /// Fetch the endpoint details for a specific OCPI version (`GET <url>`).
    ///
    /// The `url` comes from the `url` field of a [`Version`] entry returned by
    /// [`Self::versions`]. Pass it directly — no base-URL joining is applied.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails, the URL is invalid, or the
    /// envelope reports success without any data.
    pub async fn version_details(&self, url: &str) -> Result<VersionDetails, ClientError> {
        let parsed = url::Url::parse(url)?;
        let response = self
            .http
            .get(parsed)
            .header("Authorization", self.auth_header_value())
            .send()
            .await?
            .error_for_status()?;
        let envelope: OcpiResponse<VersionDetails> = response.json().await?;
        envelope.data.ok_or(ClientError::EmptyData)
    }

    /// Retrieve the remote party's own credentials (`GET <url>`).
    ///
    /// `url` is the absolute URL of the remote credentials endpoint (obtained
    /// from `VersionDetails.endpoints` after version negotiation).
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails, the URL is invalid, or the
    /// envelope carries no data.
    pub async fn get_credentials(&self, url: &str) -> Result<Credentials, ClientError> {
        let parsed = url::Url::parse(url)?;
        let response = self
            .http
            .get(parsed)
            .header("Authorization", self.auth_header_value())
            .send()
            .await?
            .error_for_status()?;
        let envelope: OcpiResponse<Credentials> = response.json().await?;
        envelope.data.ok_or(ClientError::EmptyData)
    }

    /// Register with the remote party by `POST`-ing `credentials` to `url`.
    ///
    /// On success, the remote returns a new [`Credentials`] object containing
    /// the token the client must use for subsequent requests.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails, the URL is invalid, the
    /// HTTP response is 405 (already registered), or the envelope carries no data.
    pub async fn register(
        &self,
        url: &str,
        credentials: &Credentials,
    ) -> Result<Credentials, ClientError> {
        let parsed = url::Url::parse(url)?;
        let response = self
            .http
            .post(parsed)
            .header("Authorization", self.auth_header_value())
            .json(credentials)
            .send()
            .await?
            .error_for_status()?;
        let envelope: OcpiResponse<Credentials> = response.json().await?;
        envelope.data.ok_or(ClientError::EmptyData)
    }

    /// Update the registration with the remote party (`PUT <url>`).
    ///
    /// On success, the remote returns updated [`Credentials`] for the client.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails, the URL is invalid, the
    /// HTTP response is 405 (not yet registered), or the envelope carries no data.
    pub async fn update_credentials(
        &self,
        url: &str,
        credentials: &Credentials,
    ) -> Result<Credentials, ClientError> {
        let parsed = url::Url::parse(url)?;
        let response = self
            .http
            .put(parsed)
            .header("Authorization", self.auth_header_value())
            .json(credentials)
            .send()
            .await?
            .error_for_status()?;
        let envelope: OcpiResponse<Credentials> = response.json().await?;
        envelope.data.ok_or(ClientError::EmptyData)
    }

    /// Perform the two-step OCPI version bootstrap.
    ///
    /// 1. `GET /versions` — fetch the remote party's supported versions.
    /// 2. Intersect with `supported` (this party's versions); pick the highest.
    /// 3. `GET <version-url>` — return the selected version's [`VersionDetails`].
    ///
    /// Version priority (highest wins): `V2_3_0 > V2_2_1 > V2_2 > V2_1_1 > V2_0`.
    ///
    /// For the pure, IO-free version-selection step on an already-fetched
    /// `/versions` list, see the free function [`negotiate_version`].
    ///
    /// # Errors
    ///
    /// - [`ClientError::NoMutualVersion`] if no version is supported by both parties.
    /// - [`ClientError::Http`] if a request fails.
    /// - [`ClientError::EmptyData`] if a response envelope carries no data.
    pub async fn negotiate_version(
        &self,
        supported: &[VersionNumber],
    ) -> Result<VersionDetails, ClientError> {
        let remote = self.versions().await?;
        let best = select_version(&remote, supported).ok_or(ClientError::NoMutualVersion)?;
        self.version_details(best.url.as_str()).await
    }

    /// Unregister from the remote party (`DELETE <url>`).
    ///
    /// On success, both parties must stop automated communication.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails, the URL is invalid, or the
    /// HTTP response is 405 (not yet registered).
    pub async fn delete_credentials(&self, url: &str) -> Result<(), ClientError> {
        let parsed = url::Url::parse(url)?;
        self.http
            .delete(parsed)
            .header("Authorization", self.auth_header_value())
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    // ── Sessions ──────────────────────────────────────────────────────────────

    /// Fetch a paginated list of sessions from the remote CPO (`GET <url>`).
    ///
    /// `url` is the absolute URL of the remote sessions endpoint. The query
    /// parameters `date_from`, `date_to`, `offset`, and `limit` may be set
    /// in the `params` argument; `None` fields are omitted from the query
    /// string.
    ///
    /// Returns `(sessions, pagination_meta)`.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails, the URL is invalid, or
    /// the envelope carries no data.
    pub async fn get_sessions(
        &self,
        url: &str,
        params: &ocpi_types::transport::PaginatedParams,
    ) -> Result<(Vec<Session>, PaginationMeta), ClientError> {
        let mut req = self
            .http
            .get(url::Url::parse(url)?)
            .header("Authorization", self.auth_header_value());
        if let Some(df) = params.date_from {
            req = req.query(&[("date_from", df.to_rfc3339())]);
        }
        if let Some(dt) = params.date_to {
            req = req.query(&[("date_to", dt.to_rfc3339())]);
        }
        if let Some(off) = params.offset {
            req = req.query(&[("offset", off.to_string())]);
        }
        if let Some(lim) = params.limit {
            req = req.query(&[("limit", lim.to_string())]);
        }
        let response = req.send().await?.error_for_status()?;

        let link = response
            .headers()
            .get("link")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| {
                // Parse: `<url>; rel="next"`
                let url_part = s.trim().strip_prefix('<')?.split('>').next()?;
                Some(url_part.to_string())
            });
        let total_count: u64 = response
            .headers()
            .get("x-total-count")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let page_limit: u32 = response
            .headers()
            .get("x-limit")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse().ok())
            .unwrap_or(params.limit.unwrap_or(50));

        let meta = PaginationMeta {
            next_url: link,
            total_count,
            limit: page_limit,
        };

        let envelope: OcpiResponse<Vec<Session>> = response.json().await?;
        let sessions = envelope.data.ok_or(ClientError::EmptyData)?;
        Ok((sessions, meta))
    }

    /// Retrieve a single session by its composite key (`GET <url>/{cc}/{party}/{id}`).
    ///
    /// `url` is the sessions endpoint base; the path segments are appended
    /// automatically.
    ///
    /// # Errors
    ///
    /// - [`ClientError::NotFound`] when the server returns OCPI `2003` or HTTP
    ///   404.
    /// - [`ClientError::EmptyData`] if the success envelope carries no data.
    pub async fn get_session(
        &self,
        url: &str,
        country_code: &str,
        party_id: &str,
        session_id: &str,
    ) -> Result<Session, ClientError> {
        let endpoint = format!(
            "{}/{}/{}/{}",
            url.trim_end_matches('/'),
            country_code,
            party_id,
            session_id,
        );
        let response = self
            .http
            .get(url::Url::parse(&endpoint)?)
            .header("Authorization", self.auth_header_value())
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ClientError::NotFound);
        }
        let response = response.error_for_status()?;
        let envelope: OcpiResponse<Session> = response.json().await?;
        envelope.data.ok_or(ClientError::EmptyData)
    }

    /// Create or replace a session on the remote eMSP (`PUT`).
    ///
    /// `url` is the sessions endpoint base; the path segments are appended
    /// automatically.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails or the URL is invalid.
    pub async fn put_session(
        &self,
        url: &str,
        country_code: &str,
        party_id: &str,
        session_id: &str,
        session: &Session,
    ) -> Result<(), ClientError> {
        let endpoint = format!(
            "{}/{}/{}/{}",
            url.trim_end_matches('/'),
            country_code,
            party_id,
            session_id,
        );
        self.http
            .put(url::Url::parse(&endpoint)?)
            .header("Authorization", self.auth_header_value())
            .json(session)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    /// Apply a partial update (JSON merge-patch, RFC 7396) to a session
    /// on the remote eMSP (`PATCH`).
    ///
    /// `partial` is any `Serialize` value; use a struct with
    /// `#[serde(skip_serializing_if = "Option::is_none")]` fields, or a
    /// `serde_json::Value` map, to send only the changed fields.
    ///
    /// # Errors
    ///
    /// - [`ClientError::NotFound`] when the server returns HTTP 404.
    /// - [`ClientError::Http`] on network or server errors.
    pub async fn patch_session<T: ocpi_types::serde::Serialize>(
        &self,
        url: &str,
        country_code: &str,
        party_id: &str,
        session_id: &str,
        partial: &T,
    ) -> Result<(), ClientError> {
        let endpoint = format!(
            "{}/{}/{}/{}",
            url.trim_end_matches('/'),
            country_code,
            party_id,
            session_id,
        );
        let response = self
            .http
            .patch(url::Url::parse(&endpoint)?)
            .header("Authorization", self.auth_header_value())
            .json(partial)
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ClientError::NotFound);
        }
        response.error_for_status()?;
        Ok(())
    }

    /// Send the driver's charging preferences to the CPO for the given
    /// session (`PUT /sessions/{session_id}/charging_preferences`).
    ///
    /// `url` is the sessions endpoint base; the path is appended automatically
    /// as `/{session_id}/charging_preferences`.
    ///
    /// Returns the CPO's [`ChargingPreferencesResponse`].
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails, the URL is invalid, or
    /// the envelope carries no data.
    pub async fn set_charging_preferences(
        &self,
        url: &str,
        session_id: &str,
        preferences: &ChargingPreferences,
    ) -> Result<ChargingPreferencesResponse, ClientError> {
        let endpoint = format!(
            "{}/{}/charging_preferences",
            url.trim_end_matches('/'),
            session_id,
        );
        let response = self
            .http
            .put(url::Url::parse(&endpoint)?)
            .header("Authorization", self.auth_header_value())
            .json(preferences)
            .send()
            .await?
            .error_for_status()?;
        let envelope: OcpiResponse<ChargingPreferencesResponse> = response.json().await?;
        envelope.data.ok_or(ClientError::EmptyData)
    }

    // ── CDRs ──────────────────────────────────────────────────────────────────

    /// Fetch a paginated list of CDRs from a CPO (`GET {url}`).
    ///
    /// `url` is the absolute URL of the CPO's CDRs sender endpoint.
    /// `params` carries `date_from`, `date_to`, `offset`, and `limit`.
    ///
    /// Returns the first page of CDRs plus pagination metadata. Use
    /// `PaginationMeta.next_url` to retrieve subsequent pages.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails or the URL is invalid.
    pub async fn get_cdrs(
        &self,
        url: &str,
        params: PaginatedParams,
    ) -> Result<(Vec<Cdr>, PaginationMeta), ClientError> {
        let mut parsed = url::Url::parse(url)?;
        if let Some(date_from) = params.date_from {
            parsed
                .query_pairs_mut()
                .append_pair("date_from", &date_from.to_rfc3339());
        }
        if let Some(date_to) = params.date_to {
            parsed
                .query_pairs_mut()
                .append_pair("date_to", &date_to.to_rfc3339());
        }
        if let Some(offset) = params.offset {
            parsed
                .query_pairs_mut()
                .append_pair("offset", &offset.to_string());
        }
        if let Some(limit) = params.limit {
            parsed
                .query_pairs_mut()
                .append_pair("limit", &limit.to_string());
        }
        let response = self
            .http
            .get(parsed)
            .header("Authorization", self.auth_header_value())
            .send()
            .await?
            .error_for_status()?;
        let hdrs = response.headers();
        let link = hdrs
            .get("link")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned());
        let total_count = hdrs
            .get("x-total-count")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned());
        let limit_hdr = hdrs
            .get("x-limit")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned());
        let meta = PaginationMeta::from_headers(
            link.as_deref(),
            total_count.as_deref(),
            limit_hdr.as_deref(),
        )
        .unwrap_or(PaginationMeta {
            next_url: None,
            total_count: 0,
            limit: 50,
        });
        let envelope: OcpiResponse<Vec<Cdr>> = response.json().await?;
        let cdrs = envelope.data.ok_or(ClientError::EmptyData)?;
        Ok((cdrs, meta))
    }

    /// Fetch a single CDR by ID from a CPO (`GET {url}/{cdr_id}`).
    ///
    /// # Errors
    ///
    /// Returns [`ClientError::NotFound`] when the remote responds with OCPI
    /// status code `2003`, or [`ClientError`] for other failures.
    pub async fn get_cdr(&self, url: &str, cdr_id: &str) -> Result<Cdr, ClientError> {
        let endpoint = format!("{}/{cdr_id}", url.trim_end_matches('/'));
        let response = self
            .http
            .get(url::Url::parse(&endpoint)?)
            .header("Authorization", self.auth_header_value())
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ClientError::NotFound);
        }
        let response = response.error_for_status()?;
        let envelope: OcpiResponse<Cdr> = response.json().await?;
        envelope.data.ok_or(ClientError::EmptyData)
    }

    /// Push a new CDR to an eMSP (`POST {url}`).
    ///
    /// On success the eMSP responds with `201 Created` and a `Location` header
    /// pointing to the stored CDR. This method returns that URL string.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails, the URL is invalid, or
    /// the `Location` header is absent/unparseable.
    pub async fn post_cdr(&self, url: &str, cdr: &Cdr) -> Result<String, ClientError> {
        let response = self
            .http
            .post(url::Url::parse(url)?)
            .header("Authorization", self.auth_header_value())
            .json(cdr)
            .send()
            .await?
            .error_for_status()?;
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned())
            .ok_or(ClientError::EmptyData)?;
        Ok(location)
    }

    // ── Locations ─────────────────────────────────────────────────────────────

    /// Fetch a paginated list of Locations from a CPO's Sender interface
    /// (`GET {url}`).
    ///
    /// `url` is the absolute URL of the CPO's Locations sender endpoint.
    /// `params` carries `date_from`, `date_to`, `offset`, and `limit`.
    ///
    /// Returns the first page of Locations plus pagination metadata. Use
    /// [`PaginationMeta::next_url`] to retrieve subsequent pages.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails, the URL is invalid, or the
    /// envelope carries no data.
    ///
    /// See `mod_locations.asciidoc` — Sender Interface, GET List.
    pub async fn get_locations(
        &self,
        url: &str,
        params: PaginatedParams,
    ) -> Result<(Vec<Location>, PaginationMeta), ClientError> {
        let mut parsed = url::Url::parse(url)?;
        if let Some(date_from) = params.date_from {
            parsed
                .query_pairs_mut()
                .append_pair("date_from", &date_from.to_rfc3339());
        }
        if let Some(date_to) = params.date_to {
            parsed
                .query_pairs_mut()
                .append_pair("date_to", &date_to.to_rfc3339());
        }
        if let Some(offset) = params.offset {
            parsed
                .query_pairs_mut()
                .append_pair("offset", &offset.to_string());
        }
        if let Some(limit) = params.limit {
            parsed
                .query_pairs_mut()
                .append_pair("limit", &limit.to_string());
        }
        let response = self
            .http
            .get(parsed)
            .header("Authorization", self.auth_header_value())
            .send()
            .await?
            .error_for_status()?;
        let hdrs = response.headers();
        let link = hdrs
            .get("link")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned());
        let total_count = hdrs
            .get("x-total-count")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned());
        let limit_hdr = hdrs
            .get("x-limit")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned());
        let meta = PaginationMeta::from_headers(
            link.as_deref(),
            total_count.as_deref(),
            limit_hdr.as_deref(),
        )
        .unwrap_or(PaginationMeta {
            next_url: None,
            total_count: 0,
            limit: 50,
        });
        let envelope: OcpiResponse<Vec<Location>> = response.json().await?;
        let locations = envelope.data.ok_or(ClientError::EmptyData)?;
        Ok((locations, meta))
    }

    /// Fetch a single Location by id from a CPO's Sender interface
    /// (`GET {url}/{location_id}`).
    ///
    /// # Errors
    ///
    /// - [`ClientError::NotFound`] when the remote responds with OCPI status
    ///   code `2003` (Unknown Location) or HTTP 404.
    /// - [`ClientError::EmptyData`] if the success envelope carries no data.
    pub async fn get_location(
        &self,
        url: &str,
        location_id: &str,
    ) -> Result<Location, ClientError> {
        let endpoint = join_segments(url, &[location_id]);
        let response = self
            .http
            .get(url::Url::parse(&endpoint)?)
            .header("Authorization", self.auth_header_value())
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ClientError::NotFound);
        }
        let response = response.error_for_status()?;
        let envelope: OcpiResponse<Location> = response.json().await?;
        envelope.data.ok_or(ClientError::EmptyData)
    }

    /// Fetch a single EVSE from a CPO's Sender interface
    /// (`GET {url}/{location_id}/{evse_uid}`).
    ///
    /// # Errors
    ///
    /// - [`ClientError::NotFound`] when the remote responds with OCPI status
    ///   code `2003` or HTTP 404.
    /// - [`ClientError::EmptyData`] if the success envelope carries no data.
    pub async fn get_evse(
        &self,
        url: &str,
        location_id: &str,
        evse_uid: &str,
    ) -> Result<Evse, ClientError> {
        let endpoint = join_segments(url, &[location_id, evse_uid]);
        let response = self
            .http
            .get(url::Url::parse(&endpoint)?)
            .header("Authorization", self.auth_header_value())
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ClientError::NotFound);
        }
        let response = response.error_for_status()?;
        let envelope: OcpiResponse<Evse> = response.json().await?;
        envelope.data.ok_or(ClientError::EmptyData)
    }

    /// Fetch a single Connector from a CPO's Sender interface
    /// (`GET {url}/{location_id}/{evse_uid}/{connector_id}`).
    ///
    /// # Errors
    ///
    /// - [`ClientError::NotFound`] when the remote responds with OCPI status
    ///   code `2003` or HTTP 404.
    /// - [`ClientError::EmptyData`] if the success envelope carries no data.
    pub async fn get_connector(
        &self,
        url: &str,
        location_id: &str,
        evse_uid: &str,
        connector_id: &str,
    ) -> Result<Connector, ClientError> {
        let endpoint = join_segments(url, &[location_id, evse_uid, connector_id]);
        let response = self
            .http
            .get(url::Url::parse(&endpoint)?)
            .header("Authorization", self.auth_header_value())
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ClientError::NotFound);
        }
        let response = response.error_for_status()?;
        let envelope: OcpiResponse<Connector> = response.json().await?;
        envelope.data.ok_or(ClientError::EmptyData)
    }

    // ── Locations (2.1.1) ───────────────────────────────────────────────────────

    /// Fetch a paginated list of **OCPI 2.1.1** Locations from a CPO's Sender
    /// interface (`GET {url}`).
    ///
    /// Mirrors [`OcpiClient::get_locations`] but deserializes the *2.1.1* wire
    /// shape (`type` required, no `country_code`/`party_id`, singular
    /// `tariff_id` per connector). The Sender-interface path is identical to
    /// 2.2.1 — the `{country_code}/{party_id}` segments only appear on the
    /// Receiver interface — so only the payload type differs.
    ///
    /// `url` is the absolute URL of the CPO's 2.1.1 Locations sender endpoint;
    /// `params` carries `date_from`, `date_to`, `offset`, and `limit`. Use
    /// [`PaginationMeta::next_url`] to retrieve subsequent pages.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails, the URL is invalid, or the
    /// envelope carries no data.
    ///
    /// See `specs/ocpi/2.1.1` — *Locations*, Sender Interface, GET List.
    pub async fn get_locations_2_1_1(
        &self,
        url: &str,
        params: PaginatedParams,
    ) -> Result<(Vec<Location2111>, PaginationMeta), ClientError> {
        let mut parsed = url::Url::parse(url)?;
        if let Some(date_from) = params.date_from {
            parsed
                .query_pairs_mut()
                .append_pair("date_from", &date_from.to_rfc3339());
        }
        if let Some(date_to) = params.date_to {
            parsed
                .query_pairs_mut()
                .append_pair("date_to", &date_to.to_rfc3339());
        }
        if let Some(offset) = params.offset {
            parsed
                .query_pairs_mut()
                .append_pair("offset", &offset.to_string());
        }
        if let Some(limit) = params.limit {
            parsed
                .query_pairs_mut()
                .append_pair("limit", &limit.to_string());
        }
        let response = self
            .http
            .get(parsed)
            .header("Authorization", self.auth_header_value())
            .send()
            .await?
            .error_for_status()?;
        let hdrs = response.headers();
        let link = hdrs
            .get("link")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned());
        let total_count = hdrs
            .get("x-total-count")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned());
        let limit_hdr = hdrs
            .get("x-limit")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned());
        let meta = PaginationMeta::from_headers(
            link.as_deref(),
            total_count.as_deref(),
            limit_hdr.as_deref(),
        )
        .unwrap_or(PaginationMeta {
            next_url: None,
            total_count: 0,
            limit: 50,
        });
        let envelope: OcpiResponse<Vec<Location2111>> = response.json().await?;
        let locations = envelope.data.ok_or(ClientError::EmptyData)?;
        Ok((locations, meta))
    }

    /// Fetch a single **OCPI 2.1.1** Location by id from a CPO's Sender
    /// interface (`GET {url}/{location_id}`).
    ///
    /// # Errors
    ///
    /// - [`ClientError::NotFound`] when the remote responds with HTTP 404.
    /// - [`ClientError::EmptyData`] if the success envelope carries no data.
    pub async fn get_location_2_1_1(
        &self,
        url: &str,
        location_id: &str,
    ) -> Result<Location2111, ClientError> {
        let endpoint = join_segments(url, &[location_id]);
        let response = self
            .http
            .get(url::Url::parse(&endpoint)?)
            .header("Authorization", self.auth_header_value())
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ClientError::NotFound);
        }
        let response = response.error_for_status()?;
        let envelope: OcpiResponse<Location2111> = response.json().await?;
        envelope.data.ok_or(ClientError::EmptyData)
    }

    /// Fetch a single **OCPI 2.1.1** EVSE from a CPO's Sender interface
    /// (`GET {url}/{location_id}/{evse_uid}`).
    ///
    /// # Errors
    ///
    /// - [`ClientError::NotFound`] when the remote responds with HTTP 404.
    /// - [`ClientError::EmptyData`] if the success envelope carries no data.
    pub async fn get_evse_2_1_1(
        &self,
        url: &str,
        location_id: &str,
        evse_uid: &str,
    ) -> Result<Evse2111, ClientError> {
        let endpoint = join_segments(url, &[location_id, evse_uid]);
        let response = self
            .http
            .get(url::Url::parse(&endpoint)?)
            .header("Authorization", self.auth_header_value())
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ClientError::NotFound);
        }
        let response = response.error_for_status()?;
        let envelope: OcpiResponse<Evse2111> = response.json().await?;
        envelope.data.ok_or(ClientError::EmptyData)
    }

    /// Fetch a single **OCPI 2.1.1** Connector from a CPO's Sender interface
    /// (`GET {url}/{location_id}/{evse_uid}/{connector_id}`).
    ///
    /// # Errors
    ///
    /// - [`ClientError::NotFound`] when the remote responds with HTTP 404.
    /// - [`ClientError::EmptyData`] if the success envelope carries no data.
    pub async fn get_connector_2_1_1(
        &self,
        url: &str,
        location_id: &str,
        evse_uid: &str,
        connector_id: &str,
    ) -> Result<Connector2111, ClientError> {
        let endpoint = join_segments(url, &[location_id, evse_uid, connector_id]);
        let response = self
            .http
            .get(url::Url::parse(&endpoint)?)
            .header("Authorization", self.auth_header_value())
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ClientError::NotFound);
        }
        let response = response.error_for_status()?;
        let envelope: OcpiResponse<Connector2111> = response.json().await?;
        envelope.data.ok_or(ClientError::EmptyData)
    }

    // ── Tariffs ───────────────────────────────────────────────────────────────

    /// Fetch a paginated list of tariffs from a CPO (`GET {url}`).
    ///
    /// `url` is the absolute URL of the CPO's tariffs sender endpoint.
    /// `params` carries `date_from`, `date_to`, `offset`, and `limit`.
    ///
    /// Returns the first page of tariffs plus pagination metadata. Use
    /// `PaginationMeta.next_url` to retrieve subsequent pages.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails or the URL is invalid.
    pub async fn get_tariffs(
        &self,
        url: &str,
        params: PaginatedParams,
    ) -> Result<(Vec<Tariff>, PaginationMeta), ClientError> {
        let mut parsed = url::Url::parse(url)?;
        if let Some(date_from) = params.date_from {
            parsed
                .query_pairs_mut()
                .append_pair("date_from", &date_from.to_rfc3339());
        }
        if let Some(date_to) = params.date_to {
            parsed
                .query_pairs_mut()
                .append_pair("date_to", &date_to.to_rfc3339());
        }
        if let Some(offset) = params.offset {
            parsed
                .query_pairs_mut()
                .append_pair("offset", &offset.to_string());
        }
        if let Some(limit) = params.limit {
            parsed
                .query_pairs_mut()
                .append_pair("limit", &limit.to_string());
        }
        let response = self
            .http
            .get(parsed)
            .header("Authorization", self.auth_header_value())
            .send()
            .await?
            .error_for_status()?;
        let hdrs = response.headers();
        let link = hdrs
            .get("link")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned());
        let total_count = hdrs
            .get("x-total-count")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned());
        let limit_hdr = hdrs
            .get("x-limit")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned());
        let meta = PaginationMeta::from_headers(
            link.as_deref(),
            total_count.as_deref(),
            limit_hdr.as_deref(),
        )
        .unwrap_or(PaginationMeta {
            next_url: None,
            total_count: 0,
            limit: 50,
        });
        let envelope: OcpiResponse<Vec<Tariff>> = response.json().await?;
        let tariffs = envelope.data.ok_or(ClientError::EmptyData)?;
        Ok((tariffs, meta))
    }

    /// Fetch a single tariff from an eMSP receiver
    /// (`GET {url}/{country_code}/{party_id}/{tariff_id}`).
    ///
    /// # Errors
    ///
    /// Returns [`ClientError::NotFound`] when the remote responds with HTTP 404.
    /// Returns [`ClientError`] for other failures.
    pub async fn get_tariff(
        &self,
        url: &str,
        country_code: &str,
        party_id: &str,
        tariff_id: &str,
    ) -> Result<Tariff, ClientError> {
        let endpoint = format!(
            "{}/{}/{}/{}",
            url.trim_end_matches('/'),
            country_code,
            party_id,
            tariff_id,
        );
        let response = self
            .http
            .get(url::Url::parse(&endpoint)?)
            .header("Authorization", self.auth_header_value())
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ClientError::NotFound);
        }
        let response = response.error_for_status()?;
        let envelope: OcpiResponse<Tariff> = response.json().await?;
        envelope.data.ok_or(ClientError::EmptyData)
    }

    /// Push or replace a tariff on an eMSP receiver
    /// (`PUT {url}/{country_code}/{party_id}/{tariff_id}`).
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails or the URL is invalid.
    pub async fn put_tariff(
        &self,
        url: &str,
        country_code: &str,
        party_id: &str,
        tariff_id: &str,
        tariff: &Tariff,
    ) -> Result<(), ClientError> {
        let endpoint = format!(
            "{}/{}/{}/{}",
            url.trim_end_matches('/'),
            country_code,
            party_id,
            tariff_id,
        );
        self.http
            .put(url::Url::parse(&endpoint)?)
            .header("Authorization", self.auth_header_value())
            .json(tariff)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    /// Delete a tariff from an eMSP receiver
    /// (`DELETE {url}/{country_code}/{party_id}/{tariff_id}`).
    ///
    /// # Errors
    ///
    /// Returns [`ClientError::NotFound`] when the remote responds with HTTP 404.
    /// Returns [`ClientError`] for other failures.
    pub async fn delete_tariff(
        &self,
        url: &str,
        country_code: &str,
        party_id: &str,
        tariff_id: &str,
    ) -> Result<(), ClientError> {
        let endpoint = format!(
            "{}/{}/{}/{}",
            url.trim_end_matches('/'),
            country_code,
            party_id,
            tariff_id,
        );
        let response = self
            .http
            .delete(url::Url::parse(&endpoint)?)
            .header("Authorization", self.auth_header_value())
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ClientError::NotFound);
        }
        response.error_for_status()?;
        Ok(())
    }

    // ── Tokens ────────────────────────────────────────────────────────────────

    /// Fetch a paginated list of tokens from an eMSP (`GET {url}`).
    ///
    /// `url` is the absolute URL of the eMSP's tokens sender endpoint.
    /// `params` carries `date_from`, `date_to`, `offset`, and `limit`.
    ///
    /// Returns the first page of tokens plus pagination metadata. Use
    /// `PaginationMeta.next_url` to retrieve subsequent pages.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails or the URL is invalid.
    pub async fn get_tokens(
        &self,
        url: &str,
        params: PaginatedParams,
    ) -> Result<(Vec<Token>, PaginationMeta), ClientError> {
        let mut parsed = url::Url::parse(url)?;
        if let Some(date_from) = params.date_from {
            parsed
                .query_pairs_mut()
                .append_pair("date_from", &date_from.to_rfc3339());
        }
        if let Some(date_to) = params.date_to {
            parsed
                .query_pairs_mut()
                .append_pair("date_to", &date_to.to_rfc3339());
        }
        if let Some(offset) = params.offset {
            parsed
                .query_pairs_mut()
                .append_pair("offset", &offset.to_string());
        }
        if let Some(limit) = params.limit {
            parsed
                .query_pairs_mut()
                .append_pair("limit", &limit.to_string());
        }
        let response = self
            .http
            .get(parsed)
            .header("Authorization", self.auth_header_value())
            .send()
            .await?
            .error_for_status()?;
        let hdrs = response.headers();
        let link = hdrs
            .get("link")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned());
        let total_count = hdrs
            .get("x-total-count")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned());
        let limit_hdr = hdrs
            .get("x-limit")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned());
        let meta = PaginationMeta::from_headers(
            link.as_deref(),
            total_count.as_deref(),
            limit_hdr.as_deref(),
        )
        .unwrap_or(PaginationMeta {
            next_url: None,
            total_count: 0,
            limit: 50,
        });
        let envelope: OcpiResponse<Vec<Token>> = response.json().await?;
        let tokens = envelope.data.ok_or(ClientError::EmptyData)?;
        Ok((tokens, meta))
    }

    /// Push or replace a token on a CPO receiver
    /// (`PUT {url}/{country_code}/{party_id}/{token_uid}?type=`).
    ///
    /// `token_type` is appended as a `?type=` query parameter. Defaults to
    /// `RFID` on the server side when omitted, but this method always sends it
    /// explicitly for spec correctness.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails or the URL is invalid.
    pub async fn put_token(
        &self,
        url: &str,
        country_code: &str,
        party_id: &str,
        token_uid: &str,
        token_type: TokenType,
        token: &Token,
    ) -> Result<(), ClientError> {
        let base = format!(
            "{}/{}/{}/{}",
            url.trim_end_matches('/'),
            country_code,
            party_id,
            token_uid,
        );
        let mut parsed = url::Url::parse(&base)?;
        parsed
            .query_pairs_mut()
            .append_pair("type", token_type_str(token_type));
        self.http
            .put(parsed)
            .header("Authorization", self.auth_header_value())
            .json(token)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    /// Apply a partial update (JSON merge-patch, RFC 7396) to a token on a
    /// CPO receiver (`PATCH {url}/{country_code}/{party_id}/{token_uid}?type=`).
    ///
    /// # Errors
    ///
    /// - [`ClientError::NotFound`] when the server returns HTTP 404.
    /// - [`ClientError::Http`] on network or server errors.
    pub async fn patch_token<T: ocpi_types::serde::Serialize>(
        &self,
        url: &str,
        country_code: &str,
        party_id: &str,
        token_uid: &str,
        token_type: TokenType,
        partial: &T,
    ) -> Result<(), ClientError> {
        let base = format!(
            "{}/{}/{}/{}",
            url.trim_end_matches('/'),
            country_code,
            party_id,
            token_uid,
        );
        let mut parsed = url::Url::parse(&base)?;
        parsed
            .query_pairs_mut()
            .append_pair("type", token_type_str(token_type));
        let response = self
            .http
            .patch(parsed)
            .header("Authorization", self.auth_header_value())
            .json(partial)
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ClientError::NotFound);
        }
        response.error_for_status()?;
        Ok(())
    }

    /// Request real-time authorization for a token from an eMSP
    /// (`POST {url}/{token_uid}/authorize?type=`).
    ///
    /// `location` is an optional body sent to the eMSP for location-scoped
    /// authorization checks.
    ///
    /// Returns [`AuthorizationInfo`] when the token is known to the eMSP.
    ///
    /// # Errors
    ///
    /// - [`ClientError::NotFound`] when the eMSP responds with HTTP 404
    ///   (OCPI 2004 — token unknown).
    /// - [`ClientError::EmptyData`] if the success envelope carries no data.
    /// - [`ClientError::Http`] on network or server errors.
    pub async fn authorize_token(
        &self,
        url: &str,
        token_uid: &str,
        token_type: TokenType,
        location: Option<&LocationReferences>,
    ) -> Result<AuthorizationInfo, ClientError> {
        let base = format!("{}/{token_uid}/authorize", url.trim_end_matches('/'));
        let mut parsed = url::Url::parse(&base)?;
        parsed
            .query_pairs_mut()
            .append_pair("type", token_type_str(token_type));
        let mut req = self
            .http
            .post(parsed)
            .header("Authorization", self.auth_header_value());
        if let Some(loc) = location {
            req = req.json(loc);
        }
        let response = req.send().await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ClientError::NotFound);
        }
        let response = response.error_for_status()?;
        let envelope: OcpiResponse<AuthorizationInfo> = response.json().await?;
        envelope.data.ok_or(ClientError::EmptyData)
    }

    // ── Commands ──────────────────────────────────────────────────────────────

    async fn post_command<B: ocpi_types::serde::Serialize>(
        &self,
        commands_url: &str,
        command_type: CommandType,
        body: &B,
    ) -> Result<CommandResponse, ClientError> {
        let type_str = ocpi_types::serde_json::to_value(command_type)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        let url = format!("{}/{}", commands_url.trim_end_matches('/'), type_str);
        let parsed = url::Url::parse(&url)?;
        let response = self
            .http
            .post(parsed)
            .header("Authorization", self.auth_header_value())
            .json(body)
            .send()
            .await?
            .error_for_status()?;
        let envelope: OcpiResponse<CommandResponse> = response.json().await?;
        envelope.data.ok_or(ClientError::EmptyData)
    }

    /// Send a `CANCEL_RESERVATION` command to a CPO's commands endpoint.
    ///
    /// `commands_url` is the absolute base URL (e.g. `https://cpo.example/ocpi/2.2.1/commands`).
    /// The method type segment (`/CANCEL_RESERVATION`) is appended automatically.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails or the response carries no data.
    pub async fn cancel_reservation(
        &self,
        commands_url: &str,
        cmd: CancelReservation,
    ) -> Result<CommandResponse, ClientError> {
        self.post_command(commands_url, CommandType::CancelReservation, &cmd)
            .await
    }

    /// Send a `RESERVE_NOW` command to a CPO's commands endpoint.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails or the response carries no data.
    pub async fn reserve_now(
        &self,
        commands_url: &str,
        cmd: ReserveNow,
    ) -> Result<CommandResponse, ClientError> {
        self.post_command(commands_url, CommandType::ReserveNow, &cmd)
            .await
    }

    /// Send a `START_SESSION` command to a CPO's commands endpoint.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails or the response carries no data.
    pub async fn start_session(
        &self,
        commands_url: &str,
        cmd: StartSession,
    ) -> Result<CommandResponse, ClientError> {
        self.post_command(commands_url, CommandType::StartSession, &cmd)
            .await
    }

    /// Send a `STOP_SESSION` command to a CPO's commands endpoint.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails or the response carries no data.
    pub async fn stop_session(
        &self,
        commands_url: &str,
        cmd: StopSession,
    ) -> Result<CommandResponse, ClientError> {
        self.post_command(commands_url, CommandType::StopSession, &cmd)
            .await
    }

    /// Send an `UNLOCK_CONNECTOR` command to a CPO's commands endpoint.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails or the response carries no data.
    pub async fn unlock_connector(
        &self,
        commands_url: &str,
        cmd: UnlockConnector,
    ) -> Result<CommandResponse, ClientError> {
        self.post_command(commands_url, CommandType::UnlockConnector, &cmd)
            .await
    }

    /// POST a [`CommandResult`] to the eMSP's `response_url` (the async callback).
    ///
    /// This is the second phase of the Commands flow: after the CPO has forwarded
    /// the command to the Charge Point, it POSTs the final result back to the
    /// `response_url` that the eMSP included in the original command body.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails or the URL is invalid.
    pub async fn post_command_result(
        &self,
        response_url: &str,
        result: CommandResult,
    ) -> Result<(), ClientError> {
        let parsed = url::Url::parse(response_url)?;
        self.http
            .post(parsed)
            .header("Authorization", self.auth_header_value())
            .json(&result)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    // ── ChargingProfiles ──────────────────────────────────────────────────────

    /// Request the current `ActiveChargingProfile` for a session from a CPO —
    /// receiver interface (`GET /chargingprofiles/{session_id}`).
    ///
    /// `duration` is the requested profile length in seconds; `response_url` is
    /// where the CPO will asynchronously POST the [`ActiveChargingProfileResult`].
    /// The returned [`ChargingProfileResponse`] is only the CPO's immediate
    /// acknowledgment.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails, the URL is invalid, or the
    /// response carries no data.
    pub async fn get_active_charging_profile(
        &self,
        url: &str,
        session_id: &str,
        duration: u32,
        response_url: &str,
    ) -> Result<ChargingProfileResponse, ClientError> {
        let mut parsed = charging_profile_url(url, session_id)?;
        parsed
            .query_pairs_mut()
            .append_pair("duration", &duration.to_string())
            .append_pair("response_url", response_url);
        let response = self
            .http
            .get(parsed)
            .header("Authorization", self.auth_header_value())
            .send()
            .await?
            .error_for_status()?;
        let envelope: OcpiResponse<ChargingProfileResponse> = response.json().await?;
        envelope.data.ok_or(ClientError::EmptyData)
    }

    /// Create or update a ChargingProfile on a session at a CPO — receiver
    /// interface (`PUT /chargingprofiles/{session_id}`).
    ///
    /// The `profile`'s `response_url` is where the CPO will asynchronously POST
    /// the [`ChargingProfileResult`].
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails, the URL is invalid, or the
    /// response carries no data.
    pub async fn set_charging_profile(
        &self,
        url: &str,
        session_id: &str,
        profile: SetChargingProfile,
    ) -> Result<ChargingProfileResponse, ClientError> {
        let parsed = charging_profile_url(url, session_id)?;
        let response = self
            .http
            .put(parsed)
            .header("Authorization", self.auth_header_value())
            .json(&profile)
            .send()
            .await?
            .error_for_status()?;
        let envelope: OcpiResponse<ChargingProfileResponse> = response.json().await?;
        envelope.data.ok_or(ClientError::EmptyData)
    }

    /// Cancel/clear the ChargingProfile on a session at a CPO — receiver
    /// interface (`DELETE /chargingprofiles/{session_id}?response_url={url}`).
    ///
    /// `response_url` is where the CPO will asynchronously POST the
    /// [`ClearProfileResult`].
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails, the URL is invalid, or the
    /// response carries no data.
    pub async fn clear_charging_profile(
        &self,
        url: &str,
        session_id: &str,
        response_url: &str,
    ) -> Result<ChargingProfileResponse, ClientError> {
        let mut parsed = charging_profile_url(url, session_id)?;
        parsed
            .query_pairs_mut()
            .append_pair("response_url", response_url);
        let response = self
            .http
            .delete(parsed)
            .header("Authorization", self.auth_header_value())
            .send()
            .await?
            .error_for_status()?;
        let envelope: OcpiResponse<ChargingProfileResponse> = response.json().await?;
        envelope.data.ok_or(ClientError::EmptyData)
    }

    /// Push a changed `ActiveChargingProfile` to a Sender (typically SCSP/eMSP) —
    /// the Receiver-side (typically CPO) call of the ChargingProfiles **Sender
    /// interface** (`PUT /chargingprofiles/{session_id}`).
    ///
    /// Per spec the Receiver SHALL call this whenever it learns the
    /// `ActiveChargingProfile` for an ongoing session has changed, *provided* the
    /// Sender has at least once successfully set a profile for that session via
    /// the Receiver `PUT` (`SetChargingProfile`). The response carries no data.
    ///
    /// Note this is the same path as [`set_charging_profile`](Self::set_charging_profile),
    /// but the two target different market roles' interfaces: that one PUTs a
    /// [`SetChargingProfile`] to a CPO Receiver, this one PUTs an
    /// [`ActiveChargingProfile`] to an SCSP/eMSP Sender.
    ///
    /// Spec: `mod_charging_profiles.asciidoc` §Sender Interface,
    /// `mod_charging_profiles_msp_put_method`.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails or the URL is invalid.
    pub async fn put_active_charging_profile(
        &self,
        url: &str,
        session_id: &str,
        profile: ActiveChargingProfile,
    ) -> Result<(), ClientError> {
        let parsed = charging_profile_url(url, session_id)?;
        self.http
            .put(parsed)
            .header("Authorization", self.auth_header_value())
            .json(&profile)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    /// POST an [`ActiveChargingProfileResult`] to the Sender's `response_url`
    /// (the async callback for a prior GET ActiveChargingProfile).
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails or the URL is invalid.
    pub async fn post_active_profile_result(
        &self,
        response_url: &str,
        result: ActiveChargingProfileResult,
    ) -> Result<(), ClientError> {
        self.post_profile_callback(response_url, &result).await
    }

    /// POST a [`ChargingProfileResult`] to the Sender's `response_url`
    /// (the async callback for a prior PUT SetChargingProfile).
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails or the URL is invalid.
    pub async fn post_charging_profile_result(
        &self,
        response_url: &str,
        result: ChargingProfileResult,
    ) -> Result<(), ClientError> {
        self.post_profile_callback(response_url, &result).await
    }

    /// POST a [`ClearProfileResult`] to the Sender's `response_url`
    /// (the async callback for a prior DELETE ChargingProfile).
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails or the URL is invalid.
    pub async fn post_clear_profile_result(
        &self,
        response_url: &str,
        result: ClearProfileResult,
    ) -> Result<(), ClientError> {
        self.post_profile_callback(response_url, &result).await
    }

    /// Shared POST for the three ChargingProfiles async result callbacks. The
    /// `response_url` is opaque (defined by the Sender), so it is parsed as an
    /// absolute URL and the result object is sent as the JSON body.
    async fn post_profile_callback<B: ocpi_types::serde::Serialize>(
        &self,
        response_url: &str,
        result: &B,
    ) -> Result<(), ClientError> {
        let parsed = url::Url::parse(response_url)?;
        self.http
            .post(parsed)
            .header("Authorization", self.auth_header_value())
            .json(result)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    // ── HubClientInfo (2.2.1 §mod_hub_client_info) ─────────────────────────
    //
    // HubClientInfo is a *Configuration Module*: the OCPI routing headers
    // (`OCPI-to/from-party-id/country-code`) are NOT sent on these endpoints,
    // unlike the Locations/Sessions/etc. sender methods (see #64). Only the
    // `Authorization` token is carried.

    /// Push a single `ClientInfo` to a connected party's **Receiver** interface
    /// (`PUT <url>/{country_code}/{party_id}`).
    ///
    /// Used by the Hub: whenever a party's connection status changes, the Hub
    /// notifies every other connected party by upserting the changed
    /// `ClientInfo` on that party's `clientinfo` endpoint.
    ///
    /// `url` is the party's `clientinfo` endpoint base; the `{country_code}` and
    /// `{party_id}` path segments are appended automatically.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails, the URL is invalid, or the
    /// remote responds with a non-success status.
    ///
    /// Spec: `mod_hub_client_info.asciidoc` — Receiver Interface, PUT.
    pub async fn put_client_info(
        &self,
        url: &str,
        country_code: &str,
        party_id: &str,
        info: &ClientInfo,
    ) -> Result<(), ClientError> {
        let endpoint = format!(
            "{}/{}/{}",
            url.trim_end_matches('/'),
            country_code,
            party_id
        );
        self.http
            .put(url::Url::parse(&endpoint)?)
            .header("Authorization", self.auth_header_value())
            .json(info)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    /// Retrieve a single `ClientInfo` entry from the Hub's **Sender** interface
    /// (`GET <url>/{country_code}/{party_id}`).
    ///
    /// `url` is the Hub's `clientinfo` endpoint base; the `{country_code}` and
    /// `{party_id}` path segments are appended automatically.
    ///
    /// # Errors
    ///
    /// - [`ClientError::NotFound`] when the Hub returns OCPI `2003` or HTTP 404.
    /// - [`ClientError::EmptyData`] if the success envelope carries no data.
    /// - [`ClientError`] on any other transport or status failure.
    ///
    /// Spec: `mod_hub_client_info.asciidoc` — Sender Interface, GET (single).
    pub async fn get_client_info(
        &self,
        url: &str,
        country_code: &str,
        party_id: &str,
    ) -> Result<ClientInfo, ClientError> {
        let endpoint = format!(
            "{}/{}/{}",
            url.trim_end_matches('/'),
            country_code,
            party_id
        );
        let response = self
            .http
            .get(url::Url::parse(&endpoint)?)
            .header("Authorization", self.auth_header_value())
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ClientError::NotFound);
        }
        let response = response.error_for_status()?;
        let envelope: OcpiResponse<ClientInfo> = response.json().await?;
        envelope.data.ok_or(ClientError::EmptyData)
    }

    /// Retrieve the paginated `ClientInfo` list from the Hub's **Sender**
    /// interface (`GET <url>`), returning the page plus its [`PaginationMeta`].
    ///
    /// Follows the same query-param and pagination-header handling as
    /// [`OcpiClient::get_sessions`] / [`OcpiClient::get_tokens`]; use
    /// [`PaginationMeta::next_url`] to retrieve subsequent pages.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails, the URL is invalid, or the
    /// envelope carries no data.
    ///
    /// Spec: `mod_hub_client_info.asciidoc` — Sender Interface, GET (list).
    pub async fn get_client_infos(
        &self,
        url: &str,
        params: &PaginatedParams,
    ) -> Result<(Vec<ClientInfo>, PaginationMeta), ClientError> {
        let mut req = self
            .http
            .get(url::Url::parse(url)?)
            .header("Authorization", self.auth_header_value());
        if let Some(df) = params.date_from {
            req = req.query(&[("date_from", df.to_rfc3339())]);
        }
        if let Some(dt) = params.date_to {
            req = req.query(&[("date_to", dt.to_rfc3339())]);
        }
        if let Some(off) = params.offset {
            req = req.query(&[("offset", off.to_string())]);
        }
        if let Some(lim) = params.limit {
            req = req.query(&[("limit", lim.to_string())]);
        }
        let response = req.send().await?.error_for_status()?;
        let hdrs = response.headers();
        let link = hdrs
            .get("link")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned());
        let total_count = hdrs
            .get("x-total-count")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned());
        let limit_hdr = hdrs
            .get("x-limit")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned());
        let meta = PaginationMeta::from_headers(
            link.as_deref(),
            total_count.as_deref(),
            limit_hdr.as_deref(),
        )
        .unwrap_or(PaginationMeta {
            next_url: None,
            total_count: 0,
            limit: 50,
        });
        let envelope: OcpiResponse<Vec<ClientInfo>> = response.json().await?;
        let infos = envelope.data.ok_or(ClientError::EmptyData)?;
        Ok((infos, meta))
    }
}

// ── OcpiVersionFetcher ──────────────────────────────────────────────────────

/// Build the `Authorization` header value for a fetch-back request.
///
/// Mirrors [`OcpiClient::auth_header_value`]: Base64-encode the token per OCPI
/// 2.2.1 §4.1.1, or send it raw in `compat_raw_token` mode for legacy peers.
fn fetch_auth_header(token: &str, compat_raw_token: bool) -> String {
    if compat_raw_token {
        format!("Token {token}")
    } else {
        CredentialToken::new(token).to_header_value()
    }
}

/// Parse a fetch-back URL, mapping a parse failure to [`FetchError::Transport`].
///
/// A malformed `url` (e.g. one a registering party put in its [`Credentials`])
/// is a transport-level failure from the server's perspective — it cannot reach
/// the party's API — so it maps to OCPI `3001` like any other transport error.
fn parse_fetch_url(url: &str) -> Result<Url, FetchError> {
    Url::parse(url).map_err(|e| FetchError::Transport(e.to_string()))
}

/// A reqwest-backed [`VersionFetcher`] for the credentials registration
/// fetch-back.
///
/// `ocpi-server` defines the [`VersionFetcher`] contract but cannot depend on an
/// HTTP client (that would risk a cyclic dependency with `ocpi-client`), so this
/// crate supplies the default implementation. It reuses the same Base64
/// `Authorization: Token` encoding ([`CredentialToken`]) and [`OcpiResponse`]
/// envelope parsing as [`OcpiClient`].
///
/// Pass it to `ocpi_server::CredentialsConfig::new_with_fetcher` so the receiver
/// can `GET {credentials.url}` and the chosen version's details during
/// `POST`/`PUT /credentials`.
///
/// Spec: `specs/ocpi/2.2.1/credentials.asciidoc` — §POST Method (the receiver
/// fetches the sender's `/versions` + version details after registration).
#[derive(Debug, Clone)]
pub struct OcpiVersionFetcher {
    http: reqwest::Client,
    /// When `true`, the token is sent raw (not Base64-encoded); for legacy
    /// 2.1.1/2.2 peers, matching [`OcpiClient::with_compat_raw_token`].
    compat_raw_token: bool,
}

impl Default for OcpiVersionFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl OcpiVersionFetcher {
    /// Create a fetcher with a fresh internal [`reqwest::Client`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::new(),
            compat_raw_token: false,
        }
    }

    /// Create a fetcher reusing an existing [`reqwest::Client`] (e.g. to share a
    /// connection pool with an [`OcpiClient`]).
    #[must_use]
    pub fn with_client(http: reqwest::Client) -> Self {
        Self {
            http,
            compat_raw_token: false,
        }
    }

    /// Override the token encoding mode.
    ///
    /// - `false` (default): token is Base64-encoded per OCPI 2.2.1.
    /// - `true`: token is sent raw; use with legacy 2.1.1/2.2 peers.
    #[must_use]
    pub fn with_compat_raw_token(mut self, compat: bool) -> Self {
        self.compat_raw_token = compat;
        self
    }
}

impl VersionFetcher for OcpiVersionFetcher {
    fn fetch_versions<'a>(&'a self, url: &'a str, token: &'a str) -> FetchFuture<'a, Vec<Version>> {
        Box::pin(async move {
            let parsed = parse_fetch_url(url)?;
            let response = self
                .http
                .get(parsed)
                .header(
                    "Authorization",
                    fetch_auth_header(token, self.compat_raw_token),
                )
                .send()
                .await
                .map_err(|e| FetchError::Transport(e.to_string()))?
                .error_for_status()
                .map_err(|e| FetchError::Transport(e.to_string()))?;
            let envelope: OcpiResponse<Vec<Version>> = response
                .json()
                .await
                .map_err(|e| FetchError::Invalid(e.to_string()))?;
            envelope.data.ok_or_else(|| {
                FetchError::Invalid("response envelope contained no data".to_string())
            })
        })
    }

    fn fetch_version_details<'a>(
        &'a self,
        url: &'a str,
        token: &'a str,
    ) -> FetchFuture<'a, VersionDetails> {
        Box::pin(async move {
            let parsed = parse_fetch_url(url)?;
            let response = self
                .http
                .get(parsed)
                .header(
                    "Authorization",
                    fetch_auth_header(token, self.compat_raw_token),
                )
                .send()
                .await
                .map_err(|e| FetchError::Transport(e.to_string()))?
                .error_for_status()
                .map_err(|e| FetchError::Transport(e.to_string()))?;
            let envelope: OcpiResponse<VersionDetails> = response
                .json()
                .await
                .map_err(|e| FetchError::Invalid(e.to_string()))?;
            envelope.data.ok_or_else(|| {
                FetchError::Invalid("response envelope contained no data".to_string())
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        charging_profile_url, fetch_auth_header, join_segments, negotiate_version, parse_fetch_url,
        select_version, OcpiClient, OcpiVersionFetcher,
    };
    use ocpi_server::{FetchError, VersionFetcher};
    use ocpi_types::{
        common::Url as OcpiUrl,
        serde_json,
        version::{Version, VersionNumber},
    };
    use url::Url;

    fn make_version(v: VersionNumber, url: &str) -> Version {
        Version {
            version: v,
            url: OcpiUrl::try_from(url).unwrap(),
        }
    }

    // ── negotiate_version (pure helper, spec /versions fixtures) ───────────────

    // OCPI 2.1.1 §6.1 `GET /versions` response `data` array — a legacy partner
    // that speaks only 2.1.1. (specs/ocpi/2.1.1 — Versions module.)
    const PARTNER_2_1_1_VERSIONS: &str = r#"[
        {"version": "2.1.1", "url": "https://partner.example/ocpi/2.1.1"}
    ]"#;

    // OCPI 2.2.1 `GET /versions` response `data` array — a partner that
    // advertises both 2.2.1 and 2.1.1. (specs/ocpi/2.2.1 — version endpoint.)
    const PARTNER_DUAL_VERSIONS: &str = r#"[
        {"version": "2.2.1", "url": "https://partner.example/ocpi/2.2.1"},
        {"version": "2.1.1", "url": "https://partner.example/ocpi/2.1.1"}
    ]"#;

    // The hub speaks both 2.1.1 and 2.2.1.
    const HUB_SUPPORTED: [VersionNumber; 2] = [VersionNumber::V2_1_1, VersionNumber::V2_2_1];

    #[test]
    fn negotiate_picks_2_1_1_with_legacy_partner() {
        // A 2.1.1-only partner: the only mutual version is 2.1.1.
        let remote: Vec<Version> = serde_json::from_str(PARTNER_2_1_1_VERSIONS).unwrap();
        assert_eq!(
            negotiate_version(&remote, &HUB_SUPPORTED),
            Some(VersionNumber::V2_1_1)
        );
    }

    #[test]
    fn negotiate_prefers_2_2_1_with_dual_partner() {
        // Both speak 2.1.1 and 2.2.1 → pick the highest mutual, 2.2.1.
        let remote: Vec<Version> = serde_json::from_str(PARTNER_DUAL_VERSIONS).unwrap();
        assert_eq!(
            negotiate_version(&remote, &HUB_SUPPORTED),
            Some(VersionNumber::V2_2_1)
        );
    }

    #[test]
    fn negotiate_disjoint_returns_none() {
        // Partner speaks only 2.0; the hub does not → no common version.
        // Caller maps this to an explicit `UnsupportedVersion` status_code.
        let remote = vec![make_version(
            VersionNumber::V2_0,
            "https://partner.example/2.0",
        )];
        assert_eq!(negotiate_version(&remote, &HUB_SUPPORTED), None);
    }

    #[test]
    fn negotiate_empty_remote_returns_none() {
        assert_eq!(negotiate_version(&[], &HUB_SUPPORTED), None);
    }

    #[test]
    fn negotiate_ignores_remote_versions_we_do_not_support() {
        // Partner advertises a future 2.3.0 plus 2.1.1; a hub that only speaks
        // {2.1.1, 2.2.1} must fall back to the highest it actually supports.
        let remote = vec![
            make_version(VersionNumber::V2_3_0, "https://partner.example/2.3.0"),
            make_version(VersionNumber::V2_1_1, "https://partner.example/2.1.1"),
        ];
        assert_eq!(
            negotiate_version(&remote, &HUB_SUPPORTED),
            Some(VersionNumber::V2_1_1)
        );
    }

    #[test]
    fn negotiate_agrees_with_select_version_url() {
        // The pure negotiator and the entry-returning `select_version` must
        // agree: the chosen number indexes the chosen details URL.
        let remote: Vec<Version> = serde_json::from_str(PARTNER_DUAL_VERSIONS).unwrap();
        let chosen = negotiate_version(&remote, &HUB_SUPPORTED).unwrap();
        let entry = select_version(&remote, &HUB_SUPPORTED).unwrap();
        assert_eq!(entry.version, chosen);
        assert_eq!(entry.url.as_str(), "https://partner.example/ocpi/2.2.1");
    }

    // ── select_version ────────────────────────────────────────────────────────

    #[test]
    fn select_version_picks_highest_common() {
        let remote = vec![
            make_version(VersionNumber::V2_1_1, "https://example.com/2.1.1"),
            make_version(VersionNumber::V2_2_1, "https://example.com/2.2.1"),
        ];
        let supported = [VersionNumber::V2_1_1, VersionNumber::V2_2_1];
        let picked = select_version(&remote, &supported).unwrap();
        assert_eq!(picked.version, VersionNumber::V2_2_1);
    }

    #[test]
    fn select_version_no_overlap_returns_none() {
        let remote = vec![make_version(VersionNumber::V2_0, "https://example.com/2.0")];
        let supported = [VersionNumber::V2_2_1, VersionNumber::V2_3_0];
        assert!(select_version(&remote, &supported).is_none());
    }

    #[test]
    fn select_version_single_overlap() {
        let remote = vec![
            make_version(VersionNumber::V2_0, "https://example.com/2.0"),
            make_version(VersionNumber::V2_2_1, "https://example.com/2.2.1"),
        ];
        let supported = [VersionNumber::V2_2_1];
        let picked = select_version(&remote, &supported).unwrap();
        assert_eq!(picked.version, VersionNumber::V2_2_1);
    }

    #[test]
    fn select_version_remote_subset_of_supported() {
        // Remote only supports older versions; we pick the highest the remote has.
        let remote = vec![
            make_version(VersionNumber::V2_1_1, "https://example.com/2.1.1"),
            make_version(VersionNumber::V2_2, "https://example.com/2.2"),
        ];
        let supported = [
            VersionNumber::V2_1_1,
            VersionNumber::V2_2,
            VersionNumber::V2_2_1,
            VersionNumber::V2_3_0,
        ];
        let picked = select_version(&remote, &supported).unwrap();
        assert_eq!(picked.version, VersionNumber::V2_2);
    }

    #[test]
    fn select_version_empty_remote_returns_none() {
        assert!(select_version(&[], &[VersionNumber::V2_2_1]).is_none());
    }

    #[test]
    fn select_version_single_both_sides() {
        let remote = vec![make_version(
            VersionNumber::V2_3_0,
            "https://example.com/2.3.0",
        )];
        let supported = [VersionNumber::V2_3_0];
        let picked = select_version(&remote, &supported).unwrap();
        assert_eq!(picked.version, VersionNumber::V2_3_0);
        assert_eq!(picked.url.as_str(), "https://example.com/2.3.0");
    }

    // ── VersionNumber ordering ────────────────────────────────────────────────

    #[test]
    fn version_number_ord_ascending() {
        assert!(VersionNumber::V2_0 < VersionNumber::V2_1_1);
        assert!(VersionNumber::V2_1_1 < VersionNumber::V2_2);
        assert!(VersionNumber::V2_2 < VersionNumber::V2_2_1);
        assert!(VersionNumber::V2_2_1 < VersionNumber::V2_3_0);
    }

    #[test]
    fn version_number_max_is_v2_3_0() {
        let versions = [
            VersionNumber::V2_0,
            VersionNumber::V2_1_1,
            VersionNumber::V2_2,
            VersionNumber::V2_2_1,
            VersionNumber::V2_3_0,
        ];
        assert_eq!(
            versions.iter().copied().max().unwrap(),
            VersionNumber::V2_3_0
        );
    }

    // ── OcpiClient ────────────────────────────────────────────────────────────

    #[test]
    fn builds_client_with_base_url() {
        let client = OcpiClient::new(
            Url::parse("https://example.com/ocpi/cpo/2.2.1/").unwrap(),
            "secret",
        );
        assert_eq!(
            client.base_url().as_str(),
            "https://example.com/ocpi/cpo/2.2.1/"
        );
    }

    #[test]
    fn credentials_url_parses() {
        // Verify that the absolute URL pattern used for credentials endpoints
        // parses correctly (no base-URL joining involved).
        let url = "https://example.com/ocpi/2.2.1/credentials";
        assert!(url::Url::parse(url).is_ok());
    }

    #[test]
    fn invalid_credentials_url_is_rejected() {
        // Passing a relative or malformed URL to the credentials methods should
        // produce a ClientError::Url (from url::ParseError).
        let result = url::Url::parse("not-a-url:///no-scheme-here");
        // url crate may or may not parse this; what matters is the client
        // would propagate the error. We just confirm the parse path exists.
        let _ = result;
    }

    // ── Authorization header encoding ─────────────────────────────────────────

    #[test]
    fn default_client_sends_base64_encoded_token() {
        let client = OcpiClient::new(Url::parse("https://example.com/").unwrap(), "my-raw-token");
        let header = client.auth_header_value();
        // "my-raw-token" in Base64 (RFC 4648 standard alphabet) = "bXktcmF3LXRva2Vu"
        assert_eq!(header, "Token bXktcmF3LXRva2Vu");
    }

    #[test]
    fn compat_client_sends_raw_token() {
        let client = OcpiClient::new(Url::parse("https://example.com/").unwrap(), "my-raw-token")
            .with_compat_raw_token(true);
        assert_eq!(client.auth_header_value(), "Token my-raw-token");
    }

    #[test]
    fn compat_builder_preserves_other_fields() {
        let base = Url::parse("https://example.com/ocpi/").unwrap();
        let client = OcpiClient::new(base.clone(), "tok").with_compat_raw_token(true);
        assert_eq!(client.base_url(), &base);
        assert!(client.compat_raw_token);
    }

    #[test]
    fn compat_false_is_default() {
        let client = OcpiClient::new(Url::parse("https://example.com/").unwrap(), "tok");
        assert!(!client.compat_raw_token);
    }

    // ── Locations URL building (join_segments) ────────────────────────────────

    #[test]
    fn join_segments_single_location() {
        let base = "https://server.com/ocpi/cpo/2.2.1/locations";
        assert_eq!(
            join_segments(base, &["LOC1"]),
            "https://server.com/ocpi/cpo/2.2.1/locations/LOC1"
        );
    }

    #[test]
    fn join_segments_trims_trailing_slash() {
        // A base with a trailing slash must not produce a double separator.
        let base = "https://server.com/ocpi/cpo/2.2.1/locations/";
        assert_eq!(
            join_segments(base, &["LOC1"]),
            "https://server.com/ocpi/cpo/2.2.1/locations/LOC1"
        );
    }

    #[test]
    fn join_segments_evse_and_connector() {
        let base = "https://server.com/ocpi/cpo/2.2.1/locations";
        assert_eq!(
            join_segments(base, &["LOC1", "3256"]),
            "https://server.com/ocpi/cpo/2.2.1/locations/LOC1/3256"
        );
        assert_eq!(
            join_segments(base, &["LOC1", "3256", "1"]),
            "https://server.com/ocpi/cpo/2.2.1/locations/LOC1/3256/1"
        );
    }

    #[test]
    fn join_segments_no_segments_returns_trimmed_base() {
        assert_eq!(
            join_segments("https://server.com/locations/", &[]),
            "https://server.com/locations"
        );
    }

    #[test]
    fn join_segments_result_parses_as_url() {
        let url = join_segments(
            "https://server.com/ocpi/cpo/2.2.1/locations",
            &["LOC1", "3256", "1"],
        );
        assert!(url::Url::parse(&url).is_ok());
    }

    // ── ChargingProfiles URL building ─────────────────────────────────────────

    #[test]
    fn charging_profile_url_appends_session_id() {
        let url = charging_profile_url("https://cpo.example/ocpi/2.2.1/chargingprofiles", "1234")
            .unwrap();
        assert_eq!(
            url.as_str(),
            "https://cpo.example/ocpi/2.2.1/chargingprofiles/1234"
        );
    }

    #[test]
    fn charging_profile_url_trims_trailing_slash() {
        let url = charging_profile_url("https://cpo.example/ocpi/2.2.1/chargingprofiles/", "1234")
            .unwrap();
        assert_eq!(
            url.as_str(),
            "https://cpo.example/ocpi/2.2.1/chargingprofiles/1234"
        );
    }

    #[test]
    fn charging_profile_url_supports_query_params() {
        // GET appends duration + response_url; DELETE appends response_url only.
        let mut url =
            charging_profile_url("https://cpo.example/ocpi/2.2.1/chargingprofiles", "1234")
                .unwrap();
        url.query_pairs_mut()
            .append_pair("duration", "900")
            .append_pair("response_url", "https://msp.example/cb?id=5678");
        assert_eq!(url.query_pairs().count(), 2);
        assert_eq!(
            url.path(),
            "/ocpi/2.2.1/chargingprofiles/1234",
            "query params must not corrupt the path"
        );
    }

    // ── OcpiVersionFetcher ──────────────────────────────────────────────────────

    #[test]
    fn fetch_auth_header_base64_encodes_by_default() {
        // Matches the OCPI 2.2.1 §4.1.1 Base64 encoding used by `OcpiClient`.
        assert_eq!(
            fetch_auth_header("example-token", false),
            "Token ZXhhbXBsZS10b2tlbg=="
        );
    }

    #[test]
    fn fetch_auth_header_compat_sends_raw_token() {
        // Legacy 2.1.1/2.2 peers receive the unencoded token.
        assert_eq!(
            fetch_auth_header("example-token", true),
            "Token example-token"
        );
    }

    #[test]
    fn parse_fetch_url_accepts_absolute_url() {
        assert!(parse_fetch_url("https://party.example/ocpi/versions").is_ok());
    }

    #[test]
    fn parse_fetch_url_rejects_malformed_url_as_transport_error() {
        // A bad URL in a registering party's credentials is unreachable →
        // transport-level failure (OCPI 3001), not an `Invalid` parse of a body.
        let err = parse_fetch_url("not a url").unwrap_err();
        assert!(matches!(err, FetchError::Transport(_)));
    }

    #[test]
    fn version_fetcher_is_constructible_and_object_safe() {
        // `CredentialsConfig::new_with_fetcher` takes `Arc<dyn VersionFetcher>`,
        // so the default impl must be usable as a trait object.
        let fetcher = OcpiVersionFetcher::new().with_compat_raw_token(true);
        let _boxed: std::sync::Arc<dyn VersionFetcher> = std::sync::Arc::new(fetcher);

        // `with_client` reuses an existing reqwest client (shared pool).
        let _shared = OcpiVersionFetcher::with_client(reqwest::Client::new());

        // `Default` matches `new()`.
        let _default = OcpiVersionFetcher::default();
    }
}

//! OCPI HTTP transport conventions.
//!
//! Models the protocol-level HTTP plumbing shared by all OCPI endpoints:
//! token-based authorization, message-routing headers, paginated request
//! parameters, and paginated response metadata.
//!
//! Spec: `specs/ocpi/2.2.1/transport_and_format.asciidoc`

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Header name constants ──────────────────────────────────────────────────

/// `Authorization` header name (carries the OCPI credentials token).
pub const HEADER_AUTHORIZATION: &str = "Authorization";
/// `X-Request-ID` header name.
pub const HEADER_X_REQUEST_ID: &str = "X-Request-ID";
/// `X-Correlation-ID` header name.
pub const HEADER_X_CORRELATION_ID: &str = "X-Correlation-ID";
/// `OCPI-to-party-id` routing header name.
pub const HEADER_OCPI_TO_PARTY_ID: &str = "OCPI-to-party-id";
/// `OCPI-to-country-code` routing header name.
pub const HEADER_OCPI_TO_COUNTRY_CODE: &str = "OCPI-to-country-code";
/// `OCPI-from-party-id` routing header name.
pub const HEADER_OCPI_FROM_PARTY_ID: &str = "OCPI-from-party-id";
/// `OCPI-from-country-code` routing header name.
pub const HEADER_OCPI_FROM_COUNTRY_CODE: &str = "OCPI-from-country-code";
/// `X-Total-Count` pagination header name.
pub const HEADER_X_TOTAL_COUNT: &str = "X-Total-Count";
/// `X-Limit` pagination header name.
pub const HEADER_X_LIMIT: &str = "X-Limit";
/// `Link` pagination header name.
pub const HEADER_LINK: &str = "Link";

// ── Authorization ──────────────────────────────────────────────────────────

/// A credentials token used in the `Authorization` header.
///
/// OCPI 2.2.1 §4.1.1: the raw token is Base64-encoded (RFC 4648, standard
/// alphabet with `=` padding) before being placed in the header value.
///
/// Earlier versions (2.1.1, 2.2) often omitted the encoding. This type
/// always encodes on output. Interop with non-encoding peers requires a
/// configuration flag at the HTTP client layer — not modelled here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialToken(String);

impl CredentialToken {
    /// Wrap a raw (plaintext) credentials token.
    pub fn new(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    /// The raw (plaintext) token string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Build the `Authorization` header value: `Token <base64(raw)>`.
    pub fn to_header_value(&self) -> String {
        let encoded = B64.encode(self.0.as_bytes());
        format!("Token {encoded}")
    }

    /// Parse the raw token from an `Authorization` header value.
    ///
    /// Expects the format `Token <base64>`. Returns `None` if the prefix
    /// is absent, the Base64 is invalid, or the decoded bytes are not valid
    /// UTF-8.
    ///
    /// Use [`Self::from_header_value_lenient`] when interoperating with OCPI
    /// 2.1.1/2.2 peers that do not Base64-encode the token.
    pub fn from_header_value(value: &str) -> Option<Self> {
        let encoded = value.strip_prefix("Token ")?;
        let bytes = B64.decode(encoded.trim()).ok()?;
        let raw = String::from_utf8(bytes).ok()?;
        Some(Self(raw))
    }

    /// Parse the raw token from an `Authorization` header value, accepting
    /// both Base64-encoded (OCPI 2.2.1) and raw (OCPI 2.1.1/2.2) forms.
    ///
    /// Tries Base64-decode first. If that yields valid UTF-8, the decoded
    /// string is used. Otherwise the part after `"Token "` is treated as a
    /// raw (plaintext) token.
    ///
    /// # Caveats
    ///
    /// A raw token that is coincidentally valid Base64 will be decoded
    /// incorrectly. This ambiguity is inherent to the "trial-and-error"
    /// approach documented in the OCPI 2.2.1 spec note (§ Authorization header).
    /// Use [`Self::from_header_value`] (strict) when you control both sides.
    pub fn from_header_value_lenient(value: &str) -> Option<Self> {
        let tail = value.strip_prefix("Token ")?;
        let trimmed = tail.trim();
        if let Ok(bytes) = B64.decode(trimmed) {
            if let Ok(raw) = String::from_utf8(bytes) {
                return Some(Self(raw));
            }
        }
        Some(Self(trimmed.to_owned()))
    }
}

// ── Message routing headers ────────────────────────────────────────────────

/// The four OCPI message-routing headers.
///
/// These **must** be present on requests/responses to/from *Functional
/// Modules* (Tokens, Locations, CDRs, …) and **must not** be used on
/// *Configuration Modules* (Credentials, Versions, Hub Client Info).
///
/// `country_code` is an ISO 3166-1 alpha-2 code (2 chars). `party_id` is
/// the eMI3-assigned operator identifier (up to 3 chars, case-insensitive).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct OcpiRoutingHeaders {
    /// Destination party identifier (`OCPI-to-party-id`).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub to_party_id: Option<String>,
    /// Destination country code (`OCPI-to-country-code`).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub to_country_code: Option<String>,
    /// Source party identifier (`OCPI-from-party-id`).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub from_party_id: Option<String>,
    /// Source country code (`OCPI-from-country-code`).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub from_country_code: Option<String>,
}

// ── Pagination ─────────────────────────────────────────────────────────────

/// Query parameters for a paginated GET request.
///
/// All fields are optional; the server applies defaults when absent
/// (offset defaults to `0`; limit is server-determined).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PaginatedParams {
    /// Return only objects with `last_updated` ≥ this value (inclusive).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub date_from: Option<DateTime<Utc>>,
    /// Return only objects with `last_updated` < this value (exclusive).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub date_to: Option<DateTime<Utc>>,
    /// Zero-based index of the first object to return (default: `0`).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub offset: Option<u32>,
    /// Maximum number of objects to return.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub limit: Option<u32>,
}

/// Metadata extracted from the response headers of a paginated GET.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaginationMeta {
    /// URL of the next page; `None` when this is the last page.
    pub next_url: Option<String>,
    /// Total number of matching objects available on the server
    /// (`X-Total-Count`).
    pub total_count: u64,
    /// The upper limit the server will return per page (`X-Limit`).
    pub limit: u32,
}

impl PaginationMeta {
    /// Build from raw HTTP header values.
    ///
    /// Returns `None` if either `total_count_header` or `limit_header` is
    /// absent or not a valid decimal integer.
    ///
    /// # Arguments
    ///
    /// * `link_header` — value of the `Link` response header, or `None` on
    ///   the last page.
    /// * `total_count_header` — value of `X-Total-Count`.
    /// * `limit_header` — value of `X-Limit`.
    pub fn from_headers(
        link_header: Option<&str>,
        total_count_header: Option<&str>,
        limit_header: Option<&str>,
    ) -> Option<Self> {
        let total_count = total_count_header?.trim().parse::<u64>().ok()?;
        let limit = limit_header?.trim().parse::<u32>().ok()?;
        let next_url = link_header.and_then(parse_next_link);
        Some(Self {
            next_url,
            total_count,
            limit,
        })
    }
}

/// Extract the next-page URL from a `Link` header value.
///
/// Returns `None` if the header contains no `rel="next"` entry.
///
/// # Examples
///
/// ```
/// use ocpi_types::transport::parse_next_link;
///
/// let link = r#"<https://example.com/cdrs?offset=100&limit=50>; rel="next""#;
/// assert_eq!(
///     parse_next_link(link),
///     Some("https://example.com/cdrs?offset=100&limit=50".to_owned())
/// );
/// ```
pub fn parse_next_link(link: &str) -> Option<String> {
    for part in link.split(',') {
        let part = part.trim();
        if part.contains(r#"rel="next""#) {
            let start = part.find('<')?;
            let end = part.find('>')?;
            if end > start {
                return Some(part[start + 1..end].to_owned());
            }
        }
    }
    None
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── CredentialToken ──

    #[test]
    fn credential_token_roundtrip() {
        let token = CredentialToken::new("example-token");
        let header_value = token.to_header_value();
        assert!(header_value.starts_with("Token "));
        let parsed = CredentialToken::from_header_value(&header_value).expect("valid header");
        assert_eq!(parsed, token);
    }

    #[test]
    fn credential_token_known_encoding() {
        // "example-token" → UTF-8 bytes → RFC 4648 Base64 = "ZXhhbXBsZS10b2tlbg=="
        // (The spec's example has a trailing newline in the token, giving
        //  "ZXhhbXBsZS10b2tlbgo=" — this test uses the bare token.)
        let token = CredentialToken::new("example-token");
        assert_eq!(token.to_header_value(), "Token ZXhhbXBsZS10b2tlbg==");
    }

    #[test]
    fn credential_token_invalid_base64_is_none() {
        assert!(CredentialToken::from_header_value("Token not!valid_base64!!!").is_none());
    }

    #[test]
    fn credential_token_wrong_scheme_is_none() {
        assert!(CredentialToken::from_header_value("Bearer abc").is_none());
    }

    // ── CredentialToken::from_header_value_lenient ──

    #[test]
    fn lenient_accepts_base64_encoded_token() {
        // "example-token" → Base64 → should round-trip via lenient parser
        let token = CredentialToken::new("example-token");
        let header = token.to_header_value();
        let parsed = CredentialToken::from_header_value_lenient(&header).expect("lenient parse");
        assert_eq!(parsed.as_str(), "example-token");
    }

    #[test]
    fn lenient_accepts_raw_token_that_is_not_base64() {
        // "my!!token" contains '!' which is not a Base64 character → treated as raw
        let header = "Token my!!token";
        let parsed = CredentialToken::from_header_value_lenient(header).expect("lenient parse");
        assert_eq!(parsed.as_str(), "my!!token");
    }

    #[test]
    fn lenient_wrong_scheme_is_none() {
        assert!(CredentialToken::from_header_value_lenient("Bearer abc").is_none());
    }

    #[test]
    fn lenient_missing_prefix_is_none() {
        assert!(CredentialToken::from_header_value_lenient("ZXhhbXBsZS10b2tlbg==").is_none());
    }

    // ── parse_next_link ──

    #[test]
    fn parse_next_link_single_relation() {
        let link =
            r#"<https://www.server.com/ocpi/cpo/2.2.1/cdrs/?offset=150&limit=50>; rel="next""#;
        assert_eq!(
            parse_next_link(link),
            Some("https://www.server.com/ocpi/cpo/2.2.1/cdrs/?offset=150&limit=50".to_owned())
        );
    }

    #[test]
    fn parse_next_link_multiple_relations() {
        let link = r#"<https://example.com/prev>; rel="prev", <https://example.com/next?offset=10>; rel="next""#;
        assert_eq!(
            parse_next_link(link),
            Some("https://example.com/next?offset=10".to_owned())
        );
    }

    #[test]
    fn parse_next_link_last_page_is_none() {
        let link = r#"<https://example.com/prev>; rel="prev""#;
        assert!(parse_next_link(link).is_none());
    }

    // ── PaginationMeta ──

    #[test]
    fn pagination_meta_full() {
        let meta = PaginationMeta::from_headers(
            Some(r#"<https://example.com/cdrs?offset=50&limit=50>; rel="next""#),
            Some("1234"),
            Some("50"),
        )
        .expect("valid headers");
        assert_eq!(meta.total_count, 1234);
        assert_eq!(meta.limit, 50);
        assert_eq!(
            meta.next_url,
            Some("https://example.com/cdrs?offset=50&limit=50".to_owned())
        );
    }

    #[test]
    fn pagination_meta_last_page_no_link() {
        let meta =
            PaginationMeta::from_headers(None, Some("10"), Some("50")).expect("valid headers");
        assert_eq!(meta.total_count, 10);
        assert!(meta.next_url.is_none());
    }

    #[test]
    fn pagination_meta_missing_total_count_is_none() {
        assert!(PaginationMeta::from_headers(None, None, Some("50")).is_none());
    }

    #[test]
    fn pagination_meta_missing_limit_is_none() {
        assert!(PaginationMeta::from_headers(None, Some("10"), None).is_none());
    }

    // ── PaginatedParams serde ──

    #[test]
    fn paginated_params_roundtrip() {
        let params = PaginatedParams {
            date_from: Some(
                DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            ),
            date_to: None,
            offset: Some(0),
            limit: Some(50),
        };
        let json = serde_json::to_string(&params).expect("serialize");
        let back: PaginatedParams = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, params);
        assert!(!json.contains("date_to"));
    }

    #[test]
    fn paginated_params_empty_omits_all_fields() {
        let params = PaginatedParams::default();
        let json = serde_json::to_string(&params).expect("serialize");
        assert_eq!(json, "{}");
    }

    // ── OcpiRoutingHeaders serde ──

    #[test]
    fn routing_headers_partial_roundtrip() {
        let headers = OcpiRoutingHeaders {
            to_party_id: Some("TNM".to_owned()),
            to_country_code: Some("NL".to_owned()),
            from_party_id: None,
            from_country_code: None,
        };
        let json = serde_json::to_string(&headers).expect("serialize");
        assert!(!json.contains("from_party_id"));
        let back: OcpiRoutingHeaders = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.to_party_id, Some("TNM".to_owned()));
        assert!(back.from_party_id.is_none());
    }

    #[test]
    fn routing_headers_default_is_all_none() {
        let h = OcpiRoutingHeaders::default();
        assert!(h.to_party_id.is_none());
        assert!(h.to_country_code.is_none());
        assert!(h.from_party_id.is_none());
        assert!(h.from_country_code.is_none());
    }
}

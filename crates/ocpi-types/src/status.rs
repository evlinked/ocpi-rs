//! Canonical OCPI status codes.
//!
//! Every OCPI HTTP response carries an integer `status_code` in its body
//! (independent of the HTTP status). `1000` means success; `2xxx` are client
//! errors, `3xxx` server errors, and `4xxx` hub errors. See the OCPI
//! `status_codes` specification chapter.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A status code returned in the body of every OCPI response.
///
/// Serialises and deserialises as the integer wire value (e.g. `1000`).
/// Use [`OcpiStatusCode::code`] for the raw integer and
/// [`OcpiStatusCode::from_code`] to parse a known code.
///
/// Unknown or custom-range codes (e.g. `19xx`, `29xx`, `39xx`, `49xx`) are
/// preserved as [`OcpiStatusCode::Unknown`] so they survive a round-trip
/// without data loss.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "u16", into = "u16")]
pub enum OcpiStatusCode {
    /// `1000` — generic success.
    Success,
    /// `2000` — generic client error.
    ClientError,
    /// `2001` — invalid or missing parameters.
    InvalidParameters,
    /// `2002` — not enough information provided.
    NotEnoughInformation,
    /// `2003` — unknown location / EVSE / connector.
    UnknownLocation,
    /// `2004` — unknown token.
    UnknownToken,
    /// `3000` — generic server error.
    ServerError,
    /// `3001` — unable to use the client's API.
    UnableToUseClientApi,
    /// `3002` — unsupported version.
    UnsupportedVersion,
    /// `3003` — no matching endpoints or expected endpoints missing.
    MissingEndpoints,
    /// `4000` — generic hub error.
    HubError,
    /// `4001` — unknown receiver (the `to` address is unknown).
    UnknownReceiver,
    /// `4002` — timeout on a forwarded request.
    HubTimeout,
    /// `4003` — connection problem inside the hub.
    HubConnectionProblem,
    /// A status code not defined in the standard (custom or future range).
    Unknown(u16),
}

impl OcpiStatusCode {
    /// The integer code as sent on the wire.
    #[must_use]
    pub const fn code(self) -> u16 {
        match self {
            Self::Success => 1000,
            Self::ClientError => 2000,
            Self::InvalidParameters => 2001,
            Self::NotEnoughInformation => 2002,
            Self::UnknownLocation => 2003,
            Self::UnknownToken => 2004,
            Self::ServerError => 3000,
            Self::UnableToUseClientApi => 3001,
            Self::UnsupportedVersion => 3002,
            Self::MissingEndpoints => 3003,
            Self::HubError => 4000,
            Self::UnknownReceiver => 4001,
            Self::HubTimeout => 4002,
            Self::HubConnectionProblem => 4003,
            Self::Unknown(n) => n,
        }
    }

    /// Parse a wire integer into a known status code.
    ///
    /// Returns `None` for codes outside the standard table; use
    /// [`From<u16>`] (which produces [`Self::Unknown`]) if you need an
    /// infallible parse.
    #[must_use]
    pub const fn from_code(code: u16) -> Option<Self> {
        let value = match code {
            1000 => Self::Success,
            2000 => Self::ClientError,
            2001 => Self::InvalidParameters,
            2002 => Self::NotEnoughInformation,
            2003 => Self::UnknownLocation,
            2004 => Self::UnknownToken,
            3000 => Self::ServerError,
            3001 => Self::UnableToUseClientApi,
            3002 => Self::UnsupportedVersion,
            3003 => Self::MissingEndpoints,
            4000 => Self::HubError,
            4001 => Self::UnknownReceiver,
            4002 => Self::HubTimeout,
            4003 => Self::HubConnectionProblem,
            _ => return None,
        };
        Some(value)
    }

    /// `true` if this is the success code (`1000`).
    #[must_use]
    pub const fn is_success(self) -> bool {
        matches!(self, Self::Success)
    }

    /// A short human-readable description of the code.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::Success => "Success",
            Self::ClientError => "Client error",
            Self::InvalidParameters => "Invalid or missing parameters",
            Self::NotEnoughInformation => "Not enough information",
            Self::UnknownLocation => "Unknown location",
            Self::UnknownToken => "Unknown token",
            Self::ServerError => "Server error",
            Self::UnableToUseClientApi => "Unable to use the client's API",
            Self::UnsupportedVersion => "Unsupported version",
            Self::MissingEndpoints => "Missing expected endpoints",
            Self::HubError => "Hub error",
            Self::UnknownReceiver => "Unknown receiver",
            Self::HubTimeout => "Timeout on forwarded request",
            Self::HubConnectionProblem => "Hub connection problem",
            Self::Unknown(_) => "Unknown status code",
        }
    }
}

// --- serde glue -----------------------------------------------------------

impl From<u16> for OcpiStatusCode {
    fn from(code: u16) -> Self {
        Self::from_code(code).unwrap_or(Self::Unknown(code))
    }
}

impl From<OcpiStatusCode> for u16 {
    fn from(code: OcpiStatusCode) -> Self {
        code.code()
    }
}

// --- Display --------------------------------------------------------------

impl fmt::Display for OcpiStatusCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.code())
    }
}

// --- Tests ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::OcpiStatusCode;

    /// Every standard code survives a round-trip through the integer value.
    #[test]
    fn known_codes_round_trip() {
        for code in [
            1000u16, 2000, 2001, 2002, 2003, 2004, 3000, 3001, 3002, 3003, 4000, 4001, 4002, 4003,
        ] {
            let parsed = OcpiStatusCode::from_code(code).expect("known code");
            assert_eq!(parsed.code(), code);
        }
    }

    /// Non-standard codes are preserved as `Unknown`.
    #[test]
    fn unknown_code_round_trips() {
        let u = OcpiStatusCode::from(9999u16);
        assert_eq!(u, OcpiStatusCode::Unknown(9999));
        assert_eq!(u.code(), 9999);
        assert!(!u.is_success());
    }

    /// `from_code` returns `None` for non-standard codes.
    #[test]
    fn from_code_returns_none_for_unknown() {
        assert_eq!(OcpiStatusCode::from_code(9999), None);
        // Custom ranges defined by the spec (19xx, 29xx, 39xx, 49xx)
        assert_eq!(OcpiStatusCode::from_code(1900), None);
        assert_eq!(OcpiStatusCode::from_code(2900), None);
    }

    /// Only `1000` is a success code.
    #[test]
    fn only_1000_is_success() {
        assert!(OcpiStatusCode::Success.is_success());
        assert!(!OcpiStatusCode::ServerError.is_success());
        assert!(!OcpiStatusCode::Unknown(1000).is_success());
    }

    /// `Display` shows the integer code.
    #[test]
    fn display_shows_integer() {
        assert_eq!(OcpiStatusCode::Success.to_string(), "1000");
        assert_eq!(OcpiStatusCode::Unknown(9999).to_string(), "9999");
    }

    /// Serde round-trip via JSON.
    #[test]
    fn serde_round_trip_known() {
        let code = OcpiStatusCode::InvalidParameters;
        let json = serde_json::to_string(&code).expect("serialize");
        assert_eq!(json, "2001");
        let back: OcpiStatusCode = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, code);
    }

    /// Unknown codes survive a JSON round-trip.
    #[test]
    fn serde_round_trip_unknown() {
        let json = "1900";
        let code: OcpiStatusCode = serde_json::from_str(json).expect("deserialize");
        assert_eq!(code, OcpiStatusCode::Unknown(1900));
        let back = serde_json::to_string(&code).expect("serialize");
        assert_eq!(back, "1900");
    }
}

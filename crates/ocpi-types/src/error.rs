//! Error type shared across the OCPI crates.

use crate::status::OcpiStatusCode;

/// An error constructing or interpreting OCPI messages.
#[derive(Debug, thiserror::Error)]
pub enum OcpiError {
    /// A response carried a non-success [`OcpiStatusCode`].
    #[error("OCPI status code {} ({})", .0.code(), .0.message())]
    Status(OcpiStatusCode),

    /// A value did not conform to the OCPI specification.
    #[error("invalid OCPI value: {0}")]
    Invalid(String),

    /// A feature is part of the protocol but not yet implemented in this crate.
    #[error("not yet implemented: {0}")]
    NotImplemented(&'static str),
}

impl OcpiError {
    /// The OCPI status code that best represents this error.
    ///
    /// | Variant | Code |
    /// |---------|------|
    /// | `Status(c)` | `c` (whatever the server sent) |
    /// | `Invalid(_)` | `2001` — invalid or missing parameters |
    /// | `NotImplemented(_)` | `3000` — generic server error |
    #[must_use]
    pub fn status_code(&self) -> OcpiStatusCode {
        match self {
            Self::Status(code) => *code,
            Self::Invalid(_) => OcpiStatusCode::InvalidParameters,
            Self::NotImplemented(_) => OcpiStatusCode::ServerError,
        }
    }
}

impl From<OcpiStatusCode> for OcpiError {
    fn from(code: OcpiStatusCode) -> Self {
        Self::Status(code)
    }
}

#[cfg(test)]
mod tests {
    use super::OcpiError;
    use crate::status::OcpiStatusCode;

    #[test]
    fn status_variant_maps_to_its_own_code() {
        let err = OcpiError::Status(OcpiStatusCode::HubTimeout);
        assert_eq!(err.status_code(), OcpiStatusCode::HubTimeout);
    }

    #[test]
    fn invalid_maps_to_invalid_parameters() {
        let err = OcpiError::Invalid("missing country_code".into());
        assert_eq!(err.status_code(), OcpiStatusCode::InvalidParameters);
    }

    #[test]
    fn not_implemented_maps_to_server_error() {
        let err = OcpiError::NotImplemented("credentials");
        assert_eq!(err.status_code(), OcpiStatusCode::ServerError);
    }

    #[test]
    fn from_status_code_round_trips() {
        let code = OcpiStatusCode::UnsupportedVersion;
        let err: OcpiError = code.into();
        assert_eq!(err.status_code(), code);
    }
}

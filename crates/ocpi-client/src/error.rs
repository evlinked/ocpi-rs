//! Errors produced by the OCPI client.

/// Something went wrong while talking to a remote OCPI party.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// The underlying HTTP request failed.
    #[error(transparent)]
    Http(#[from] reqwest::Error),

    /// A URL could not be constructed.
    #[error("invalid URL: {0}")]
    Url(#[from] url::ParseError),

    /// The response envelope reported success but carried no `data`.
    #[error("response envelope contained no data")]
    EmptyData,

    /// The response carried a non-success OCPI status code.
    #[error(transparent)]
    Ocpi(#[from] ocpi_types::OcpiError),

    /// The requested operation is not yet implemented.
    #[error("not yet implemented: {0}")]
    NotImplemented(&'static str),

    /// No OCPI version is supported by both parties; negotiation failed.
    ///
    /// Corresponds to OCPI status code `3002` (`UnsupportedVersion`).
    #[error("no mutual OCPI version: remote and local version sets do not overlap")]
    NoMutualVersion,

    /// The requested resource was not found on the remote party.
    ///
    /// Typically returned when the remote responds with OCPI status code
    /// `2003` (`UnknownLocation`) or HTTP 404.
    #[error("resource not found on remote")]
    NotFound,
}

#[cfg(test)]
mod tests {
    use super::ClientError;

    #[test]
    fn no_mutual_version_displays_correctly() {
        let err = ClientError::NoMutualVersion;
        assert!(err.to_string().contains("no mutual OCPI version"));
    }

    #[test]
    fn not_found_displays_correctly() {
        let err = ClientError::NotFound;
        assert!(err.to_string().contains("not found"));
    }
}

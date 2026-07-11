//! OCPI **2.3.0** Credentials override — the `hub_party_id` delta.
//!
//! 2.3.0 adds a single field to the Credentials object relative to 2.2.1:
//!
//! > `hub_party_id` — `CiString(5)`, cardinality `?`. The Hub party of this
//! > platform. The two-letter country code and three-character party ID are
//! > concatenated together in this field as one five-character string.
//!
//! Spec: `specs/ocpi/2.3.0/credentials.asciidoc` — Credentials object.
//!
//! A Platform that supports Hub functionality with the message-routing headers
//! SHALL give the country code and party ID of the Hub in `hub_party_id`.
//! 2.3.0 also expects a Roaming Hub's platform to list the parties reachable
//! through it as **normal** [`CredentialsRole`] entries in `roles` — a
//! documentation/usage change that needs no new type, so [`CredentialsRole`]
//! stays wire-identical to 2.2.1 and is re-exported unchanged.
//!
//! Everything except the new field is byte-for-byte the 2.2.1 shape, so this
//! is a minimal `v2_3_0`-local fork: `token` + `url` + `roles` are identical,
//! and `hub_party_id` is `skip_serializing_if = "Option::is_none"` so a
//! non-hub 2.3.0 platform's credentials round-trip exactly like 2.2.1's.

use serde::{Deserialize, Serialize};

use crate::common::{CiString5, Url};
use crate::OcpiError;

// `CredentialsRole` is wire-identical between 2.2.1 and 2.3.0 (the 2.3.0 change
// is only *which* roles a hub lists, not the role's shape), so it stays a plain
// re-export — `Credentials::roles` below references this same type.
pub use crate::v2_2_1::CredentialsRole;

/// The OCPI **2.3.0** credentials object exchanged during registration (POST),
/// updates (PUT), and returned on GET.
///
/// Identical to [`crate::v2_2_1::Credentials`] except for the added
/// [`hub_party_id`](Credentials::hub_party_id) field. `roles` must be
/// non-empty; multi-role is schema-legal per the spec.
///
/// Spec: `specs/ocpi/2.3.0/credentials.asciidoc` — Credentials object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credentials {
    /// Bearer token the remote party must use in subsequent requests.
    ///
    /// OCPI spec: printable non-whitespace ASCII (U+0021–U+007E), max 64
    /// characters. Not validated here — callers are responsible.
    pub token: String,
    /// URL of this party's `/versions` endpoint.
    pub url: Url,
    /// **2.3.0 addition.** The Hub party of this platform: the two-letter
    /// country code and three-character party ID concatenated into one
    /// five-character string (e.g. `"NLHUB"`). Present only for a platform that
    /// provides Hub functionality with the message-routing headers; absent
    /// otherwise (and then omitted from the wire).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hub_party_id: Option<CiString5>,
    /// Roles this platform provides. Non-empty; one entry is the common case.
    /// A 2.3.0 Roaming Hub lists the parties reachable through it here too.
    pub roles: Vec<CredentialsRole>,
}

impl Credentials {
    /// Returns `Err` when `roles` is empty (spec requires at least one).
    ///
    /// # Errors
    ///
    /// Returns [`OcpiError::Invalid`] if `roles` is empty.
    pub fn validate(&self) -> Result<(), OcpiError> {
        if self.roles.is_empty() {
            return Err(OcpiError::Invalid(
                "credentials.roles must contain at least one entry".into(),
            ));
        }
        Ok(())
    }

    /// Returns `Err` when `roles` has more than one entry.
    ///
    /// Call this in server implementations that have not yet added multi-role
    /// support; return
    /// [`OcpiStatusCode::ServerError`](crate::OcpiStatusCode::ServerError) to
    /// the remote party so it knows the limitation is server-side.
    ///
    /// # Errors
    ///
    /// Returns [`OcpiError::Invalid`] if `roles.len() > 1`.
    pub fn check_single_role(&self) -> Result<(), OcpiError> {
        if self.roles.len() > 1 {
            return Err(OcpiError::Invalid(
                "multi-role credentials are not yet supported by this server".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Credentials, CredentialsRole};
    use crate::common::CiString5;

    // A minimal CPO credentials object — no `hub_party_id`, so the field is
    // absent on the wire and this is byte-for-byte a 2.2.1 credentials object.
    const MINIMAL_CPO: &str = r#"{
        "token": "ebf3b399-779f-4497-9b9d-ac6ad3cc44d2",
        "url": "https://example.com/ocpi/versions",
        "roles": [
            {
                "role": "CPO",
                "business_details": { "name": "Example Operator" },
                "party_id": "EXA",
                "country_code": "NL"
            }
        ]
    }"#;

    // A hub platform's credentials object carrying `hub_party_id` and listing a
    // reachable party as a normal `roles` entry — the 2.3.0 shape.
    const HUB_CREDENTIALS: &str = r#"{
        "token": "9bf3b399-779f-4497-9b9d-ac6ad3cc44d2",
        "url": "https://hub.example.com/ocpi/versions",
        "hub_party_id": "NLHUB",
        "roles": [
            {
                "role": "HUB",
                "business_details": { "name": "Example Hub" },
                "party_id": "HUB",
                "country_code": "NL"
            },
            {
                "role": "CPO",
                "business_details": { "name": "Reachable CPO" },
                "party_id": "RCP",
                "country_code": "DE"
            }
        ]
    }"#;

    #[test]
    fn minimal_credentials_round_trip_omits_absent_hub_party_id() {
        let creds: Credentials = serde_json::from_str(MINIMAL_CPO).unwrap();
        assert!(creds.hub_party_id.is_none());
        assert_eq!(creds.roles.len(), 1);

        let out = serde_json::to_string(&creds).unwrap();
        // `skip_serializing_if` keeps the field off the wire when absent, so a
        // non-hub 2.3.0 credentials object stays wire-identical to 2.2.1's.
        assert!(
            !out.contains("hub_party_id"),
            "unexpected field on wire: {out}"
        );
        let back: Credentials = serde_json::from_str(&out).unwrap();
        assert_eq!(back, creds);
    }

    #[test]
    fn hub_credentials_round_trip_carries_hub_party_id_and_reachable_roles() {
        let creds: Credentials = serde_json::from_str(HUB_CREDENTIALS).unwrap();
        assert_eq!(
            creds.hub_party_id.as_ref().map(|s| s.as_str()),
            Some("NLHUB")
        );
        // 2.3.0: a hub lists the parties reachable through it as normal roles.
        assert_eq!(creds.roles.len(), 2);
        assert_eq!(creds.roles[1].party_id.as_str(), "RCP");

        let out = serde_json::to_string(&creds).unwrap();
        assert!(out.contains("\"hub_party_id\":\"NLHUB\""));
        let back: Credentials = serde_json::from_str(&out).unwrap();
        assert_eq!(back, creds);
    }

    #[test]
    fn minimal_2_3_0_credentials_deserialize_identically_via_2_2_1_shape() {
        // The non-hub 2.3.0 object is wire-compatible with the 2.2.1 type: the
        // same JSON parses into both, and the shared fields agree.
        let via_2_3_0: Credentials = serde_json::from_str(MINIMAL_CPO).unwrap();
        let via_2_2_1: crate::v2_2_1::Credentials = serde_json::from_str(MINIMAL_CPO).unwrap();
        assert_eq!(via_2_3_0.token, via_2_2_1.token);
        assert_eq!(via_2_3_0.url, via_2_2_1.url);
        assert_eq!(via_2_3_0.roles, via_2_2_1.roles);
    }

    #[test]
    fn hub_party_id_over_five_chars_is_rejected_on_deserialize() {
        // `hub_party_id` is CiString(5); an over-length value fails at the trust
        // boundary rather than being silently truncated or accepted.
        let bad = r#"{
            "token": "t",
            "url": "https://example.com/ocpi/versions",
            "hub_party_id": "TOOLONG",
            "roles": [
                {
                    "role": "HUB",
                    "business_details": { "name": "H" },
                    "party_id": "HUB",
                    "country_code": "NL"
                }
            ]
        }"#;
        assert!(serde_json::from_str::<Credentials>(bad).is_err());
    }

    #[test]
    fn validate_and_single_role_guards_behave_as_2_2_1() {
        let mut creds: Credentials = serde_json::from_str(HUB_CREDENTIALS).unwrap();
        assert!(creds.validate().is_ok());
        // Two roles → the single-role server guard rejects it.
        assert!(creds.check_single_role().is_err());

        creds.roles.truncate(1);
        assert!(creds.check_single_role().is_ok());

        creds.roles.clear();
        assert!(creds.validate().is_err());
    }

    #[test]
    fn hub_party_id_constructs_from_five_char_string() {
        let id = CiString5::try_from("NLHUB".to_string()).unwrap();
        let role: CredentialsRole = serde_json::from_str(
            r#"{ "role": "HUB", "business_details": { "name": "H" }, "party_id": "HUB", "country_code": "NL" }"#,
        )
        .unwrap();
        let creds = Credentials {
            token: "t".to_string(),
            url: crate::common::Url::try_from("https://example.com/ocpi/versions").unwrap(),
            hub_party_id: Some(id),
            roles: vec![role],
        };
        let back: Credentials =
            serde_json::from_str(&serde_json::to_string(&creds).unwrap()).unwrap();
        assert_eq!(back, creds);
    }
}

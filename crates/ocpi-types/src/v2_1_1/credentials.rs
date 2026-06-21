//! OCPI 2.1.1 — Credentials module / registration handshake types.

use serde::{Deserialize, Serialize};

use crate::common::{BusinessDetails, CiString2, CiString3, Url};

// ── Credentials ───────────────────────────────────────────────────────────────

/// The OCPI **2.1.1** credentials object exchanged during registration (POST),
/// updates (PUT), and returned on GET.
///
/// ## Delta from 2.2.1
///
/// The 2.1.1 object is **flat**: a party advertises exactly one
/// `party_id` / `country_code` / `business_details` at the top level. There is
/// **no `roles: [...]` array** — the multi-role array (and the Hub role) were
/// introduced in OCPI 2.2. Modelling 2.1.1 with the 2.2.1
/// [`crate::v2_2_1::Credentials`] (which requires `roles`) would not round-trip
/// against a real 2.1.1 peer, so this is a distinct type.
///
/// The registration flow is the classic *Token A → B → C* exchange via
/// `GET`/`POST`/`PUT`/`DELETE /credentials`, but with **no `OCPI-to/from-*`
/// routing headers** (those are 2.2+ only).
///
/// Spec: OCPI 2.1.1 — *Credentials* module / *Registration* use-case
/// (`specs/ocpi/2.1.1/OCPI_2.1.1.pdf`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credentials {
    /// Bearer token the remote party must use in subsequent requests.
    ///
    /// OCPI 2.1.1 spec: case-sensitive ASCII, max 64 characters. Not validated
    /// here — callers are responsible.
    pub token: String,
    /// URL of this party's `/versions` endpoint.
    pub url: Url,
    /// Human-readable business details for this party.
    pub business_details: BusinessDetails,
    /// eMI3 party identifier (3-char, e.g. `"EXA"`).
    pub party_id: CiString3,
    /// ISO 3166-1 alpha-2 country code (e.g. `"NL"`).
    pub country_code: CiString2,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::Image;

    #[test]
    fn credentials_serde_spec_example() {
        // Ported from the OCPI 2.1.1 spec "Credentials object" example — note
        // the flat shape: party_id/country_code/business_details at top level,
        // and NO `roles` array.
        // Spec ref: specs/ocpi/2.1.1/OCPI_2.1.1.pdf, "Credentials" module.
        let json = r#"{
            "token": "ebf3b399-779f-4497-9b9d-ac6ad3cc44d2",
            "url": "https://example.com/ocpi/cpo/versions",
            "business_details": {
                "name": "Example Operator",
                "website": "https://example.com"
            },
            "party_id": "EXA",
            "country_code": "NL"
        }"#;
        let creds: Credentials = serde_json::from_str(json).unwrap();
        assert_eq!(creds.token, "ebf3b399-779f-4497-9b9d-ac6ad3cc44d2");
        assert_eq!(creds.party_id.as_str(), "EXA");
        assert_eq!(creds.country_code.as_str(), "NL");
        assert_eq!(creds.business_details.name, "Example Operator");

        let back: Credentials =
            serde_json::from_str(&serde_json::to_string(&creds).unwrap()).unwrap();
        assert_eq!(back, creds);
    }

    #[test]
    fn credentials_wire_form_is_flat_no_roles_array() {
        let creds = Credentials {
            token: "TOKEN_B".into(),
            url: Url::try_from("https://emsp.example.com/ocpi/versions").unwrap(),
            business_details: BusinessDetails {
                name: "Example Provider".into(),
                website: None,
                logo: None,
            },
            party_id: CiString3::try_from("MSP").unwrap(),
            country_code: CiString2::try_from("DE").unwrap(),
        };
        let json = serde_json::to_string(&creds).unwrap();
        // The 2.1.1 wire form must NOT carry a 2.2+ `roles` array, and must
        // carry party_id/country_code at the top level.
        assert!(
            !json.contains("\"roles\""),
            "2.1.1 credentials must be flat (no roles array): {json}"
        );
        assert!(json.contains("\"party_id\":\"MSP\""));
        assert!(json.contains("\"country_code\":\"DE\""));
    }

    #[test]
    fn business_details_logo_roundtrips() {
        let creds = Credentials {
            token: "TOKEN_C".into(),
            url: Url::try_from("https://cpo.example.com/ocpi/versions").unwrap(),
            business_details: BusinessDetails {
                name: "Example Operator".into(),
                website: Some("https://example.com".into()),
                logo: Some(Image {
                    url: "https://example.com/logo.png".into(),
                    thumbnail: Some("https://example.com/logo-thumb.png".into()),
                    category: "OPERATOR".into(),
                    image_type: "png".into(),
                    width: Some(512),
                    height: Some(512),
                }),
            },
            party_id: CiString3::try_from("EXA").unwrap(),
            country_code: CiString2::try_from("NL").unwrap(),
        };
        let back: Credentials =
            serde_json::from_str(&serde_json::to_string(&creds).unwrap()).unwrap();
        assert_eq!(back, creds);
        assert_eq!(back.business_details.logo.unwrap().category, "OPERATOR");
    }
}

//! OCPI **2.1.1** typed models.
//!
//! Modules: Versions, Credentials, Locations, Sessions, CDRs, Tariffs, Tokens,
//! Commands.
//!
//! Populated incrementally — see milestone **M7** in the roadmap. Shared
//! primitives live in [`crate::common`], [`crate::envelope`], and
//! [`crate::version`].
//!
//! ## Version-negotiation delta from 2.2.1
//!
//! The Sender/Receiver `InterfaceRole` split was introduced in OCPI **2.2**.
//! In 2.1.1 a version-details endpoint entry is therefore just
//! `{ identifier, url }` with **no `role` field**. The shared
//! [`crate::version::Endpoint`] carries a required `role`, so it cannot
//! represent a 2.1.1 endpoint faithfully. Rather than weaken the 2.2.1 type
//! by making `role` optional, this module defines a distinct 2.1.1-shaped
//! [`Endpoint`] and [`VersionDetails`]. [`VersionNumber`] and [`ModuleID`]
//! are version-agnostic and reused as-is.
//!
//! The 2.1.1 module set is **Locations, Sessions, CDRs, Tariffs, Tokens,
//! Commands, Credentials** — there is **no** `HubClientInfo` and **no**
//! `ChargingProfiles` (both arrived in 2.2). The reused [`ModuleID`] enum is a
//! superset; constructing a 2.1.1 [`Endpoint`] with one of the 2.2-only
//! identifiers is possible at the type level but out of spec — see
//! [`Endpoint::is_valid_2_1_1`].

use serde::{Deserialize, Serialize};

use crate::common::Url;
use crate::version::{ModuleID, VersionNumber};

// ── Endpoint ──────────────────────────────────────────────────────────────────

/// A single module endpoint entry within a 2.1.1 [`VersionDetails`] response.
///
/// Unlike the 2.2.1 [`crate::version::Endpoint`], there is **no `role`** —
/// the Sender/Receiver split did not exist before OCPI 2.2.
///
/// Spec: OCPI 2.1.1 — *Version details endpoint* (`specs/ocpi/2.1.1/OCPI_2.1.1.pdf`,
/// "Versions" chapter, Endpoint class).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    /// Which module this endpoint belongs to.
    pub identifier: ModuleID,
    /// URL to call for this module endpoint.
    pub url: Url,
}

impl Endpoint {
    /// Whether `identifier` is part of the OCPI 2.1.1 module set.
    ///
    /// `HubClientInfo` and `ChargingProfiles` were introduced in 2.2 and are
    /// therefore not valid in a 2.1.1 version-details response, even though
    /// the shared [`ModuleID`] enum can represent them.
    #[must_use]
    pub const fn is_valid_2_1_1(&self) -> bool {
        !matches!(
            self.identifier,
            ModuleID::HubClientInfo | ModuleID::ChargingProfiles
        )
    }
}

// ── VersionDetails ────────────────────────────────────────────────────────────

/// Response body for `GET /versions/{version}` in OCPI 2.1.1.
///
/// Lists all supported module endpoints for the 2.1.1 API. Mirrors the 2.2.1
/// [`crate::version::VersionDetails`] but uses the role-less 2.1.1 [`Endpoint`].
///
/// Spec: OCPI 2.1.1 — *Version details endpoint* (`specs/ocpi/2.1.1/OCPI_2.1.1.pdf`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionDetails {
    /// The OCPI version these endpoints implement (expected `2.1.1`).
    pub version: VersionNumber,
    /// All supported module endpoints for this version.
    pub endpoints: Vec<Endpoint>,
}

impl VersionDetails {
    /// Whether every endpoint uses a module identifier valid in OCPI 2.1.1.
    ///
    /// See [`Endpoint::is_valid_2_1_1`].
    #[must_use]
    pub fn endpoints_valid_2_1_1(&self) -> bool {
        self.endpoints.iter().all(Endpoint::is_valid_2_1_1)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_has_no_role_on_the_wire() {
        let ep = Endpoint {
            identifier: ModuleID::Locations,
            url: Url::try_from("https://example.com/ocpi/cpo/2.1.1/locations/").unwrap(),
        };
        let json = serde_json::to_string(&ep).unwrap();
        assert!(
            !json.contains("role"),
            "2.1.1 endpoint must omit role: {json}"
        );
        assert!(json.contains("identifier"));
        assert!(json.contains("url"));
        let back: Endpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ep);
    }

    #[test]
    fn endpoint_deserializes_without_role() {
        // A faithful 2.1.1 payload carries no role key at all.
        let json = r#"{
            "identifier": "tokens",
            "url": "https://example.com/ocpi/cpo/2.1.1/tokens/"
        }"#;
        let ep: Endpoint = serde_json::from_str(json).unwrap();
        assert_eq!(ep.identifier, ModuleID::Tokens);
        assert_eq!(
            ep.url.as_str(),
            "https://example.com/ocpi/cpo/2.1.1/tokens/"
        );
    }

    #[test]
    fn version_details_serde_spec_example() {
        // Ported from the OCPI 2.1.1 spec "Version details endpoint" example:
        // a CPO advertising its 2.1.1 module endpoints. Note: no `role` field.
        // Spec ref: specs/ocpi/2.1.1/OCPI_2.1.1.pdf, "Versions" chapter.
        let json = r#"{
            "version": "2.1.1",
            "endpoints": [
                {
                    "identifier": "credentials",
                    "url": "https://example.com/ocpi/cpo/2.1.1/credentials/"
                },
                {
                    "identifier": "locations",
                    "url": "https://example.com/ocpi/cpo/2.1.1/locations/"
                },
                {
                    "identifier": "tariffs",
                    "url": "https://example.com/ocpi/cpo/2.1.1/tariffs/"
                },
                {
                    "identifier": "tokens",
                    "url": "https://example.com/ocpi/cpo/2.1.1/tokens/"
                }
            ]
        }"#;
        let details: VersionDetails = serde_json::from_str(json).unwrap();
        assert_eq!(details.version, VersionNumber::V2_1_1);
        assert_eq!(details.endpoints.len(), 4);
        assert_eq!(details.endpoints[0].identifier, ModuleID::Credentials);
        assert_eq!(details.endpoints[1].identifier, ModuleID::Locations);
        assert_eq!(
            details.endpoints[1].url.as_str(),
            "https://example.com/ocpi/cpo/2.1.1/locations/"
        );
        assert!(details.endpoints_valid_2_1_1());

        // Round-trip and prove no `role` leaks into the serialized form.
        let out = serde_json::to_string(&details).unwrap();
        assert!(
            !out.contains("role"),
            "serialized 2.1.1 details must omit role"
        );
        let back: VersionDetails = serde_json::from_str(&out).unwrap();
        assert_eq!(back, details);
    }

    #[test]
    fn detects_out_of_spec_2_2_modules() {
        // HubClientInfo / ChargingProfiles are 2.2+ and not valid in 2.1.1.
        let hub = Endpoint {
            identifier: ModuleID::HubClientInfo,
            url: Url::try_from("https://example.com/ocpi/cpo/2.1.1/hubclientinfo/").unwrap(),
        };
        assert!(!hub.is_valid_2_1_1());

        let profiles = Endpoint {
            identifier: ModuleID::ChargingProfiles,
            url: Url::try_from("https://example.com/ocpi/cpo/2.1.1/chargingprofiles/").unwrap(),
        };
        assert!(!profiles.is_valid_2_1_1());

        let details = VersionDetails {
            version: VersionNumber::V2_1_1,
            endpoints: vec![
                Endpoint {
                    identifier: ModuleID::Sessions,
                    url: Url::try_from("https://example.com/ocpi/cpo/2.1.1/sessions/").unwrap(),
                },
                hub,
            ],
        };
        assert!(!details.endpoints_valid_2_1_1());
    }
}

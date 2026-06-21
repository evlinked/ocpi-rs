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

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::common::{
    BusinessDetails, CiString2, CiString3, CiString36, DisplayText, EnergyMix, Url,
};
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

// ── TariffDimensionType ───────────────────────────────────────────────────────

/// The dimension a 2.1.1 [`PriceComponent`] is priced against.
///
/// OCPI 2.1.1 defines exactly these four; later versions add more. Identical to
/// the 2.2.1 enum, but defined here so the 2.1.1 Tariffs surface is complete
/// and version-pinned.
///
/// Spec: OCPI 2.1.1 — *Tariffs* module, `TariffDimensionType`
/// (`specs/ocpi/2.1.1/OCPI_2.1.1.pdf`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TariffDimensionType {
    /// Defined in kWh, `step_size` multiplier: 1 Wh.
    Energy,
    /// Flat fee, no unit.
    Flat,
    /// Time not charging: defined in hours, `step_size` multiplier: 1 second.
    ParkingTime,
    /// Time charging: defined in hours, `step_size` multiplier: 1 second.
    Time,
}

// ── PriceComponent ────────────────────────────────────────────────────────────

/// A single priced dimension within a 2.1.1 [`TariffElement`].
///
/// ## Delta from 2.2.1
///
/// The 2.1.1 `PriceComponent` has **no `vat`** field — per-component VAT was
/// added in 2.2.1.
///
/// Spec: OCPI 2.1.1 — *Tariffs* module, `PriceComponent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriceComponent {
    /// The dimension that this component applies to.
    ///
    /// Field renamed from spec `type` (Rust keyword).
    #[serde(rename = "type")]
    pub component_type: TariffDimensionType,
    /// Price per unit (excl. VAT) for this component.
    pub price: f64,
    /// Minimum amount to be billed. Unit depends on the `component_type`.
    pub step_size: u32,
}

// ── TariffRestrictions ────────────────────────────────────────────────────────

/// Conditions under which a 2.1.1 [`TariffElement`] applies.
///
/// ## Delta from 2.2.1
///
/// The 2.1.1 restriction set has **no `min_current` / `max_current`** (added in
/// 2.2) and **no `reservation`** restriction (added in 2.2.1). It is the sole
/// place validity windows live in 2.1.1 — the `Tariff` object itself carries no
/// `start_date_time` / `end_date_time`.
///
/// Spec: OCPI 2.1.1 — *Tariffs* module, `TariffRestrictions`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TariffRestrictions {
    /// Start time of day in local time, applies every day, format: `HH:MM` (24h).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub start_time: Option<String>,
    /// End time of day in local time, applies every day, format: `HH:MM` (24h).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub end_time: Option<String>,
    /// Start date, applies from this day (inclusive), format: `YYYY-MM-DD`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub start_date: Option<String>,
    /// End date, applies until this day (inclusive), format: `YYYY-MM-DD`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub end_date: Option<String>,
    /// Minimum consumed energy (kWh) — inclusive.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub min_kwh: Option<f64>,
    /// Maximum consumed energy (kWh) — exclusive.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max_kwh: Option<f64>,
    /// Minimum power (kW) over the entire charging session — inclusive.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub min_power: Option<f64>,
    /// Maximum power (kW) over the entire charging session — exclusive.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max_power: Option<f64>,
    /// Minimum duration (seconds) of the charging session — inclusive.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub min_duration: Option<u32>,
    /// Maximum duration (seconds) of the charging session — exclusive.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max_duration: Option<u32>,
    /// Applicable days of the week. Reuses the version-agnostic
    /// [`crate::v2_2_1::DayOfWeek`] (the Mon–Sun enum is identical across versions).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub day_of_week: Vec<crate::v2_2_1::DayOfWeek>,
}

// ── TariffElement ─────────────────────────────────────────────────────────────

/// A group of price components with optional restrictions on when they apply.
///
/// Spec: OCPI 2.1.1 — *Tariffs* module, `TariffElement`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TariffElement {
    /// List of price components that make up the pricing of this tariff.
    pub price_components: Vec<PriceComponent>,
    /// Optional restrictions that constrain when this element applies.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub restrictions: Option<TariffRestrictions>,
}

// ── Tariff ────────────────────────────────────────────────────────────────────

/// An OCPI **2.1.1** Tariff — the pricing a CPO advertises for using its EVSEs.
///
/// ## Deltas from the 2.2.1 [`crate::v2_2_1::Tariff`]
///
/// - **No** `country_code` / `party_id` (added in 2.2).
/// - **No** `type: TariffType` (ad-hoc/profile tariff typing is 2.2).
/// - **No** `min_price` / `max_price` (added in 2.2.1).
/// - **No** top-level `start_date_time` / `end_date_time` — validity windows
///   live only inside [`TariffRestrictions`] in 2.1.1.
///
/// Spec: OCPI 2.1.1 — *Tariffs* module, *Object description*
/// (`specs/ocpi/2.1.1/OCPI_2.1.1.pdf`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tariff {
    /// Unique tariff ID within the CPO's platform.
    pub id: CiString36,
    /// ISO 4217 currency code.
    pub currency: String,
    /// Human-readable name(s) or description(s) in various languages.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tariff_alt_text: Vec<DisplayText>,
    /// Alternative URL to a tariff information web page.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tariff_alt_url: Option<Url>,
    /// List of tariff elements.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub elements: Vec<TariffElement>,
    /// Details on the energy supplied with this tariff.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub energy_mix: Option<EnergyMix>,
    /// Timestamp of the last update (or creation) of this tariff.
    pub last_updated: DateTime<Utc>,
}

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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::Image;
    use crate::v2_2_1::DayOfWeek;

    #[test]
    fn tariff_dimension_type_serde_roundtrip() {
        for (ty, wire) in [
            (TariffDimensionType::Energy, "\"ENERGY\""),
            (TariffDimensionType::Flat, "\"FLAT\""),
            (TariffDimensionType::ParkingTime, "\"PARKING_TIME\""),
            (TariffDimensionType::Time, "\"TIME\""),
        ] {
            assert_eq!(serde_json::to_string(&ty).unwrap(), wire);
            let back: TariffDimensionType = serde_json::from_str(wire).unwrap();
            assert_eq!(back, ty);
        }
    }

    #[test]
    fn price_component_has_no_vat() {
        let pc = PriceComponent {
            component_type: TariffDimensionType::Energy,
            price: 0.25,
            step_size: 1,
        };
        let json = serde_json::to_string(&pc).unwrap();
        assert!(
            !json.contains("vat"),
            "2.1.1 PriceComponent must not carry `vat`: {json}"
        );
    }

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
    fn tariff_serde_spec_example() {
        // Ported from the OCPI 2.1.1 spec Tariff example: a simple energy +
        // parking tariff with a time-window restriction. Note: no country_code/
        // party_id/type/min_price/max_price, and no top-level validity dates.
        // Spec ref: specs/ocpi/2.1.1/OCPI_2.1.1.pdf, "Tariffs" chapter.
        let json = r#"{
            "id": "12",
            "currency": "EUR",
            "tariff_alt_text": [
                {"language": "en", "text": "2.00 euro p/hour"}
            ],
            "elements": [
                {
                    "price_components": [
                        {"type": "TIME", "price": 2.00, "step_size": 300}
                    ]
                },
                {
                    "price_components": [
                        {"type": "PARKING_TIME", "price": 3.00, "step_size": 60}
                    ],
                    "restrictions": {
                        "start_time": "09:00",
                        "end_time": "18:00",
                        "day_of_week": ["MONDAY", "TUESDAY"]
                    }
                }
            ],
            "last_updated": "2018-12-17T11:36:01Z"
        }"#;
        let tariff: Tariff = serde_json::from_str(json).unwrap();
        assert_eq!(tariff.id.as_str(), "12");
        assert_eq!(tariff.currency, "EUR");
        assert_eq!(tariff.elements.len(), 2);
        assert_eq!(
            tariff.elements[0].price_components[0].component_type,
            TariffDimensionType::Time
        );
        let restr = tariff.elements[1].restrictions.as_ref().unwrap();
        assert_eq!(restr.start_time.as_deref(), Some("09:00"));
        assert_eq!(
            restr.day_of_week,
            vec![DayOfWeek::Monday, DayOfWeek::Tuesday]
        );

        let back: Tariff = serde_json::from_str(&serde_json::to_string(&tariff).unwrap()).unwrap();
        assert_eq!(back, tariff);
    }

    #[test]
    fn tariff_wire_form_omits_2_2_fields() {
        let tariff = Tariff {
            id: CiString36::try_from("12").unwrap(),
            currency: "EUR".into(),
            tariff_alt_text: Vec::new(),
            tariff_alt_url: None,
            elements: vec![TariffElement {
                price_components: vec![PriceComponent {
                    component_type: TariffDimensionType::Flat,
                    price: 1.0,
                    step_size: 1,
                }],
                restrictions: None,
            }],
            energy_mix: None,
            last_updated: "2018-12-17T11:36:01Z".parse().unwrap(),
        };
        let json = serde_json::to_string(&tariff).unwrap();
        // The Tariff object itself carries none of these 2.2/2.2.1 additions.
        // (`type` is not checked by substring here because a nested
        // PriceComponent legitimately uses the `type` key; the absence of a
        // Tariff-level `tariff_type` field is guaranteed structurally.)
        for absent in [
            "country_code",
            "party_id",
            "min_price",
            "max_price",
            "start_date_time",
            "end_date_time",
        ] {
            assert!(
                !json.contains(absent),
                "2.1.1 Tariff must not carry {absent}: {json}"
            );
        }
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

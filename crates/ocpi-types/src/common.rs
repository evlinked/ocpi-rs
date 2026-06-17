//! Common data types shared across OCPI modules.
//!
//! This is a foundational subset (milestone M1). Additional shared types are
//! added as their owning modules are implemented.

use std::fmt;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use crate::OcpiError;

// ── Role ──────────────────────────────────────────────────────────────────────

/// Party / platform role in the OCPI ecosystem.
///
/// Spec: `specs/ocpi/2.2.1/types.asciidoc` — Role enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Role {
    /// Charge Point Operator.
    #[serde(rename = "CPO")]
    Cpo,
    /// eMobility Service Provider.
    #[serde(rename = "EMSP")]
    Emsp,
    /// Hub.
    Hub,
    /// National Access Point.
    #[serde(rename = "NAP")]
    Nap,
    /// Navigation Service Provider.
    #[serde(rename = "NSP")]
    Nsp,
    /// Other role.
    Other,
    /// Smart Charging Service Provider.
    #[serde(rename = "SCSP")]
    Scsp,
}

// ── CiString<N> ───────────────────────────────────────────────────────────────

/// A validated case-insensitive ASCII string of at most `MAX` bytes.
///
/// OCPI spec: "Only printable ASCII allowed" (U+0020–U+007E). The `MAX`
/// const parameter enforces the per-field length constraint at construction
/// time. Case-insensitivity is a *comparison* property — the stored value
/// preserves the original casing; callers normalize when comparing.
///
/// Spec: `specs/ocpi/2.2.1/types.asciidoc` — CiString type.
#[derive(Debug, Clone, Eq)]
pub struct CiString<const MAX: usize>(String);

impl<const MAX: usize> CiString<MAX> {
    /// The stored string value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<const MAX: usize> PartialEq for CiString<MAX> {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq_ignore_ascii_case(&other.0)
    }
}

impl<const MAX: usize> Hash for CiString<MAX> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_ascii_lowercase().hash(state);
    }
}

impl<const MAX: usize> TryFrom<&str> for CiString<MAX> {
    type Error = OcpiError;

    fn try_from(s: &str) -> Result<Self, OcpiError> {
        if s.len() > MAX {
            return Err(OcpiError::Invalid(format!(
                "CiString<{MAX}>: value too long ({} bytes)",
                s.len()
            )));
        }
        if !s.bytes().all(|b| (0x20..=0x7E).contains(&b)) {
            return Err(OcpiError::Invalid(
                "CiString: only printable ASCII (U+0020–U+007E) allowed".into(),
            ));
        }
        Ok(Self(s.to_owned()))
    }
}

impl<const MAX: usize> TryFrom<String> for CiString<MAX> {
    type Error = OcpiError;

    fn try_from(s: String) -> Result<Self, OcpiError> {
        if s.len() > MAX {
            return Err(OcpiError::Invalid(format!(
                "CiString<{MAX}>: value too long ({} bytes)",
                s.len()
            )));
        }
        if !s.bytes().all(|b| (0x20..=0x7E).contains(&b)) {
            return Err(OcpiError::Invalid(
                "CiString: only printable ASCII (U+0020–U+007E) allowed".into(),
            ));
        }
        Ok(Self(s))
    }
}

impl<const MAX: usize> fmt::Display for CiString<MAX> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<const MAX: usize> Serialize for CiString<MAX> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de, const MAX: usize> Deserialize<'de> for CiString<MAX> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::try_from(s).map_err(serde::de::Error::custom)
    }
}

/// `CiString(2)` — ISO 3166-1 alpha-2 country code and similar 2-char fields.
pub type CiString2 = CiString<2>;
/// `CiString(3)` — eMI3 party identifier and similar 3-char fields.
pub type CiString3 = CiString<3>;
/// `CiString(36)` — UUIDs and similar medium-length identifiers.
pub type CiString36 = CiString<36>;
/// `CiString(39)` — CDR IDs (allows a credit-CDR suffix beyond the usual 36).
pub type CiString39 = CiString<39>;
/// `CiString(48)` — EVSE IDs (eMI3 EVSE ID standard, up to 48 chars).
pub type CiString48 = CiString<48>;
/// `CiString(64)` — token display numbers and similar 64-char case-insensitive fields.
pub type CiString64 = CiString<64>;
/// `CiString(255)` — general-purpose, matches the URL field limit.
pub type CiString255 = CiString<255>;

// ── Url ───────────────────────────────────────────────────────────────────────

/// An OCPI URL: a `string(255)` following the W3C URI spec.
///
/// Validates only the length constraint (≤ 255 bytes). Full URI syntax
/// validation is out of scope at this stage.
///
/// Spec: `specs/ocpi/2.2.1/types.asciidoc` — URL type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Url(String);

impl Url {
    /// The raw URL string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for Url {
    type Error = OcpiError;

    fn try_from(s: &str) -> Result<Self, OcpiError> {
        if s.len() > 255 {
            return Err(OcpiError::Invalid(format!(
                "URL exceeds 255 bytes ({} bytes)",
                s.len()
            )));
        }
        Ok(Self(s.to_owned()))
    }
}

impl TryFrom<String> for Url {
    type Error = OcpiError;

    fn try_from(s: String) -> Result<Self, OcpiError> {
        if s.len() > 255 {
            return Err(OcpiError::Invalid(format!(
                "URL exceeds 255 bytes ({} bytes)",
                s.len()
            )));
        }
        Ok(Self(s))
    }
}

impl fmt::Display for Url {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for Url {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Url {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::try_from(s).map_err(serde::de::Error::custom)
    }
}

// ── Pre-existing types ────────────────────────────────────────────────────────

/// Human-readable text in a specific language (OCPI `DisplayText`).
///
/// Spec: `specs/ocpi/2.2.1/types.asciidoc` — DisplayText class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayText {
    /// ISO 639-1 language code (e.g. `en`, `vi`). Max 2 characters.
    pub language: String,
    /// The text to be displayed (max 512 characters per the spec).
    pub text: String,
}

impl DisplayText {
    /// Validates field length constraints from the spec.
    ///
    /// # Errors
    ///
    /// Returns [`OcpiError::Invalid`] if `language` exceeds 2 characters or
    /// `text` exceeds 512 characters.
    pub fn validate(&self) -> Result<(), OcpiError> {
        if self.language.len() > 2 {
            return Err(OcpiError::Invalid(
                "DisplayText.language must be at most 2 characters (ISO 639-1)".into(),
            ));
        }
        if self.text.len() > 512 {
            return Err(OcpiError::Invalid(format!(
                "DisplayText.text must be at most 512 characters ({} given)",
                self.text.len()
            )));
        }
        Ok(())
    }
}

/// A geographic location in decimal degrees (OCPI `GeoLocation`).
///
/// Latitude and longitude are strings per the specification. The geodetic
/// system is WGS 84.
///
/// Spec: `specs/ocpi/2.2.1/mod_locations.asciidoc` — GeoLocation class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoLocation {
    /// Latitude in decimal degrees (e.g. `"50.770774"`). Max 10 chars.
    pub latitude: String,
    /// Longitude in decimal degrees (e.g. `"-126.104965"`). Max 11 chars.
    pub longitude: String,
}

impl GeoLocation {
    /// Validates the coordinate format required by the spec.
    ///
    /// Latitude must match `-?[0-9]{1,2}\.[0-9]{5,7}` and fit in 10 chars.
    /// Longitude must match `-?[0-9]{1,3}\.[0-9]{5,7}` and fit in 11 chars.
    ///
    /// # Errors
    ///
    /// Returns [`OcpiError::Invalid`] if either coordinate fails validation.
    pub fn validate(&self) -> Result<(), OcpiError> {
        if !is_valid_latitude(&self.latitude) {
            return Err(OcpiError::Invalid(format!(
                "GeoLocation.latitude '{}' is invalid; expected format: -?[0-9]{{1,2}}.[0-9]{{5,7}}, max 10 chars",
                self.latitude
            )));
        }
        if !is_valid_longitude(&self.longitude) {
            return Err(OcpiError::Invalid(format!(
                "GeoLocation.longitude '{}' is invalid; expected format: -?[0-9]{{1,3}}.[0-9]{{5,7}}, max 11 chars",
                self.longitude
            )));
        }
        Ok(())
    }
}

fn is_valid_coord(s: &str, max_len: usize, max_int_digits: usize) -> bool {
    if s.len() > max_len {
        return false;
    }
    let s = s.strip_prefix('-').unwrap_or(s);
    let Some((int_part, frac_part)) = s.split_once('.') else {
        return false;
    };
    !int_part.is_empty()
        && int_part.len() <= max_int_digits
        && int_part.bytes().all(|b| b.is_ascii_digit())
        && (5..=7).contains(&frac_part.len())
        && frac_part.bytes().all(|b| b.is_ascii_digit())
}

fn is_valid_latitude(s: &str) -> bool {
    is_valid_coord(s, 10, 2)
}

fn is_valid_longitude(s: &str) -> bool {
    is_valid_coord(s, 11, 3)
}

// ── Price ─────────────────────────────────────────────────────────────────────

/// Price with and without VAT.
///
/// Spec: `specs/ocpi/2.2.1/types.asciidoc` — Price class.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Price {
    /// Price/cost excluding VAT.
    pub excl_vat: f64,
    /// Price/cost including VAT (optional).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub incl_vat: Option<f64>,
}

// ── EnergySourceCategory ──────────────────────────────────────────────────────

/// Categories of energy sources.
///
/// Spec: `specs/ocpi/2.2.1/mod_locations.asciidoc` — EnergySourceCategory enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EnergySourceCategory {
    /// Nuclear power sources.
    Nuclear,
    /// All kinds of fossil power sources.
    GeneralFossil,
    /// Fossil power from coal.
    Coal,
    /// Fossil power from gas.
    Gas,
    /// All kinds of regenerative power sources.
    GeneralGreen,
    /// Regenerative power from photovoltaic panels.
    Solar,
    /// Regenerative power from wind turbines.
    Wind,
    /// Regenerative power from water turbines.
    Water,
}

// ── EnergySource ──────────────────────────────────────────────────────────────

/// A key-value pair of energy source type and its percentage in the mix.
///
/// All entries' `percentage` values should sum to 100.
///
/// Spec: `specs/ocpi/2.2.1/mod_locations.asciidoc` — EnergySource class.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnergySource {
    /// The type of energy source.
    pub source: EnergySourceCategory,
    /// Percentage of this source (0–100) in the mix.
    pub percentage: f64,
}

// ── EnvironmentalImpactCategory ───────────────────────────────────────────────

/// Categories of environmental impact values.
///
/// Spec: `specs/ocpi/2.2.1/mod_locations.asciidoc` — EnvironmentalImpactCategory enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EnvironmentalImpactCategory {
    /// Produced nuclear waste in grams per kilowatthour.
    NuclearWaste,
    /// Exhausted carbon dioxide in grams per kilowatthour.
    CarbonDioxide,
}

// ── EnvironmentalImpact ───────────────────────────────────────────────────────

/// Amount of waste produced or emitted per kWh.
///
/// Spec: `specs/ocpi/2.2.1/mod_locations.asciidoc` — EnvironmentalImpact class.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentalImpact {
    /// The environmental impact category of this value.
    pub category: EnvironmentalImpactCategory,
    /// Amount of this portion in grams per kilowatthour.
    pub amount: f64,
}

// ── EnergyMix ─────────────────────────────────────────────────────────────────

/// Energy mix and environmental impact of the energy supplied at a location or
/// in a tariff.
///
/// Spec: `specs/ocpi/2.2.1/mod_locations.asciidoc` — EnergyMix class.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnergyMix {
    /// True if 100% from regenerative sources (CO₂ and nuclear waste are zero).
    pub is_green_energy: bool,
    /// Energy sources making up this mix. Percentages should sum to 100.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub energy_sources: Vec<EnergySource>,
    /// Nuclear waste and CO₂ exhaust values.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environ_impact: Vec<EnvironmentalImpact>,
    /// Name of the energy supplier (max 64 chars).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub supplier_name: Option<String>,
    /// Name of the energy supplier's product or tariff plan (max 64 chars).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub energy_product_name: Option<String>,
}

/// Logo / image reference (OCPI `Image`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Image {
    /// URL from which the image can be fetched.
    pub url: String,
    /// Optional URL of a thumbnail-sized version.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub thumbnail: Option<String>,
    /// Image category (e.g. `OPERATOR`, `LOCATION`).
    pub category: String,
    /// Image file type (e.g. `png`, `jpeg`).
    #[serde(rename = "type")]
    pub image_type: String,
    /// Optional width in pixels.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub width: Option<u32>,
    /// Optional height in pixels.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub height: Option<u32>,
}

/// Details of a business / party (OCPI `BusinessDetails`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BusinessDetails {
    /// Name of the operator.
    pub name: String,
    /// Optional link to the operator's website.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub website: Option<String>,
    /// Optional logo of the operator.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub logo: Option<Image>,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Role ──

    #[test]
    fn role_serializes_to_uppercase() {
        assert_eq!(serde_json::to_string(&Role::Cpo).unwrap(), "\"CPO\"");
        assert_eq!(serde_json::to_string(&Role::Emsp).unwrap(), "\"EMSP\"");
        assert_eq!(serde_json::to_string(&Role::Hub).unwrap(), "\"HUB\"");
        assert_eq!(serde_json::to_string(&Role::Nap).unwrap(), "\"NAP\"");
        assert_eq!(serde_json::to_string(&Role::Nsp).unwrap(), "\"NSP\"");
        assert_eq!(serde_json::to_string(&Role::Other).unwrap(), "\"OTHER\"");
        assert_eq!(serde_json::to_string(&Role::Scsp).unwrap(), "\"SCSP\"");
    }

    #[test]
    fn role_roundtrip() {
        for role in [
            Role::Cpo,
            Role::Emsp,
            Role::Hub,
            Role::Nap,
            Role::Nsp,
            Role::Other,
            Role::Scsp,
        ] {
            let json = serde_json::to_string(&role).unwrap();
            let back: Role = serde_json::from_str(&json).unwrap();
            assert_eq!(back, role);
        }
    }

    // ── CiString ──

    #[test]
    fn ci_string_accepts_valid_ascii() {
        let s = CiString3::try_from("NL").unwrap();
        assert_eq!(s.as_str(), "NL");
    }

    #[test]
    fn ci_string_accepts_max_length() {
        let s = CiString3::try_from("TNM").unwrap();
        assert_eq!(s.as_str(), "TNM");
    }

    #[test]
    fn ci_string_rejects_too_long() {
        let err = CiString3::try_from("TOOLONG").unwrap_err();
        assert!(matches!(err, OcpiError::Invalid(_)));
    }

    #[test]
    fn ci_string_rejects_non_printable_ascii() {
        let err = CiString3::try_from("A\nB").unwrap_err();
        assert!(matches!(err, OcpiError::Invalid(_)));
    }

    #[test]
    fn ci_string_rejects_non_ascii() {
        let err = CiString3::try_from("Ñ").unwrap_err();
        assert!(matches!(err, OcpiError::Invalid(_)));
    }

    #[test]
    fn ci_string_equality_is_case_insensitive() {
        let a = CiString3::try_from("TNM").unwrap();
        let b = CiString3::try_from("tnm").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn ci_string_serde_roundtrip() {
        let s = CiString3::try_from("CPO").unwrap();
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, "\"CPO\"");
        let back: CiString3 = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn ci_string_serde_rejects_too_long() {
        let err = serde_json::from_str::<CiString2>("\"TOOLONG\"");
        assert!(err.is_err());
    }

    // ── Url ──

    #[test]
    fn url_accepts_valid() {
        let u = Url::try_from("https://example.com/ocpi/cpo/2.2.1/").unwrap();
        assert_eq!(u.as_str(), "https://example.com/ocpi/cpo/2.2.1/");
    }

    #[test]
    fn url_accepts_255_chars() {
        let s = "https://".to_owned() + &"a".repeat(247);
        assert_eq!(s.len(), 255);
        assert!(Url::try_from(s).is_ok());
    }

    #[test]
    fn url_rejects_256_chars() {
        let s = "https://".to_owned() + &"a".repeat(248);
        assert_eq!(s.len(), 256);
        let err = Url::try_from(s).unwrap_err();
        assert!(matches!(err, OcpiError::Invalid(_)));
    }

    #[test]
    fn url_serde_roundtrip() {
        let u = Url::try_from("https://example.com/").unwrap();
        let json = serde_json::to_string(&u).unwrap();
        assert_eq!(json, "\"https://example.com/\"");
        let back: Url = serde_json::from_str(&json).unwrap();
        assert_eq!(back, u);
    }

    #[test]
    fn url_serde_rejects_too_long() {
        let long = format!("\"https://{}\"", "a".repeat(250));
        assert!(serde_json::from_str::<Url>(&long).is_err());
    }

    // ── DisplayText::validate ──

    #[test]
    fn display_text_validate_accepts_valid() {
        let dt = DisplayText {
            language: "en".into(),
            text: "Hello".into(),
        };
        assert!(dt.validate().is_ok());
    }

    #[test]
    fn display_text_validate_rejects_long_language() {
        let dt = DisplayText {
            language: "eng".into(),
            text: "Hello".into(),
        };
        let err = dt.validate().unwrap_err();
        assert!(matches!(err, OcpiError::Invalid(_)));
    }

    #[test]
    fn display_text_validate_rejects_long_text() {
        let dt = DisplayText {
            language: "en".into(),
            text: "x".repeat(513),
        };
        let err = dt.validate().unwrap_err();
        assert!(matches!(err, OcpiError::Invalid(_)));
    }

    #[test]
    fn display_text_validate_accepts_512_char_text() {
        let dt = DisplayText {
            language: "vi".into(),
            text: "x".repeat(512),
        };
        assert!(dt.validate().is_ok());
    }

    // ── GeoLocation::validate ──

    #[test]
    fn geo_location_validate_accepts_valid() {
        let g = GeoLocation {
            latitude: "50.770774".into(),
            longitude: "-126.104965".into(),
        };
        assert!(g.validate().is_ok());
    }

    #[test]
    fn geo_location_validate_accepts_positive_longitude() {
        let g = GeoLocation {
            latitude: "51.047599".into(),
            longitude: "3.729596".into(),
        };
        assert!(g.validate().is_ok());
    }

    #[test]
    fn geo_location_validate_rejects_bad_latitude_few_decimals() {
        let g = GeoLocation {
            latitude: "50.7707".into(), // only 4 decimal places — need ≥5
            longitude: "3.729596".into(),
        };
        assert!(g.validate().is_err());
    }

    #[test]
    fn geo_location_validate_rejects_bad_latitude_no_dot() {
        let g = GeoLocation {
            latitude: "50770774".into(),
            longitude: "3.729596".into(),
        };
        assert!(g.validate().is_err());
    }

    #[test]
    fn geo_location_validate_rejects_latitude_too_long() {
        // 11 chars — exceeds max 10
        let g = GeoLocation {
            latitude: "50.7707741".into(),
            longitude: "3.729596".into(),
        };
        // "50.7707741" is 10 chars, borderline OK
        assert!(g.validate().is_ok());
        let g2 = GeoLocation {
            latitude: "50.77077411".into(), // 11 chars
            longitude: "3.729596".into(),
        };
        assert!(g2.validate().is_err());
    }

    #[test]
    fn geo_location_validate_rejects_bad_longitude_too_many_int_digits() {
        // longitude int part max 3 digits
        let g = GeoLocation {
            latitude: "50.770774".into(),
            longitude: "1234.729596".into(), // 4 int digits
        };
        assert!(g.validate().is_err());
    }

    // ── Price ──

    #[test]
    fn price_roundtrip_with_incl_vat() {
        let p = Price {
            excl_vat: 4.0,
            incl_vat: Some(4.84),
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: Price = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn price_roundtrip_excl_only() {
        let p = Price {
            excl_vat: 2.5,
            incl_vat: None,
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(!json.contains("incl_vat"));
        let back: Price = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn price_deserializes_from_spec_values() {
        let json = r#"{"excl_vat": 4.0, "incl_vat": 4.84}"#;
        let p: Price = serde_json::from_str(json).unwrap();
        assert!((p.excl_vat - 4.0).abs() < f64::EPSILON);
        assert!((p.incl_vat.unwrap() - 4.84).abs() < 1e-9);
    }

    // ── EnergySourceCategory ──

    #[test]
    fn energy_source_category_serializes_screaming_snake() {
        assert_eq!(
            serde_json::to_string(&EnergySourceCategory::GeneralFossil).unwrap(),
            "\"GENERAL_FOSSIL\""
        );
        assert_eq!(
            serde_json::to_string(&EnergySourceCategory::GeneralGreen).unwrap(),
            "\"GENERAL_GREEN\""
        );
        assert_eq!(
            serde_json::to_string(&EnergySourceCategory::Nuclear).unwrap(),
            "\"NUCLEAR\""
        );
    }

    #[test]
    fn energy_source_category_roundtrip() {
        for cat in [
            EnergySourceCategory::Nuclear,
            EnergySourceCategory::GeneralFossil,
            EnergySourceCategory::Coal,
            EnergySourceCategory::Gas,
            EnergySourceCategory::GeneralGreen,
            EnergySourceCategory::Solar,
            EnergySourceCategory::Wind,
            EnergySourceCategory::Water,
        ] {
            let json = serde_json::to_string(&cat).unwrap();
            let back: EnergySourceCategory = serde_json::from_str(&json).unwrap();
            assert_eq!(back, cat);
        }
    }

    // ── EnergySource ──

    #[test]
    fn energy_source_roundtrip() {
        let es = EnergySource {
            source: EnergySourceCategory::Solar,
            percentage: 45.0,
        };
        let json = serde_json::to_string(&es).unwrap();
        let back: EnergySource = serde_json::from_str(&json).unwrap();
        assert_eq!(back, es);
    }

    // ── EnvironmentalImpactCategory ──

    #[test]
    fn environmental_impact_category_serializes_screaming_snake() {
        assert_eq!(
            serde_json::to_string(&EnvironmentalImpactCategory::NuclearWaste).unwrap(),
            "\"NUCLEAR_WASTE\""
        );
        assert_eq!(
            serde_json::to_string(&EnvironmentalImpactCategory::CarbonDioxide).unwrap(),
            "\"CARBON_DIOXIDE\""
        );
    }

    // ── EnvironmentalImpact ──

    #[test]
    fn environmental_impact_roundtrip() {
        let ei = EnvironmentalImpact {
            category: EnvironmentalImpactCategory::CarbonDioxide,
            amount: 372.0,
        };
        let json = serde_json::to_string(&ei).unwrap();
        let back: EnvironmentalImpact = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ei);
    }

    // ── EnergyMix ──

    #[test]
    fn energy_mix_simple_green_roundtrip() {
        let m = EnergyMix {
            is_green_energy: true,
            energy_sources: vec![],
            environ_impact: vec![],
            supplier_name: None,
            energy_product_name: None,
        };
        let json = serde_json::to_string(&m).unwrap();
        assert!(!json.contains("energy_sources"));
        assert!(!json.contains("environ_impact"));
        let back: EnergyMix = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn energy_mix_complete_roundtrip() {
        let m = EnergyMix {
            is_green_energy: false,
            energy_sources: vec![
                EnergySource {
                    source: EnergySourceCategory::Wind,
                    percentage: 70.0,
                },
                EnergySource {
                    source: EnergySourceCategory::Solar,
                    percentage: 30.0,
                },
            ],
            environ_impact: vec![EnvironmentalImpact {
                category: EnvironmentalImpactCategory::CarbonDioxide,
                amount: 10.3,
            }],
            supplier_name: Some("Green Power Co.".into()),
            energy_product_name: Some("Wind100".into()),
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: EnergyMix = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn energy_mix_missing_optional_arrays_deserialize_as_empty() {
        let json = r#"{"is_green_energy": true}"#;
        let m: EnergyMix = serde_json::from_str(json).unwrap();
        assert!(m.energy_sources.is_empty());
        assert!(m.environ_impact.is_empty());
        assert!(m.supplier_name.is_none());
        assert!(m.energy_product_name.is_none());
    }
}

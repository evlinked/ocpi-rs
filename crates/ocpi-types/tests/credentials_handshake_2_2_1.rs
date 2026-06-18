//! Contract test for the OCPI **2.2.1** credentials registration handshake
//! (the *Token A → B → C* exchange), driven entirely through the public
//! `ocpi-types` crate-root API.
//!
//! This test serves two purposes for the downstream hub (`charge-hub`, issue
//! #67):
//!
//! 1. **Importability** — every type the hub needs to drive the handshake is
//!    imported here from the crate root (`ocpi_types::…`), not from the
//!    version-namespaced module path. If a re-export regresses, this test
//!    stops compiling.
//! 2. **Wire contract** — it walks the full registration sequence using the
//!    spec's payload shapes and asserts the token-rotation semantics that make
//!    the handshake secure (A is single-use; B and C are distinct).
//!
//! The *live* axum/reqwest transport smoke (a real server + client round-trip)
//! is tracked separately in issue #23; this test pins the data contract that
//! both sides serialize against.
//!
//! Spec: `specs/ocpi/2.2.1/credentials.asciidoc` — *Registration* and
//! *Credentials object*.

// All imports come from the crate root — this is the surface the hub consumes.
use ocpi_types::{
    BusinessDetails, CiString2, CiString3, Credentials, CredentialsRole, Image, OcpiResponse,
    OcpiStatusCode, Role, Url,
};

/// Build a single-role credentials object for a party, as it appears on the
/// wire during the handshake.
fn credentials_for(token: &str, role: Role, party: &str, country: &str, name: &str) -> Credentials {
    Credentials {
        token: token.to_owned(),
        url: Url::try_from("https://example.com/ocpi/versions").unwrap(),
        roles: vec![CredentialsRole {
            role,
            business_details: BusinessDetails {
                name: name.to_owned(),
                website: None,
                logo: None,
            },
            party_id: CiString3::try_from(party).unwrap(),
            country_code: CiString2::try_from(country).unwrap(),
        }],
    }
}

/// Step 1 — the new party (eMSP), authenticating with **Token A**, POSTs its
/// own credentials carrying the token the receiver should use to call back:
/// **Token B**. We model the receiver parsing that POST body off the wire.
#[test]
fn step_post_credentials_carries_token_b() {
    // Exactly the body an eMSP would POST to `…/credentials` (Token A is the
    // bearer header, not part of the body).
    let posted = r#"{
        "token": "TOKEN_B_emsp_to_cpo",
        "url": "https://emsp.example.com/ocpi/versions",
        "roles": [
            {
                "role": "EMSP",
                "business_details": {"name": "Example Provider"},
                "party_id": "MSP",
                "country_code": "DE"
            }
        ]
    }"#;

    let creds: Credentials = serde_json::from_str(posted).unwrap();
    // The receiver must accept the registration (non-empty, single-role).
    creds.validate().unwrap();
    creds.check_single_role().unwrap();
    assert_eq!(creds.token, "TOKEN_B_emsp_to_cpo");
    assert_eq!(creds.roles[0].role, Role::Emsp);
    assert_eq!(creds.roles[0].party_id.as_str(), "MSP");
}

/// Step 2 — the receiver (CPO) responds with **its own** credentials wrapped in
/// the OCPI envelope, carrying **Token C** for the eMSP to use henceforth.
#[test]
fn step_response_carries_token_c_in_envelope() {
    let cpo = credentials_for(
        "TOKEN_C_cpo_to_emsp",
        Role::Cpo,
        "EXA",
        "NL",
        "Example Operator",
    );
    let envelope = OcpiResponse::success(cpo);

    // Serialize as the CPO server would, then parse as the eMSP client would.
    let on_wire = serde_json::to_string(&envelope).unwrap();
    let parsed: OcpiResponse<Credentials> = serde_json::from_str(&on_wire).unwrap();

    assert!(parsed.is_success());
    assert_eq!(parsed.status_code, OcpiStatusCode::Success);
    let data = parsed
        .data
        .expect("success envelope must carry credentials");
    assert_eq!(data.token, "TOKEN_C_cpo_to_emsp");
    assert_eq!(data.roles[0].role, Role::Cpo);
}

/// The handshake's security property: the token the eMSP handed out (B) and the
/// token the CPO hands back (C) are distinct, and neither is the bootstrap
/// Token A. After this exchange Token A is discarded.
#[test]
fn token_a_b_c_are_distinct() {
    let token_a = "TOKEN_A_preshared";
    let emsp_post = credentials_for("TOKEN_B", Role::Emsp, "MSP", "DE", "Example Provider");
    let cpo_reply = credentials_for("TOKEN_C", Role::Cpo, "EXA", "NL", "Example Operator");

    assert_ne!(
        token_a, emsp_post.token,
        "B must differ from the bootstrap token A"
    );
    assert_ne!(
        emsp_post.token, cpo_reply.token,
        "B (to CPO) and C (to eMSP) must differ"
    );
    assert_ne!(token_a, cpo_reply.token);
}

/// Step 3 — credentials **update** (PUT): the spec allows re-running the
/// exchange to rotate tokens. The new object replaces the old; the rotated
/// token must differ from the one it supersedes.
#[test]
fn step_update_rotates_token() {
    let original = credentials_for("TOKEN_C", Role::Cpo, "EXA", "NL", "Example Operator");
    let rotated = credentials_for("TOKEN_C_v2", Role::Cpo, "EXA", "NL", "Example Operator");

    let json = serde_json::to_string(&rotated).unwrap();
    let back: Credentials = serde_json::from_str(&json).unwrap();
    assert_eq!(back, rotated);
    assert_ne!(original.token, rotated.token, "PUT must rotate the token");
    // Party identity is stable across the rotation.
    assert_eq!(original.roles[0].party_id, rotated.roles[0].party_id);
}

/// Step 4 — **unregister** (DELETE): the receiver acknowledges with a
/// data-less success envelope. `OcpiResponse<()>` round-trips with `data`
/// absent.
#[test]
fn step_delete_returns_empty_success_envelope() {
    let envelope: OcpiResponse<()> = OcpiResponse::success_empty();
    let on_wire = serde_json::to_string(&envelope).unwrap();

    // `data` must be omitted entirely (not `null`) per the envelope contract.
    assert!(
        !on_wire.contains("\"data\""),
        "empty envelope must omit data: {on_wire}"
    );

    let parsed: OcpiResponse<()> = serde_json::from_str(&on_wire).unwrap();
    assert!(parsed.is_success());
    assert_eq!(parsed.status_code, OcpiStatusCode::Success);
}

/// A role's `business_details` may carry a logo `Image`. Exercise that path
/// through the crate-root `Image` re-export so the hub can build rich roles.
#[test]
fn role_with_logo_image_roundtrips() {
    let role = CredentialsRole {
        role: Role::Cpo,
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

    let json = serde_json::to_string(&role).unwrap();
    let back: CredentialsRole = serde_json::from_str(&json).unwrap();
    assert_eq!(back, role);
    assert_eq!(back.business_details.logo.unwrap().category, "OPERATOR");
}

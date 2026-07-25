//! M9 conformance — the version-negotiation matrix (issue #236).
//!
//! [`ocpi_client::negotiate_version`] is the SDK's front door: it filters a
//! partner's advertised `GET /versions` catalogue to the locally-supported set
//! and returns the **highest mutual** version. Its correctness rests on the
//! `VersionNumber: Ord` chain `V2_0 < V2_1_1 < V2_2 < V2_2_1 < V2_3_0 < V3_0`,
//! and on the load-bearing invariant that the two **recognition-only** versions
//! at the ends of that chain — `2.0` (#182) and `3.0` (#219/#223) — are
//! *recognised, parsed and ordered but never selected*.
//!
//! The behaviour was previously pinned by ~8 scattered per-scenario unit tests
//! in `ocpi-client/src/lib.rs`. This file is the consolidated, auditable ledger:
//! one explicit `(remote_advertised × locally-supported) -> Option<selected>`
//! table, plus the two supporting invariants (recognition-only is never in a
//! shipped `supported` set; the full `Ord` chain the matrix depends on). It is
//! the negotiation analogue of the #225/#226 payload-corpus conformance harness.
//!
//! Test-only: exercises the public `negotiate_version` surface exactly as it
//! ships. No source, public-type, or dependency changes.
//!
//! Spec / references:
//! - `crates/ocpi-client/src/lib.rs` — `negotiate_version` (the `.filter().max()`
//!   highest-mutual rule).
//! - `crates/ocpi-types/src/version.rs` — `VersionNumber` and its `Ord` fence.
//! - `specs/ocpi/2.2.1/version_information_endpoint.asciidoc` — the `/versions`
//!   catalogue the client negotiates over.
//! - `specs/ocpi/2.0/README.md` (#182), `specs/ocpi/3.0/README.md` (#219/#223)
//!   — the resolved recognition-only contracts at the ends of the range.

use ocpi_client::negotiate_version;
use ocpi_types::serde_json;
use ocpi_types::version::{Version, VersionNumber};

use VersionNumber::{V2_0, V2_1_1, V2_2, V2_2_1, V2_3_0, V3_0};

/// The two recognition-only versions: parsed and ordered, but no shipped
/// `supported` set may include them, so negotiation can never *select* them.
const RECOGNITION_ONLY: [VersionNumber; 2] = [V2_0, V3_0];

/// Build a spec-shaped `GET /versions` catalogue (`data` array) from a list of
/// versions and deserialize it through real serde — exactly as a partner's
/// response arrives off the wire — so the matrix drives `VersionNumber`'s
/// `Deserialize`/`FromStr` path, not hand-built values.
fn catalogue(versions: &[VersionNumber]) -> Vec<Version> {
    let entries: Vec<String> = versions
        .iter()
        .map(|v| {
            format!(
                r#"{{"version":"{v}","url":"https://partner.example/ocpi/{v}"}}"#,
                v = v.as_str()
            )
        })
        .collect();
    let json = format!("[{}]", entries.join(","));
    serde_json::from_str(&json)
        .unwrap_or_else(|e| panic!("catalogue fixture {versions:?} must deserialize: {e}"))
}

/// One row of the negotiation ledger.
struct Case {
    /// Why this row exists — surfaced in the failure message.
    name: &'static str,
    /// The versions the partner advertises in its `/versions` catalogue.
    remote: &'static [VersionNumber],
    /// The versions the local party (e.g. a hub) actually speaks.
    supported: &'static [VersionNumber],
    /// The highest mutual version, or `None` for an unsupported partner.
    expected: Option<VersionNumber>,
}

/// A mainstream 2.x hub. Deliberately excludes both recognition-only versions.
const HUB_2X: &[VersionNumber] = &[V2_1_1, V2_2, V2_2_1];
/// An early-adopter hub that has moved to 2.3.0 but keeps 2.2.1 back-coverage.
const HUB_MODERN: &[VersionNumber] = &[V2_2_1, V2_3_0];
/// A legacy hub still pinned to 2.1.1.
const HUB_LEGACY: &[VersionNumber] = &[V2_1_1];
/// The widest realistic supported set — the whole *selectable* range.
const HUB_WIDE: &[VersionNumber] = &[V2_1_1, V2_2, V2_2_1, V2_3_0];

/// The full negotiation matrix. Every representative `supported` set is one of
/// the `HUB_*` constants above, each of which is asserted recognition-only-free
/// by [`shipped_supported_sets_never_include_recognition_only`].
const MATRIX: &[Case] = &[
    // ── highest-mutual selection when the sets overlap ──────────────────────
    Case {
        name: "dual-version partner, hub picks the higher shared 2.2.1",
        remote: &[V2_1_1, V2_2_1],
        supported: HUB_MODERN,
        expected: Some(V2_2_1),
    },
    Case {
        name: "full overlap selects the top of the shared range",
        remote: &[V2_1_1, V2_2, V2_2_1],
        supported: HUB_2X,
        expected: Some(V2_2_1),
    },
    Case {
        name: "legacy 2.1.1-only partner: the only mutual version is 2.1.1",
        remote: &[V2_1_1],
        supported: HUB_2X,
        expected: Some(V2_1_1),
    },
    Case {
        name: "partner advertises the whole selectable range; a wide hub takes the top, 2.3.0",
        remote: &[V2_1_1, V2_2, V2_2_1, V2_3_0],
        supported: HUB_WIDE,
        expected: Some(V2_3_0),
    },
    Case {
        name: "partner ahead of a legacy hub still meets at 2.1.1",
        remote: &[V2_1_1, V2_2, V2_2_1],
        supported: HUB_LEGACY,
        expected: Some(V2_1_1),
    },
    // ── no common version → None (the explicit `UnsupportedVersion` path) ───
    Case {
        name: "disjoint: a 2.0-only partner shares nothing with a 2.x hub",
        remote: &[V2_0],
        supported: HUB_2X,
        expected: None,
    },
    Case {
        name: "disjoint: a partner ahead on 2.3.0 shares nothing with a legacy hub",
        remote: &[V2_3_0],
        supported: HUB_LEGACY,
        expected: None,
    },
    Case {
        name: "empty catalogue → None",
        remote: &[],
        supported: HUB_2X,
        expected: None,
    },
    // ── advertised-but-unsupported versions are ignored, not selected ───────
    Case {
        name: "unsupported 2.3.0 is ignored; only mutual version with the partner is 2.1.1",
        remote: &[V2_3_0, V2_1_1],
        supported: HUB_2X,
        expected: Some(V2_1_1),
    },
    // ── recognition-only edges: recognised, never selected ──────────────────
    Case {
        name: "2.0-only partner → None (never V2_0), even against the widest hub",
        remote: &[V2_0],
        supported: HUB_WIDE,
        expected: None,
    },
    Case {
        name: "3.0-only partner → None (never V3_0), even against the widest hub",
        remote: &[V3_0],
        supported: HUB_WIDE,
        expected: None,
    },
    Case {
        name: "both recognition-only edges advertised together → still None",
        remote: &[V2_0, V3_0],
        supported: HUB_MODERN,
        expected: None,
    },
    Case {
        name: "2.0 alongside a mutual 2.2.1 → the mutual version wins, 2.0 ignored",
        remote: &[V2_0, V2_2_1],
        supported: HUB_2X,
        expected: Some(V2_2_1),
    },
    Case {
        name: "forward 3.0 alongside a mutual 2.2.1 → the mutual version wins, 3.0 ignored",
        remote: &[V3_0, V2_2_1],
        supported: HUB_MODERN,
        expected: Some(V2_2_1),
    },
    Case {
        name: "both edges bracketing a mutual version → the mutual version wins",
        remote: &[V2_0, V2_2_1, V3_0],
        supported: HUB_2X,
        expected: Some(V2_2_1),
    },
    Case {
        name: "recognition never promotes: 3.0 alongside a lower mutual 2.1.1 → 2.1.1",
        remote: &[V3_0, V2_1_1],
        supported: HUB_2X,
        expected: Some(V2_1_1),
    },
];

#[test]
fn negotiation_matrix_holds_for_every_row() {
    for case in MATRIX {
        let remote = catalogue(case.remote);
        let got = negotiate_version(&remote, case.supported);
        assert_eq!(
            got, case.expected,
            "case '{}': remote={:?} supported={:?} → expected {:?}, got {:?}",
            case.name, case.remote, case.supported, case.expected, got
        );
    }
}

/// The invariant behind #182 and #219/#223: no `supported` set a real
/// deployment ships lists a recognition-only version, so `negotiate_version`
/// can never *select* `V2_0` or `V3_0` — recognition never becomes support.
#[test]
fn shipped_supported_sets_never_include_recognition_only() {
    for supported in [HUB_2X, HUB_MODERN, HUB_LEGACY, HUB_WIDE] {
        for recognition_only in RECOGNITION_ONLY {
            assert!(
                !supported.contains(&recognition_only),
                "supported set {supported:?} must not include recognition-only {recognition_only:?}"
            );
        }
    }
}

/// Direct corollary: a partner advertising *only* a recognition-only version
/// negotiates to `None` against every shipped supported set — never the
/// recognition-only version itself.
#[test]
fn recognition_only_partner_is_never_selected() {
    for recognition_only in RECOGNITION_ONLY {
        let remote = catalogue(&[recognition_only]);
        for supported in [HUB_2X, HUB_MODERN, HUB_LEGACY, HUB_WIDE] {
            assert_eq!(
                negotiate_version(&remote, supported),
                None,
                "a {recognition_only:?}-only partner must not be selected by {supported:?}"
            );
        }
    }
}

/// The `VersionNumber: Ord` chain the highest-mutual `.max()` rule depends on,
/// pinned in one place: strictly ascending across the whole range, with the two
/// recognition-only versions at the ends. `V3_0` is the maximum (recognised top)
/// yet — per the matrix above — never selectable.
#[test]
fn version_number_ord_chain_is_pinned() {
    let ascending = [V2_0, V2_1_1, V2_2, V2_2_1, V2_3_0, V3_0];
    for pair in ascending.windows(2) {
        assert!(
            pair[0] < pair[1],
            "version ordering broken: {:?} !< {:?}",
            pair[0],
            pair[1]
        );
    }

    // `.max()` over the full chain is the recognised top, `V3_0`.
    assert_eq!(ascending.iter().copied().max(), Some(V3_0));

    // Sorting a shuffled copy reproduces the canonical chain.
    let mut shuffled = [V3_0, V2_1_1, V2_3_0, V2_0, V2_2_1, V2_2];
    shuffled.sort();
    assert_eq!(shuffled, ascending);
}

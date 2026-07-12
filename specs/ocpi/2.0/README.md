# OCPI 2.0 (legacy) — recognised, not implemented

OCPI 2.0 predates the project's conversion to AsciiDoc and is not carried on a
dedicated `release-2.0-bugfixes` branch upstream, so `scripts/fetch-specs.sh`
does not vendor it automatically.

## Back-coverage contract (milestone M7, resolved — see [#182])

The 2.0 slice of milestone **M7** is **done by graceful recognition, not by a
type surface**. Concretely, the 2.0 contract this SDK commits to is:

- `VersionNumber::V2_0` parses and serializes `"2.0"` and orders it lowest
  (`V2_0 < V2_1_1 < V2_2 < V2_2_1 < V2_3_0`) — `crates/ocpi-types/src/version.rs`.
- Version negotiation degrades a 2.0-only partner to *no common version*, which
  the caller maps to an explicit `UnsupportedVersion` `status_code` — rather than
  silently mis-parsing a version it does not speak. The regression fence for this
  path is the `negotiate_disjoint_returns_none` test in
  `crates/ocpi-client/src/lib.rs` (*"Partner speaks only 2.0; the hub does not →
  no common version"*).

There is **no `v2_0` type module** and **no 2.0 support-matrix column**; none is
planned. OCPI 2.0 predates the roaming ecosystem this SDK targets and effectively
no live partner negotiates it, so a full 2.0 type surface would be a large,
low-value track against an obsolete version. Should that ever change, it would be
its own milestone/issue chain (scoped like the `v2_2` / `v2_3_0` foundations),
not a blocker on M7.

Upstream history: https://github.com/ocpi/ocpi

[#182]: https://github.com/evlinked/ocpi-rs/issues/182

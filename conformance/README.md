# OCPI conformance corpus

A spec-organised corpus of **example payloads taken from the OCPI
specification itself**, each round-tripped through the matching `ocpi-types`
model by a table-driven harness. It turns *"we think every module matches the
spec's own examples"* into a single, CI-enforced, auditable fact — and makes
version drift visible: if a refactor stops an example round-tripping, the
harness names the exact fixture and spec section.

## Layout

```
conformance/<version>/<module>/<name>.json
```

Each `.json` is a single spec example payload. The harness for a version lives
next to the crate it exercises:

| Version | Harness |
|---------|---------|
| 2.2.1   | [`crates/ocpi-types/tests/conformance_2_2_1.rs`](../crates/ocpi-types/tests/conformance_2_2_1.rs) |

## The invariant

For every fixture the harness asserts a **serde round-trip**: the example
deserializes into its typed model, re-serializes, and deserializes again to an
*equal* value (`T → JSON → T` stability). This is faithful to the wire without
being brittle about field ordering or the unknown-field tolerance the crate
deliberately keeps (2.3.0 `SHALL NOT reject on unknown fields`, #184) — a raw
JSON string-compare would fight both. `include_str!` embeds each fixture at
compile time, so a deleted or renamed corpus file breaks the build rather than
silently dropping coverage.

Run it with:

```sh
cargo test -p ocpi-types --test conformance_2_2_1
```

## Provenance — 2.2.1

Each fixture maps back to the spec example it was transcribed from. Where a
module's own unit test already carried the inline example, this corpus is the
consolidated home for it (the inline test remains as the module-local
assertion; this harness is the central coverage ledger).

| Fixture | Type | Spec source |
|---------|------|-------------|
| `credentials/minimal_cpo.json` | `Credentials` | `credentials.asciidoc` §Credentials object (single CPO role) |
| `credentials/multi_role.json` | `Credentials` | `credentials.asciidoc` §Credentials object (multiple roles) |
| `cdrs/cdr.json` | `Cdr` | `mod_cdrs.asciidoc` §Example of a CDR |
| `cdrs/cdr_token.json` | `CdrToken` | `mod_cdrs.asciidoc` §CdrToken |
| `sessions/simple_start.json` | `Session` | `mod_sessions.asciidoc` §Simple Session example of a just started session |
| `tariffs/tariff.json` | `Tariff` | `mod_tariffs.asciidoc` §Example |
| `tokens/app_user.json` | `Token` | `mod_tokens.asciidoc` §Example APP_USER Token |
| `tokens/full_rfid.json` | `Token` | `mod_tokens.asciidoc` §Example RFID Token |
| `commands/start_session.json` | `StartSession` | `mod_commands.asciidoc` §StartSession object |
| `commands/set_charging_profile.json` | `SetChargingProfile` | `mod_commands.asciidoc` §SetChargingProfile object |
| `hub_client_info/client_info.json` | `ClientInfo` | `mod_hub_client_info.asciidoc` §ClientInfo object |

## Adding a fixture

1. Drop the spec example under `conformance/<version>/<module>/<name>.json`
   (canonical source: the example payloads in `specs/ocpi/<version>/*.asciidoc`).
2. Add one `round_trip::<Type>("<module>/<name>", include_str!(…))` line to the
   version's harness.
3. Record the provenance row in this table.

## Scope

2.2.1 (the primary production target) lands first, per issue #225. Follow-up
corpora for 2.1.1 / 2.2 / 2.3.0 extend the same layout and harness pattern.

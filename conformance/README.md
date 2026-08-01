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
| 2.1.1   | [`crates/ocpi-types/tests/conformance_2_1_1.rs`](../crates/ocpi-types/tests/conformance_2_1_1.rs) |
| 2.2     | [`crates/ocpi-types/tests/conformance_2_2.rs`](../crates/ocpi-types/tests/conformance_2_2.rs) |
| 2.2.1   | [`crates/ocpi-types/tests/conformance_2_2_1.rs`](../crates/ocpi-types/tests/conformance_2_2_1.rs) |
| 2.3.0   | [`crates/ocpi-types/tests/conformance_2_3_0.rs`](../crates/ocpi-types/tests/conformance_2_3_0.rs) |

All four harnesses share the [`round_trip`](../crates/ocpi-types/tests/corpus_common/mod.rs)
helper (in `tests/corpus_common/`, a subdirectory so Cargo does not treat it as
its own test binary).

## The invariant

For every fixture the harness asserts a **serde round-trip**: the example
deserializes into its typed model, re-serializes, and deserializes again to an
*equal* value (`T → JSON → T` stability). This is faithful to the wire without
being brittle about field ordering or the unknown-field tolerance the crate
deliberately keeps (2.3.0 `SHALL NOT reject on unknown fields`, #184) — a raw
JSON string-compare would fight both. `include_str!` embeds each fixture at
compile time, so a deleted or renamed corpus file breaks the build rather than
silently dropping coverage.

Run them with:

```sh
cargo test -p ocpi-types --test conformance_2_1_1 \
  --test conformance_2_2 --test conformance_2_2_1 --test conformance_2_3_0
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

## Provenance — 2.1.1

The 2.1.1 corpus covers the version's genuine wire deltas (the *flat* `Cdr` /
`Session` with an embedded full `Location`, `auth_id` instead of a `cdr_token`,
and `stop_date_time` / `start_datetime` field names; the `auth_id`-keyed
`Token`). The `Cdr` / `Session` fixtures each embed a full `Location` (with an
`Evse` + `Connector` in the CDR), so the Location object class is round-tripped
for 2.1.1 here rather than as a standalone fixture.

| Fixture | Type | Spec source |
|---------|------|-------------|
| `cdrs/cdr.json` | `v2_1_1::Cdr` | 2.1.1 §CDR Object (embedded `location`, `auth_id`, `stop_date_time`) |
| `sessions/session.json` | `v2_1_1::Session` | 2.1.1 §Session Object (embedded `location`, `auth_id`, `start_datetime`) |
| `tokens/rfid.json` | `v2_1_1::Token` | 2.1.1 §Token Object (`auth_id`-keyed RFID token) |

## Provenance — 2.2

2.2 is a mostly wire-identical predecessor of 2.2.1; the corpus covers its
genuine (subtractive) deltas only — the wire-identical surface is re-exports
already exercised by the 2.2.1 corpus.

| Fixture | Type | Spec source |
|---------|------|-------------|
| `cdrs/cdr_token.json` | `v2_2::CdrToken` | 2.2 §CdrToken (no 2.2.1 `country_code` / `party_id`) |
| `commands/start_session.json` | `v2_2::StartSession` | 2.2 §StartSession (no 2.2.1 `connector_id`) |

## Provenance — 2.3.0

The 2.3.0 corpus covers the version's genuine wire deltas over 2.2.1 (the M8
close-out surface): the new Payments module, the North-American tax rework on
Tariffs / CDRs / Sessions, the `hub_party_id` Credentials, and the
Parking-bearing Location. The wire-identical modules (Tokens, Commands,
ChargingProfiles, HubClientInfo, Versions) are byte-for-byte re-exports of 2.2.1,
already exercised by the 2.2.1 corpus and the `reuse_types_stay_aliases_of_2_2_1`
compile-time identity, so they are not duplicated here.

| Fixture | Type | Spec source |
|---------|------|-------------|
| `tariffs/tax_included.json` | `v2_3_0::Tariff` | `mod_tariffs.asciidoc` §North American taxes (`tax_included`) |
| `payments/terminal.json` | `v2_3_0::Terminal` | `mod_payments.asciidoc` §Terminal (assigned locations + EVSEs) |
| `payments/financial_advice.json` | `v2_3_0::FinancialAdviceConfirmation` | `mod_payments.asciidoc` §FinancialAdviceConfirmation (successful capture) |
| `credentials/hub_party_id.json` | `v2_3_0::Credentials` | `credentials.asciidoc` §Credentials object (`hub_party_id` + HUB role) |
| `cdrs/na_taxed.json` | `v2_3_0::Cdr` | `mod_cdrs.asciidoc` §CDR (reworked `Price`: `before_taxes` + itemised `TaxAmount`) |
| `sessions/na_taxed.json` | `v2_3_0::Session` | `mod_sessions.asciidoc` §Session (reworked `Price`) |
| `locations/location.json` | `v2_3_0::Location` | `mod_locations.asciidoc` §Location object (`parking_places` + `help_phone`) |

## Adding a fixture

1. Drop the spec example under `conformance/<version>/<module>/<name>.json`
   (canonical source: the example payloads in `specs/ocpi/<version>/*.asciidoc`).
2. Add one `round_trip::<Type>("<module>/<name>", include_str!(…))` line to the
   version's harness.
3. Record the provenance row in this table.

## Scope

2.2.1 (the primary production target) landed first, per issue #225. The 2.1.1 /
2.2 / 2.3.0 corpora (issue #234) extend the same layout and shared harness
pattern, covering each version's genuine wire-delta modules; the wire-identical
(re-exported) modules are not duplicated per version, since the 2.2.1 corpus
already exercises those shapes. The Location/EVSE/Connector object class is
round-tripped in 2.3.0 (standalone `Location`) and 2.1.1 (embedded in the `Cdr` /
`Session`).

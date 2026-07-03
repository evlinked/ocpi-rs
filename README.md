# ocpi-rs

*A modern, production-grade OCPI (Open Charge Point Interface) implementation in Rust. Typed models, an async client, and server-side handlers for every OCPI version.*

[![CI](https://github.com/EVLinked/ocpi-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/EVLinked/ocpi-rs/actions/workflows/ci.yml)
[![Security](https://github.com/EVLinked/ocpi-rs/actions/workflows/security.yml/badge.svg)](https://github.com/EVLinked/ocpi-rs/actions/workflows/security.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.86%2B-orange.svg)](https://www.rust-lang.org)

---

## What is OCPI?

OCPI is the open protocol that lets EV charging networks roam: a Charge Point Operator (CPO) and an e-Mobility Service Provider (eMSP) exchange Locations, Sessions, CDRs, Tariffs, Tokens, and remote Commands over a versioned REST/JSON API with a token-based credentials handshake. `ocpi-rs` aims to implement the **full standard, all versions**, as a reusable library/SDK.

## Why this project

- **Safety**: memory-safe, no `unsafe` in the type layer (`#![forbid(unsafe_code)]`).
- **Correctness**: types follow the spec; the unsupported case is rejected with an explicit OCPI `status_code`, never silently dropped.
- **Reusability**: a clean SDK (types + client + server traits) you can embed in a CPO or eMSP backend.
- **Portability**: small static binaries, `rustls` by default (no system OpenSSL).

## Scope & Non-Goals

### In scope
- Typed models for every OCPI module across versions **2.0, 2.1.1, 2.2, 2.2.1, 2.3.0** (and a forward-scaffold for 3.0).
- An async **client** (sender role): version negotiation, credentials handshake, typed module senders.
- **Server** handler traits (receiver role) with an optional `axum` integration.
- A small **CLI** for inspecting and validating OCPI parties.

### Out of scope (for this repo)
- OCPP (charger ↔ CSMS) — see the sibling project [`EVLinked/ocpp-rs`](https://github.com/EVLinked/ocpp-rs).
- Billing, pricing engines, end-user apps.
- A hosted, deployable CPO/eMSP service (this is a library you build one with).

## Workspace layout

| Crate | Role |
|---|---|
| [`ocpi-types`](crates/ocpi-types) | Wire types: response envelope, status codes, common data types, version negotiation. Version-namespaced module models. |
| [`ocpi-client`](crates/ocpi-client) | Async HTTP client for the sender role (`reqwest`, `rustls`). |
| [`ocpi-server`](crates/ocpi-server) | Receiver-side handler traits + optional `axum` routers. |
| [`ocpi-cli`](crates/ocpi-cli) | `ocpi` command-line tool: list versions, validate envelopes. |

## Quickstart

```bash
# Build everything
cargo build --workspace

# List the versions a remote OCPI party supports
cargo run -p ocpi-cli -- versions https://host/ocpi/cpo/ --token <TOKEN>

# Validate a JSON file parses as an OCPI response envelope
cargo run -p ocpi-cli -- validate ./response.json
```

## Roadmap & Milestones

Each milestone maps to a GitHub milestone and a release. OCPI **2.2.1** is the primary production target; older and newer versions follow.

- [x] **M0** — Bootstrap: CI, security, governance, docs, vendored specs
- [x] **M1** — Core foundation: envelope, status codes, transport headers, pagination, common types — `v0.1.0`
- [x] **M2** — Versions + Credentials/Registration handshake (2.2.1) — `v0.2.0`
- [x] **M3** — Locations (2.2.1) — `v0.3.0`
- [x] **M4** — Sessions + CDRs (2.2.1) — `v0.4.0`
- [x] **M5** — Tariffs + Tokens (2.2.1) — `v0.5.0`
- [x] **M6** — Commands + ChargingProfiles + HubClientInfo → **OCPI 2.2.1 complete** ✅ — `v1.0.0`
- [ ] **M7** — OCPI 2.1.1 (+ 2.2 / 2.0 back-coverage) — `v1.1.0`
- [ ] **M8** — OCPI 2.3.0 (Payments, terminals, new fields) — `v1.2.0`
- [ ] **M9** — Conformance, fuzzing, docs site, 3.0 forward-scaffold — `v1.3.0+`

> **OCPI 2.2.1 is feature-complete** across `ocpi-types`, `ocpi-client`, and `ocpi-server` (all 10 modules: types + sender methods + receiver handlers/routers). Remaining 2.2.1 work is non-blocking polish — end-to-end smoke tests ([#23](https://github.com/evlinked/ocpi-rs/issues/23), [#32](https://github.com/evlinked/ocpi-rs/issues/32), [#71](https://github.com/evlinked/ocpi-rs/issues/71), [#72](https://github.com/evlinked/ocpi-rs/issues/72)) and the ChargingProfiles Sender PUT method ([#75](https://github.com/evlinked/ocpi-rs/issues/75)).

## Module × version support matrix

Legend: ☐ planned · ◑ in progress · ☑ done

| Module | 2.1.1 | 2.2.1 | 2.3.0 |
|---|:--:|:--:|:--:|
| Versions | ☑ | ☑ | ☐ |
| Credentials | ☑ | ☑ | ☐ |
| Locations | ◑ | ☑ | ☐ |
| Sessions | ☑ | ☑ | ☐ |
| CDRs | ☑ | ☑ | ☐ |
| Tariffs | ☑ | ☑ | ☐ |
| Tokens | ◑ | ☑ | ☐ |
| Commands | ◑ | ☑ | ☐ |
| ChargingProfiles | — | ☑ | ☐ |
| HubClientInfo | — | ☑ | ☐ |
| Payments | — | — | ☐ |

☑ = types + client sender methods + server receiver handler/router shipped for 2.2.1. The ChargingProfiles **Sender** interface — the CPO-pushes-`ActiveChargingProfile` PUT ([#75](https://github.com/evlinked/ocpi-rs/issues/75)) — is now complete via `charging_profiles_sender_router` + `OcpiClient::put_active_charging_profile`.

M7 (OCPI 2.1.1) is underway. The **Versions** module is now end-to-end for 2.1.1: the role-less foundation (`v2_1_1::Endpoint` / `v2_1_1::VersionDetails`, [#86](https://github.com/evlinked/ocpi-rs/issues/86)), the client `negotiate_version` helper ([#103](https://github.com/evlinked/ocpi-rs/pull/103)), and the server advertising both 2.1.1 and 2.2.1 via `VersionsConfig::add_legacy_version` so `GET /versions/2.1.1` serves a role-less catalogue ([#99](https://github.com/evlinked/ocpi-rs/issues/99)). The **Credentials** module is now end-to-end for 2.1.1 over the *flat* object: client sender methods (`OcpiClient::{register,get_credentials,update_credentials}_2_1_1`) and the server receiver `credentials_2_1_1_router` + `Credentials2111Config`, running the Token A→B→C handshake ([#112](https://github.com/evlinked/ocpi-rs/issues/112)). The 2.1.1 registration *fetch-back* (the role-less `/versions` callback) now lands too: a `LegacyVersionFetcher` returns the role-less `v2_1_1::VersionDetails` a faithful 2.1.1 partner emits, and `Credentials2111Config::new_with_fetcher` `GET`s the registering party's `/versions`, negotiates the highest mutual version, and stores its endpoint catalogue (failure → status code `3001`) ([#115](https://github.com/evlinked/ocpi-rs/issues/115)). The **Commands** data types ([#93](https://github.com/evlinked/ocpi-rs/issues/93)) have landed for 2.1.1 (`CommandType`, `CommandResponseType`, `CommandResponse`, `ReserveNow`/`StartSession`/`StopSession`/`UnlockConnector`) — faithful to the spec's single-`CommandResponseType` model (no 2.2 `CommandResultType` split). The **Locations** module has its client *sender* getters for 2.1.1 (`OcpiClient::{get_locations,get_location,get_evse,get_connector}_2_1_1`, [#113](https://github.com/evlinked/ocpi-rs/issues/113)) — the 2.1.1 server *receiver* router is a tracked follow-up. The **Sessions** module is now end-to-end for 2.1.1: the client sender list + receiver getters/setters (`OcpiClient::{get_sessions,get_session,put_session,patch_session}_2_1_1`) and the server `sessions_2_1_1_router` + `Sessions2111Config` ([#120](https://github.com/evlinked/ocpi-rs/issues/120)). Per OCPI 2.1.1 §9.2.2 the Sessions receiver path keeps the `{country_code}/{party_id}/{session_id}` segments (a client-owned object) — identical to 2.2.1; only the `Session` object shape differs (`auth_id`, embedded `location`, one-word `start_datetime`, no `charging_preferences`). The **CDRs** module is now end-to-end for 2.1.1: the client sender list + receiver getter/push (`OcpiClient::{get_cdrs,get_cdr,post_cdr}_2_1_1`) and the server `cdrs_2_1_1_router` + `Cdrs2111Config` ([#120](https://github.com/evlinked/ocpi-rs/issues/120)). A CDR is a server-owned object named via the `Location` header on `POST /cdrs` (§10.2.2), so the 2.1.1 paths are flat (`POST /cdrs`, `GET /cdrs/{id}`, `GET /cdrs`) — identical to 2.2.1; only the `Cdr` object shape differs (bare `auth_id`, embedded `location`, `stop_date_time`, a single numeric `total_cost`, no `session_id`). The **Tariffs** module is now end-to-end for 2.1.1: client sender list (`OcpiClient::get_tariffs_2_1_1`) plus receiver getters/setters (`get_tariff_2_1_1`/`put_tariff_2_1_1`/`delete_tariff_2_1_1`) and the server `tariffs_2_1_1_router` + `Tariffs2111Config` ([#122](https://github.com/evlinked/ocpi-rs/issues/122)) — the 2.1.1 transport paths are identical to 2.2.1 (Sender flat `GET /tariffs`; Receiver `{country_code}/{party_id}/{tariff_id}`), only the `Tariff` object shape differs. Remaining 2.1.1 module types are tracked in [#89](https://github.com/evlinked/ocpi-rs/issues/89)–[#93](https://github.com/evlinked/ocpi-rs/issues/93).

## How this repo is built

This repo develops itself. A nightly Claude **remote routine** picks one owner-approved GitHub issue, implements it on a branch, opens a PR, and marks it ready for review for the owner to merge under strict CI. See [`nightly/PLAYBOOK.md`](nightly/PLAYBOOK.md) and [CONTRIBUTING.md](CONTRIBUTING.md).

**Governance:** only the owner is trusted. The nightly bot's PRs are automatically marked ready for review (no auto-merge); the owner reviews and merges every PR once all required checks are green. See [CONTRIBUTING.md](CONTRIBUTING.md#governance).

## Specifications

The OCPI specs are vendored under [`specs/`](specs/) for reference. They are © EV Roaming Foundation and are **not** covered by this project's MIT license — see [`specs/NOTICE.md`](specs/NOTICE.md).

## License

MIT — see [LICENSE](LICENSE).

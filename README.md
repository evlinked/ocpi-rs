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

- [ ] **M0** — Bootstrap: CI, security, governance, docs, vendored specs
- [ ] **M1** — Core foundation: envelope, status codes, transport headers, pagination, common types — `v0.1.0`
- [ ] **M2** — Versions + Credentials/Registration handshake (2.2.1) — `v0.2.0`
- [ ] **M3** — Locations (2.2.1) — `v0.3.0`
- [ ] **M4** — Sessions + CDRs (2.2.1) — `v0.4.0`
- [ ] **M5** — Tariffs + Tokens (2.2.1) — `v0.5.0`
- [ ] **M6** — Commands + ChargingProfiles + HubClientInfo → **OCPI 2.2.1 complete** — `v1.0.0`
- [ ] **M7** — OCPI 2.1.1 (+ 2.2 / 2.0 back-coverage) — `v1.1.0`
- [ ] **M8** — OCPI 2.3.0 (Payments, terminals, new fields) — `v1.2.0`
- [ ] **M9** — Conformance, fuzzing, docs site, 3.0 forward-scaffold — `v1.3.0+`

## Module × version support matrix

Legend: ☐ planned · ◑ in progress · ☑ done

| Module | 2.1.1 | 2.2.1 | 2.3.0 |
|---|:--:|:--:|:--:|
| Versions | ☐ | ☐ | ☐ |
| Credentials | ☐ | ☐ | ☐ |
| Locations | ☐ | ☐ | ☐ |
| Sessions | ☐ | ☐ | ☐ |
| CDRs | ☐ | ☐ | ☐ |
| Tariffs | ☐ | ☐ | ☐ |
| Tokens | ☐ | ☐ | ☐ |
| Commands | ☐ | ☐ | ☐ |
| ChargingProfiles | — | ☐ | ☐ |
| HubClientInfo | — | ☐ | ☐ |
| Payments | — | — | ☐ |

## How this repo is built

This repo develops itself. A nightly Claude **remote routine** picks one owner-approved GitHub issue, implements it on a branch, opens a PR, and lets strict CI gate the merge. See [`nightly/PLAYBOOK.md`](nightly/PLAYBOOK.md) and [CONTRIBUTING.md](CONTRIBUTING.md).

**Governance:** only the owner is trusted. The owner's (and the nightly bot's) PRs auto-merge once all required checks are green; everyone else's PRs are reviewed and merged manually by the owner. See [CONTRIBUTING.md](CONTRIBUTING.md#governance).

## Specifications

The OCPI specs are vendored under [`specs/`](specs/) for reference. They are © EV Roaming Foundation and are **not** covered by this project's MIT license — see [`specs/NOTICE.md`](specs/NOTICE.md).

## License

MIT — see [LICENSE](LICENSE).

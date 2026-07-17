# `ocpi-fuzz` — deserialization fuzzing (M9)

Fuzz harnesses for `ocpi-rs`'s single most important trust boundary:
**deserializing untrusted OCPI JSON off the wire**. `ocpi-server`'s receiver
routers and `ocpi-client`'s response parsing both turn JSON authored by a
remote party into typed objects; this crate drives that boundary with
[`cargo-fuzz`] / libFuzzer to make the charter's promise continuously verified.

## The invariant

> No input — however malformed, truncated, deeply nested, non-UTF-8, or
> adversarial — may ever `panic!`, overflow, or hang the deserializer. Every
> input must resolve to `Ok(_)` or a clean `Err(_)`.

A panic on a hostile payload is a remote **denial-of-service** on a Hub that
deserializes JSON from every partner it roams with, not a cosmetic bug. Each
`fuzz_target!` calls `serde_json::from_slice::<T>(data)` and discards the
result; libFuzzer treats any panic/abort as a crash and writes the offending
input to `fuzz/artifacts/<target>/`.

## Targets

| Target      | Type                            | Boundary |
|-------------|---------------------------------|----------|
| `envelope`  | `OcpiResponse<serde_json::Value>` | The response wrapper (`status_code` / `timestamp` parsing). |
| `versions`  | `Vec<Version>`                  | The `/versions` catalogue — first thing negotiation parses (#219). |
| `location`  | `Location`                      | Largest composite a CPO pushes (nested EVSEs/connectors). |
| `session`   | `Session`                       | Charging session object. |
| `cdr`       | `Cdr`                           | Billing record a Hub relays between partners. |
| `tariff`    | `Tariff`                        | Nested price components / restrictions / floats. |
| `token`     | `Token`                         | Authorization token object. |

## Running

`cargo-fuzz` requires a **nightly** toolchain and the libFuzzer runtime:

```sh
rustup toolchain install nightly
cargo install cargo-fuzz

# Fuzz one target (Ctrl-C to stop; runs until it finds a crash):
cargo +nightly fuzz run location

# Time-boxed smoke run over just the committed seed corpus:
cargo +nightly fuzz run location -- -runs=50000 -max_total_time=30

# List targets:
cargo +nightly fuzz list
```

A reproduced crash is written to `fuzz/artifacts/<target>/`; re-run it with
`cargo +nightly fuzz run <target> fuzz/artifacts/<target>/<crash-file>`.

## Seed corpus

`corpus/<target>/*.json` holds the **spec example payloads** already used as
module unit-test fixtures (transcribed from `specs/ocpi/2.2.1/*.asciidoc`), so
the fuzzer starts from valid, structurally-rich inputs and explores mutations
around them. Only these human-authored seeds are committed; libFuzzer's own
coverage-expanding discoveries under `corpus/` are `.gitignore`d.

## Why it is excluded from the workspace

`fuzz/` is listed under `exclude` in the root `Cargo.toml` and carries its own
`[workspace]` table, so it is **not** part of the default build. The libFuzzer
runtime is nightly-only and would otherwise break `cargo build/test
--workspace` and the stable `-D warnings` CI gates. `libfuzzer-sys` is a
dependency of *this member only* — never of the shipping crates
(`ocpi-types` / `ocpi-client` / `ocpi-server` / `ocpi-cli`), so their
"no new runtime dependencies" and `#![forbid(unsafe_code)]` guarantees hold.

[`cargo-fuzz`]: https://github.com/rust-fuzz/cargo-fuzz

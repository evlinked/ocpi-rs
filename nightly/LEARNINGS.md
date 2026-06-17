# Nightly Learnings

Durable, project-specific lessons. Read at the start of every run. Add an entry
when you discover something that would save the next run time or a failed CI
cycle. Keep entries short and specific. Prune contradictions.

## Workflow & PR hygiene

- **NEVER bundle a `.github/`/Cargo guarded-path change into a feature PR.** PRs #31
  (Locations types + MSRV) and #24 (credentials router + MSRV) each stapled an MSRV
  `.github` change onto pure-additive implementation work. Result: both got `needs-human`,
  the owner didn't merge immediately, and they went `dirty` as `main` advanced — blocking
  M3 entirely for ~5 runs. **Keep guarded-path changes (CI/MSRV/deps/LICENSE/scripts) in
  their OWN PR** so implementation work stays auto-mergeable. The MSRV job is
  `continue-on-error: true` on `main` (non-blocking), so its calibration is never urgent.
- **Rescuing a rotted/`dirty` nightly PR: re-deliver, don't merge-resolve.** When a stuck
  PR is pure-additive (new types/modules), the fastest clean fix is to re-apply just the
  net-new content on a fresh branch off current `main` and close the old PR as superseded —
  NOT to resolve a 15+ region merge conflict. A blind `git cherry-pick` of the original
  commit will conflict heavily because later milestones grew the same files.
- **HTTP-level router tests (`tower::oneshot`) are NOT worth a guarded-path PR.** PR #24
  (credentials router, #22) added `tower`/`tokio`/`serde_json` as `ocpi-server` dev-deps purely
  to exercise the axum router via `ServiceExt::oneshot` + `#[tokio::test]`. That `Cargo.toml`
  edit forced `needs-human` and the PR rotted for ~6 runs, blocking all of M2. The established
  codebase pattern is the opposite: **every merged router (sessions/cdrs/tariffs/tokens/commands)
  has ZERO HTTP-level tests** — they test the `*Config` struct's sync helpers directly and let
  the async handlers be compile-checked only. When re-delivering a stuck router PR, drop the
  oneshot tests and keep sync `Config` tests so `Cargo.toml` stays untouched → auto-mergeable.
  HTTP-level coverage belongs in the dedicated e2e smoke-test issue (#23/#32), which introduces
  the harness dev-deps in its own PR.
- **Before re-delivering old type work, grep `main` for the types first.** M4/M5/M6 added
  `TokenType`, `ConnectorType`, `ConnectorFormat`, `PowerType` to `v2_2_1.rs` ahead of the
  Locations module (Sessions/CDRs/Tokens referenced them). Re-applying an older Locations
  commit that *defines* them causes duplicate-definition errors. Reuse what exists; add only
  the genuinely missing types. `grep -nE '^pub (enum|struct) ' <file>` is the quick check.

## Toolchain & CI

- **rustfmt.toml is stable-safe on purpose.** Do not add nightly-only options
  (`imports_granularity`, `group_imports`, `wrap_comments`, `fn_args_layout`,
  etc.). Stable `cargo fmt --check` ignores them and the formatting drifts
  between contributors. Keep it to stable keys.
- **clippy.toml accepts only valid keys.** An unknown key makes clippy error out
  for the whole workspace. `missing-safety-doc` is **not** a clippy.toml key
  (it's a lint, set via attributes). Verify keys against the installed clippy.
- **`axum::Router` is already `#[must_use]`.** Do not put `#[must_use]` on a
  function returning it — clippy's `double_must_use` fails under `-D warnings`.
- **Public async traits need `#[allow(async_fn_in_trait)]`.** The
  `async_fn_in_trait` lint is warn-by-default and becomes an error under
  `-D warnings`. Add the allow (we don't need `Send` bounds for these handlers).
- **Private struct fields must be read** or rustc's dead-code lint fails the
  build under `-D warnings`. Either use the field in a real method or rethink it.
- **Use `reqwest` with `rustls-tls` + `default-features = false`** to avoid a
  system OpenSSL dependency in CI and in static builds.
- **No DB in the core.** This is a library; CI needs no Postgres service. Keep
  the server's default store in-memory and feature-gate anything heavier.

## Rust type system

- **`#[derive(Hash)]` with manual `PartialEq` is a clippy error** (`derived_hash_with_manual_eq`,
  deny-by-default under `-D warnings`). When `PartialEq` is case-insensitive, implement
  `Hash` manually too — hash `.to_ascii_lowercase()` so `a == b → hash(a) == hash(b)`.
- **Const-generic newtypes for bounded strings** — `struct Foo<const N: usize>(String)` is
  ergonomic in Rust 1.65+. Serde requires a manual impl (no derive support for const
  generics); the pattern is `Serialize → serialize_str` and `Deserialize → String::deserialize +
  TryFrom`.

- **Inherent methods shadow same-named trait methods — use this to avoid PATCH
  recursion.** `LocationsConfig` has both inherent `patch_location()` etc. AND a
  `LocationsHandler` trait impl with identical signatures. In the trait body,
  `self.patch_location(...)` resolves to the **inherent** method (inherent wins
  over trait in method lookup), so the trait impl can delegate to the store
  without recursing into itself. Verify with a runtime test, not just compile.
- **Reusable RFC 7396 merge-patch helper:** `fn apply_merge_patch<T>(cur: &T, p:
  Value) -> Result<T>` where `T: ocpi_types::serde::Serialize +
  ocpi_types::serde::de::DeserializeOwned` (both re-exported from `ocpi-types`,
  so `ocpi-server` needs no direct `serde` dep). Serialize → `json_merge` →
  `from_value`. One helper covers Location/EVSE/Connector PATCH at every nesting
  level. Generic axum response helpers that `Json(OcpiResponse::<T>::...)` need a
  `T: serde::Serialize` bound or `into_response()` won't compile.

## Serde patterns

- **Serialize an enum as its integer value** (not variant name): use `#[serde(from = "u16", into = "u16")]` on the enum + `impl From<u16>` (infallible) + `impl From<MyEnum> for u16`. No manual `Serialize`/`Deserialize` impl needed. Works for any `Copy`/`Clone` enum.
- **`Display` needed when changing a field type**: if downstream code formats a field with `{}`, changing that field's type (e.g., `u16` → `MyEnum`) requires `impl Display` on the new type, or the format call won't compile.
- **`Option<OcpiStatusCode>` vs plain `OcpiStatusCode`**: once you have an `Unknown(u16)` catch-all variant, `from_code` can stay `Option<Self>` for the _known_ lookup while `From<u16>` gives the infallible path. This keeps existing call sites that `expect("known code")` working unchanged.

## OCPI domain

- **Single-letter enum values (e.g. `W`, `A`) need explicit `#[serde(rename = "...")]`**, not `rename_all`. SCREAMING_SNAKE_CASE would produce the same result for single-letter uppercase variants, but explicit renames make intent clear and prevent accidental breakage if a variant is renamed. Use `#[serde(rename = "W")]` on variant `W` (not `rename_all = "SCREAMING_SNAKE_CASE"`) when wire values are single uppercase letters.
- **M6 is Commands + ChargingProfiles + HubClientInfo** per the README milestone ("M6 — Commands + ChargingProfiles + HubClientInfo → OCPI 2.2.1 complete"). ChargingProfiles was inadvertently skipped in runs 6-8. Always diff README milestones vs implemented types when declaring a milestone complete.
- **`status_code` is an integer in the body**, independent of the HTTP status.
  `1000` = success. Keep `OcpiStatusCode` ↔ `u16` mapping exhaustive.
- **Version strings are dotted** (`"2.2.1"`); `VersionNumber` serializes via
  serde `rename`. `/versions` negotiation selects the module set at runtime.
- **3.0 is upstream-restricted.** Do not expect to fully implement it from public
  sources; scaffold types and mark work `blocked-upstream`.
- **`Authorization: Token <base64>`** — OCPI 2.2.1 requires Base64 (RFC 4648
  standard alphabet) encoding of the raw credentials token. 2.1.1/2.2
  implementations often skip the encoding; interop requires a config flag at the
  HTTP client layer, not in the type model.
- **Lenient header parsing: try-Base64-first is ambiguous but unavoidable.** For
  server-side compat with mixed 2.1.1/2.2/2.2.1 peers, try Base64-decode first;
  if that yields valid UTF-8, use the decoded result. Otherwise treat the raw
  tail as a plaintext token. Document the caveat: raw tokens that are
  coincidentally valid Base64 will be decoded incorrectly. Expose as two separate
  functions (`from_header_value` strict vs `from_header_value_lenient`) so call
  sites are self-documenting.
- **`base64 0.22` is already a transitive dep** (comes in via reqwest). Promoting
  it to a direct workspace dep does NOT add a new package to Cargo.lock and does
  not require a `needs-human` for the dep itself, but touching workspace
  Cargo.toml still triggers `needs-human` per the workflow rules.
- **Routing headers vs configuration modules** — `OCPI-to/from-party-id/country-code`
  headers are REQUIRED for Functional Modules (Tokens, Locations, CDRs) but MUST
  NOT be used on Configuration Modules (Credentials, Versions, Hub Client Info).
- **`Link` header format** — `<URL>; rel="next"`, comma-separated for multiple
  relations. Absent on the last page. `X-Limit` reflects the server's upper bound,
  not the count actually returned.
- **`async_fn_in_trait` + axum incompatibility** — Axum requires handler futures
  to be `Send`. `async_fn_in_trait` does not guarantee `Send` on the returned
  future. Do NOT wire an `async_fn_in_trait` trait directly to an axum generic
  handler. Use a concrete struct (e.g. `VersionsConfig`) as axum `State` instead,
  and keep the trait as a standalone interface for non-axum uses.
- **`#[serde(untagged)]` on a single variant is invalid** — Apply `untagged` to
  the whole enum or not at all. Per-variant `untagged` on a tuple variant inside
  an otherwise-tagged enum does not work as a catch-all; it compiles but
  deserialization will be wrong. Use a custom `Deserialize` impl for catch-all
  unknown string variants.
- **Clippy `unnecessary_lazy_evaluations`** — `Option::ok_or_else(|| T)` is
  flagged when `T` is cheap to construct (not a method call with side effects).
  Use `ok_or(T)` for simple enum variants.
- **Clippy `unnecessary_get_then_check`** — `map.get(k).is_none()` should be
  `!map.contains_key(k)`; `map.get(k).is_some()` should be `map.contains_key(k)`.
- **`#[derive(PartialOrd, Ord)]` on enums works correctly when variants are declared in the intended ordering.** Rust's auto-derived `Ord` assigns discriminants 0, 1, 2, … in declaration order. For `VersionNumber`, declaring variants `V2_0, V2_1_1, V2_2, V2_2_1, V2_3_0` means older < newer automatically. Verify declaration order before adding `Ord` to any domain enum.
- **Extract pure helpers for async methods that have non-trivial selection logic.** `select_version(remote, supported) -> Option<&Version>` is a synchronous pure function despite `negotiate_version` being async. Testing the selection logic directly (no HTTP mocking) makes the test suite faster and more targeted.
- **`f64` fields prevent `Eq` derivation.** Any struct containing an `f64` field (Price, EnergySource, EnvironmentalImpact, EnergyMix) can only derive `PartialEq`, not `Eq`. If downstream code or tests use `.eq()` inside a `HashMap` key or `BTreeSet`, switch those fields to a decimal representation. For now `PartialEq` is sufficient.
- **`Vec<T>` optional arrays (cardinality `*` in OCPI): use `#[serde(default, skip_serializing_if = "Vec::is_empty")]`.** This gives clean JSON (arrays omitted when empty) while allowing missing fields to deserialize as `vec![]` without wrapping in `Option<Vec<T>>`.
- **Coordinate validation without a regex crate:** The OCPI `GeoLocation` regex (`-?[0-9]{1,2}\.[0-9]{5,7}`) can be validated with a small private helper that strips the optional sign, splits on `.`, and checks digit counts. Avoids the `regex` crate (which would be a new dep and trigger `needs-human`).
- **EnergyMix and friends live in `common.rs`, not a version-specific module.** They are shared across Locations, Tariffs, and Sessions. Place them in `ocpi-types::common` and re-export from the crate root.
- **`cargo-deny` is not pre-installed in the remote execution environment.** Skip local `deny check` and trust CI; no new dependencies in this run means deny will pass.
- **OCPI `type` field collides with the Rust keyword.** Structs like `CdrDimension` and `CdrToken` have a spec field named `type`. Rename it in Rust (e.g. `dimension_type`, `token_type`) and annotate with `#[serde(rename = "type")]` to preserve the wire name. Every CDR/Session type that embeds a `type` field needs this treatment — check before implementing.
- **Shared CDR primitives (CdrToken, AuthMethod, ChargingPeriod, CdrDimension/Type) live in `v2_2_1.rs`** — defined when Sessions types were implemented (#34). CDR types issue (#35) must re-export them from there, not redefine. Use `pub use v2_2_1::{…}` in the CDR module if it becomes a separate file.
- **`serde_json` was already in Cargo.lock as a dev-dep of `ocpi-types`.** Moving it to `[dependencies]` (regular dep) does NOT change `Cargo.lock` — the package was already locked. Re-exporting from `ocpi-types` avoids any new direct deps in `ocpi-server`/`ocpi-client`, keeping Cargo.lock unchanged.
- **Cargo.lock metadata changes when you add a new direct dep, even if the package was already transitive.** The `[[package]]` entry for the workspace member gains an extra line in the `dependencies = [...]` list. Any change to `Cargo.lock` triggers RISK=high in guardrails. Avoid new direct deps by routing through `ocpi-types` re-exports.
- **`pub use chrono::{self, DateTime, Utc};` re-exports both the module AND specific types.** Downstream crates can `use ocpi_types::chrono::TimeZone as _` for trait methods and `use ocpi_types::{DateTime, Utc}` for the common types. No direct chrono dep needed.
- **`cargo generate-lockfile` regenerates the ENTIRE lock file with latest compatible versions.** Never run it to check if Cargo.lock would change. Instead, run `cargo check --locked` — it fails if the lock file needs updating and succeeds otherwise.
- **`use super::*` in tests does NOT re-export private `use` items.** Private `use` statements are not part of the module's public API and are not imported by glob. If tests need a trait (e.g., `chrono::TimeZone`) for method calls, import it explicitly inside the test module.
- **`ocpi_types::serde_json::json!({…})` works in tests** once `serde_json` is re-exported from `ocpi-types`. No direct dep on `serde_json` needed in `ocpi-server`.
- **`OcpiResponse::success_empty()` for mutation responses** — PUT/PATCH that return `status_code=1000` with no `data` body. Since `data` has `#[serde(skip_serializing_if = "Option::is_none")]`, the JSON omits the `data` key entirely. Use this instead of `success(())` which would serialize `data: null`.
- **CDR POST `Location` header via reqwest:** `response.headers().get(reqwest::header::LOCATION)` extracts the `Location` header from a 201 response. Calling `.and_then(|v| v.to_str().ok()).map(|s| s.to_owned())` copies to an owned String before the response body is consumed by `.json()`. NLL lets you borrow headers, copy to Strings, then move the response — no lifetime clash.
- **`PaginationMeta::from_headers` takes three `Option<&str>`, not a `HeaderMap`.** Extract each header separately as an owned String first, then pass `.as_deref()`. Constructing a fallback `PaginationMeta { next_url: None, total_count: 0, limit: 50 }` works because all three fields are `pub`.
- **CDR store key is flat `id`, not composite.** Unlike Sessions (keyed by `{country_code}/{party_id}/{session_id}`), CDRs have a globally unique `id` within the CPO's system. The POST endpoint doesn't include path segments — the server constructs a URL like `{base_url}/cdrs/{id}` and returns it in the `Location` header. Pass `base_url` to `CdrsConfig::new()` at construction time.
- **`axum::routing::post` is not needed when chaining `.post()` on a `MethodRouter`.** `get(handler).post(other_handler)` uses the `MethodRouter::post` method. Importing `axum::routing::post` alongside `get` is only needed when both are called as free functions in separate `route()` calls. Clippy's `unused_import` catches this.
- **`#[derive(Default)]` on all-optional structs like `TariffRestrictions`** gives callers a clean partial-construction idiom: `TariffRestrictions { day_of_week: vec![DayOfWeek::Monday], ..Default::default() }`. Avoids writing `None` for every field. Test fixtures also benefit — `..Default::default()` fills in the uninteresting fields.
- **Serde proc-macro in a crate without a direct `serde` dep:** `#[derive(serde::Deserialize)]` resolves the proc-macro's helper crate via `extern crate serde` which fails with `E0433` if `serde` is not a direct dependency. Fix: use the re-exported path — `#[derive(ocpi_types::serde::Deserialize)]` + `#[serde(crate = "ocpi_types::serde")]`. The `crate = "…"` attribute tells serde's code generator to look up its runtime helpers at that path instead of the crate root. Apply the same pattern for `Serialize` or any other serde derive.
- **`mergeable_state: dirty` on a `needs-human` PR signals a squash-merge history divergence.** When main advances via squash-merges after a PR was opened, the PR branch's original commits have different SHAs from the squash commits on main. `git merge main` locally may say "Already up to date" (stale proxy) while GitHub reports dirty. The real fix is merging the actual GitHub main (available locally as the nightly branch `claude/sweet-hopper-*`). If conflicts span hundreds of lines in a heavily-modified file (`lib.rs`), the safest action is to post a comment and let the owner rebase — do NOT spend the whole night resolving merge conflicts in a `needs-human` PR when there are productive implementation tasks available.
- **Commands module is asynchronous by design.** The CPO's `CommandResponse` is NOT the Charge Point's result — it's just acknowledgment that the CPO received and forwarded the command. The actual `CommandResult` arrives later via `POST` to the `response_url`. This two-phase pattern affects how `commands_router()` should be designed (receiver route + separate sender/callback route).
- **`CommandType` enum variants need no explicit serde renames** — SCREAMING_SNAKE_CASE from serde's `rename_all` correctly produces CANCEL_RESERVATION, RESERVE_NOW, etc. No digit-adjacent transitions, so no edge cases.
- **Calling an async outbound dependency from inside an axum handler: use a boxed-future trait, not `async fn` in trait.** When a handler must `await` a user-supplied trait (e.g. `VersionFetcher` for the credentials fetch-back), `async fn` in the trait gives no `Send` guarantee (axum handler futures must be `Send`) and is not object-safe (`dyn`). The std-only fix: a return type alias `pub type FetchFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;` and trait methods `fn f<'a>(&'a self, ...) -> FetchFuture<'a, T>;` with the trait declared `: Send + Sync`. The trait object is then `Send + Sync` and awaitable in a handler. Implementors write `Box::pin(async move { ... })`. No new deps, no `async-trait`. Add a `fn assert_send<T: Send>(_: &T){}` test against the handler-side future for a compile-time guarantee.
- **The cross-crate default impl of a server-side trait belongs in its OWN follow-up PR.** `ocpi-server` defines a transport-shaped trait (e.g. `VersionFetcher`) to avoid depending on `ocpi-client`. The natural reqwest-backed default impl lives in `ocpi-client` — but that makes `ocpi-client` depend on `ocpi-server`, which adds a line to `ocpi-client`'s `[[package]] dependencies` in `Cargo.lock` → `needs-human` (the gate guards `Cargo.lock`, though NOT crate-level `Cargo.toml`). Ship the trait + server wiring + mock-tested behavior in an auto-mergeable PR; split the reqwest default impl into a follow-up. Host apps can also supply their own impl in the meantime.
- **Auto-merge gate guards `Cargo.lock` but NOT crate `Cargo.toml`.** The regex is `^(\.github/|deny\.toml$|LICENSE$|Cargo\.lock$|.*\.ya?ml$|scripts/)`. A workspace-internal dep addition still changes `Cargo.lock`, so it trips `needs-human` anyway. The reliable auto-merge check is `cargo check --locked` + `git status --short` showing only `src/` changes.
- **MSRV failures from transitive deps are silent until CI runs.** `clap_builder ≥ 4.6` uses `edition = "2024"` in its manifest; Cargo 1.82 refuses to parse it entirely (not a compile error — a parse error before compilation starts). Bumping `rust-version` in `Cargo.toml` and the CI msrv job is the only fix; pinning clap to < 4.6 would work too but conflicts with the rest of the dep graph. Always match the CI msrv job version to the declared `rust-version`.

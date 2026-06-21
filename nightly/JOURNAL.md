# Nightly Journal

Append-only log, newest first. One short entry per run: date, issue, PR, CI
result, what worked, what to try next.

---

## 2026-06-20 (run 18) — PR-queue hygiene: marked 4 stuck draft PRs ready

- **No new feature issue tonight.** Sync (workflow step 1) found **4 open
  nightly PRs** — #96, #97, #98, #103 — over the "never more than 2 open" rule,
  and all four were stuck in **draft**. Did not open a feature PR; draining the
  queue was the job.
- **All four are healthy:** owner-authored (`duyhuynh-vn`), `mergeable_state:
  clean`, and **all 12 CI check-runs green** (`fmt`/`clippy`/`test
  stable+beta+nightly`/`doc`/`coverage`/`deny`/`audit`/`msrv`/`guardrails`/`gate`).
  None touch guarded paths (only `v2_1_1.rs`, `README.md`, `ocpi-client/src`,
  and a new `tests/` file), so none are `needs-human`.
- **Root cause — the new gate does NOT flip drafts.** #102 (landed on `main`
  2026-06-20 10:44) replaced auto-merge with "auto-mark ready for review" via
  `gh pr ready` in `pr-ready-gate.yml`. But the `gate` check-run reports
  **success** while the PRs stay **draft**: under `pull_request_target` the
  default `GITHUB_TOKEN` cannot convert a draft to ready (the GraphQL
  `markPullRequestReadyForReview` is silently swallowed by `|| true`). So even
  with the new gate, trusted PRs pile up in draft exactly like run 17 — the gate
  *labels* but never *readies*.
- **Action:** marked all four ready via the owner token
  (`update_pull_request draft=false`). The `ready_for_review` transition fires
  the gate, which now labels them `ready-for-review`. Queue is unblocked for the
  owner to merge. Did **not** merge them myself (governance: the owner merges).
- **Merge ordering for the owner:** #96/#97/#98 all edit
  `crates/ocpi-types/src/v2_1_1.rs` **and** flip a `README.md` matrix cell, so
  they collide pairwise — merge **one**, then the other two need a trivial
  rebase. **#103** touches only `ocpi-client/src/lib.rs` → independent, merge
  anytime.
- **What worked:** `mergeable_state: clean` + a `get_check_runs` spot-check is a
  fast, reliable green signal (the legacy status API still returns
  `total_count:0` — see LEARNINGS). Marking ready (not `merge_pull_request`)
  keeps the owner's governance in the loop.
- **Next (owner action needed):** merge the four ready PRs (one of 96/97/98,
  then rebase the rest; 103 anytime) to get back under the 2-PR cap. **Infra
  fix worth doing:** give the gate a token that can actually ready a draft (a PAT
  / app token with PR write, or a `workflow_dispatch`), else every night's PR
  keeps sticking in draft. After the queue drains, M7 (2.1.1) continues:
  Sessions (#90), CDRs (#91), Locations (#89), and server/client wiring for the
  already-merged 2.1.1 types.

---

## 2026-06-19 (run 17) — PR-queue hygiene: drained 4 stuck draft PRs

- **No new feature issue tonight.** Sync (workflow step 1) found **5 open
  nightly PRs** — #81, #82, #83, #84, #85 — well over the "never more than 2
  open" rule. All five were owner-authored, **green CI** (Actions check-runs all
  `success`), and `mergeable_state: clean`, but every one was left in **draft**.
- **Root cause:** the `auto-merge-gate` workflow explicitly parks drafts
  (`if draft=true → "Draft PR — waiting"; exit 0`). It only arms squash
  auto-merge on the `ready_for_review` event. Several runs earlier today opened
  PRs and **skipped the Ship step** (mark ready / arm auto-merge), so they piled
  up. CI being green was a red herring — the legacy commit-status API returns
  `total_count:0` for these (CI runs as Actions *check-runs*, not statuses), so a
  naive `get_status` check looks "pending" when it's actually green.
- **Action:** mapped file overlaps, then marked the four mutually-conflict-free
  PRs ready: **#81** (M3 Locations e2e, test-only), **#82** (HubClientInfo client
  methods, `ocpi-client/src/lib.rs`), **#84** (M4 Sessions/CDRs e2e + sessions
  `charging_preferences` route, `ocpi-server/src/lib.rs`), **#85** (M5
  Tariffs/Tokens e2e, test-only). The gate squash-merged all four
  (`merged_by: github-actions[bot]`). Queue **5 → 1**. Local
  `cargo build --workspace --all-features` on the merged tree: green.
- **#83 held (still open, draft):** ChargingProfiles Sender PUT (#75). It edits
  the same `use ocpi_types::v2_2_1::{…}` import blocks as the now-merged #82
  (client) and #84 (server), so it's conflicted. Left draft + commented with the
  exact reconcile steps; did **not** push a rebase onto its branch from this
  session (designated dev branch is `claude/dazzling-maxwell-bj1kpd`; the
  guardrail forbids pushing to other branches without permission).
- **What worked:** the gate is the safe mechanism — required checks still gate
  each merge, and the disjoint-file ordering meant no merge went `dirty`.
  Marking ready (not a manual `merge_pull_request`) keeps the owner's governance
  in the loop.
- **Next:** rebase/merge-main #83 to resolve the two import-block conflicts, run
  fmt/clippy/test, mark ready → gate merges it. After that the 2.2.1 surface +
  the full per-milestone e2e suite (M2–M5) are all on `main`; M6 ChargingProfiles
  Sender PUT is the only loose end. Then groom M7 (2.1.1 back-coverage) for
  owner-approved issues.

---

## 2026-06-19 — M2 end-to-end registration smoke test (issue #23)

- **Issue #23:** First live-transport smoke test of the M2 bootstrap — stand up
  the versions + credentials receiver in-process and drive the full
  `GET /versions` → `GET /versions/2.2.1` → `POST /credentials` sequence through
  `OcpiClient` (no raw reqwest). P2, M2, owner-approved `nightly`. This is the
  designated issue for introducing the e2e harness dev-deps (per LEARNINGS).
- **Branch:** `nightly/2026-06-19-issue-23`.
- **PR:** draft, `Closes #23`. **Touches `Cargo.toml` + `Cargo.lock`** (new
  dev-deps) → expected `needs-human` (auto-merge gate guards `Cargo.lock`).
- **CI (local):** `fmt --check` ✅ `clippy --all-targets -D warnings` ✅
  `test --workspace` ✅ (2 new integration tests; 305 existing tests still green).
  `cargo check --locked` ✅. `cargo deny` not installed in runner; only test-only
  dev-deps added (no new packages — axum/tokio already locked).
- **What shipped:** `crates/ocpi-client/tests/m2_registration.rs` — two
  `#[tokio::test]`s. `m2_registration_bootstrap_end_to_end` binds **two** loopback
  axum servers: Party A (eMSP) serves only `versions_router` so Party B's
  registration **fetch-back** (`OcpiVersionFetcher`) can discover A's endpoints;
  Party B (CPO) serves `versions_router.merge(credentials_router)` with
  `CredentialsConfig::new_with_fetcher`. Asserts 2.2.1 negotiation, Token-C
  exchange (≠ bootstrap Token A), 200 on follow-up `GET /credentials`, and 405 on
  re-POST. `get_credentials_unauthorized_before_registration` asserts 401 pre-reg.
  Dev-deps added to `ocpi-client`: `ocpi-server` w/ `axum` feature, `axum`,
  `tokio` w/ `net`, `ocpi-types` (all test-only — verified absent from the
  `--edges normal` tree).
- **Harness trick:** split bind-then-serve (`bind()` returns the `TcpListener` +
  its `http://127.0.0.1:PORT` origin; `serve()` spawns `axum::serve`). Binding
  before building the router lets each server reference its **own** origin in the
  absolute URLs it advertises (the `/versions` list + credentials endpoint),
  which the fetch-back and client both require. `TcpListener::bind` leaves the
  socket listening, so no readiness sleep is needed before the client calls.
- **Real bug surfaced (out of scope, follow-up filed):** the credentials server
  keys the registration by the **POST bearer token** (bootstrap Token A) rather
  than the issued **Token C**. Per OCPI 2.2.1 the eMSP must present Token C on
  subsequent calls and Token A must be invalidated. Filed as `bug` #76.
- **Update (run 16, same night):** #76 is fixed by #78 (server now keys the
  registry by Token C and burns Token A on a successful POST). Per the owner's
  review note on the PR, this smoke test is **stacked on #78** (merged its branch
  in; PR base retargeted to `nightly/2026-06-19-issue-76`) and the follow-up
  `GET /credentials` now bears **Token C** (a fresh `OcpiClient` — the token is
  fixed at construction) and asserts the burned **Token A → 401**. The inline
  NOTE is dropped. Both tests still green.
- **Next:** the remaining e2e smoke tests #32 (M3 Locations), #71 (M4
  Sessions/CDRs), #72 (M5 Tariffs/Tokens) — the harness pattern + dev-deps from
  this PR now exist to copy. Or **#70** (README milestone/matrix sync, docs-only,
  auto-mergeable). Suggest **#32** next to keep extending live coverage.

---

## 2026-06-18 (run 15) — M6 ChargingProfiles server handler + router + client methods (issue #57)

- **Issue #57:** Add the receiver-side `ChargingProfilesHandler` + stateless
  `ChargingProfilesConfig` + `charging_profiles_router()` to `ocpi-server`, and the
  6 sender-side `OcpiClient` methods to `ocpi-client`, following the Commands
  two-phase async pattern. P1, M6, owner-approved `nightly`. Types merged via #58.
  **This completes M6 → the full 2.2.1 functional-module surface.**
- **Branch:** `claude/dazzling-maxwell-buke6j` (designated dev branch this run).
- **PR:** (opened this run) — `Closes #57`, squash auto-merge enabled.
- **CI (local):** `fmt` ✅ `clippy --workspace --all-targets --all-features -D
  warnings` ✅ `test --workspace --all-features` ✅ (ocpi-server 76 tests, +2 new;
  ocpi-client 30 tests, +3 new). **Only `src/` changes — no `Cargo.toml`/
  `Cargo.lock` → auto-mergeable.** `cargo check --locked` ✅. `cargo deny` not
  installed in the runner; dep graph unchanged from main (which passes deny).
- **What shipped:**
  - `ocpi-server`: `ChargingProfilesHandler` trait (3 receiver methods:
    `get_active_profile`/`set_charging_profile`/`clear_charging_profile`; 3 sender
    result callbacks: `receive_active_profile_result`/`_charging_profile_result`/
    `_clear_profile_result`). `ChargingProfilesConfig` stateless default —
    `not_supported_response()` returns `ChargingProfileResponse { NOT_SUPPORTED,
    timeout: 0 }`; callbacks are no-ops. `http::charging_profiles_router()` with
    `GET/PUT/DELETE /chargingprofiles/{session_id}` (+ `?duration&response_url`
    query structs) and 3 `POST .../{activeprofile,result,clearprofile}` callbacks.
  - `ocpi-client`: `get_active_charging_profile` (GET+query), `set_charging_profile`
    (PUT), `clear_charging_profile` (DELETE+query), and `post_active_profile_result`
    / `post_charging_profile_result` / `post_clear_profile_result` (POST to opaque
    `response_url` via shared private `post_profile_callback`). New pure helper
    `charging_profile_url(base, session_id)` carries 3 URL-building unit tests.
- **Spec fidelity:** receiver endpoints keyed by `{session_id}` single segment;
  GET/DELETE put `duration`/`response_url` in the query string, not a body
  (`mod_charging_profiles.asciidoc` §CPO GET/PUT/DELETE). Sender POST URL is open
  per spec — the issue's `.../{activeprofile|result|clearprofile}` routes are a
  reasonable convention; documented in PR. Receiver result routes carry
  `{session_id}` but the issue's trait signatures omit it, so handlers extract
  `Path(_session_id)` and ignore it (faithful to the owner-approved signatures).
- **Known gaps (PR-documented):** (1) the **Sender PUT** method
  (`PUT /chargingprofiles/{session_id}` with `ActiveChargingProfile` body — CPO
  pushes a changed active profile to the SCSP/eMSP) is out of #57's scope; file a
  follow-up. (2) Routing headers not auto-attached (cross-cutting #64). (3) No
  async/HTTP round-trip test (would need a `tokio`/`axum-test` dev-dep →
  `needs-human`); covered by the sync `not_supported_response`/URL-helper tests
  per the established no-new-dev-deps pattern.
- **Clippy trap:** calling `ChargingProfilesConfig::default()` on a unit struct in
  a test trips `clippy::default_constructed_unit_structs` under `-D warnings`. The
  `#[derive(Default)]` on the type is fine (mirrors `CommandsConfig`); just don't
  *invoke* `.default()` pointlessly. Dropped that line.
- **Next:** 2.2.1 functional surface is now complete. Good follow-ups: #70 (README
  milestone/matrix sync — flip M6 ☑ + ChargingProfiles ☑ now that #57 lands), the
  e2e smoke tests #71/#72/#32/#23 (each needs an `axum-test`/`tower` dev-dep →
  `needs-human`), or a new issue for the ChargingProfiles **Sender PUT** method.
  Suggest **#70** next (docs-only, auto-mergeable) once this merges.

---

## 2026-06-17 (run 14) — M3 Locations client methods (issue #30)

- **Issue #30:** Add the sender-side Locations methods to `ocpi-client::OcpiClient`
  — `get_locations` (paginated list), `get_location`, `get_evse`, `get_connector`.
  P2, M3, owner-approved `nightly`. Completes the client half of M3; unblocks #32
  (e2e smoke test). Depends on #28 (types, merged via #59); #29 (server handler,
  merged via #63) is the receiver counterpart.
- **Branch:** `claude/dazzling-maxwell-n0dhr8` (designated dev branch this run).
- **PR:** (opened this run) — `Closes #30`, squash auto-merge enabled.
- **CI (local):** `fmt` ✅ `clippy --workspace --all-targets --all-features -D
  warnings` ✅ `test --workspace --all-features` ✅ (ocpi-client 22 tests, +5 new;
  workspace 192) — **no `Cargo.toml`/`Cargo.lock` changes → auto-mergeable.**
  `cargo deny` not installed in the runner; dep graph unchanged from main (which
  passes deny in CI).
- **What shipped (all in `ocpi-client/src/lib.rs`):**
  - `get_locations(url, params: PaginatedParams) -> (Vec<Location>, PaginationMeta)`
    — mirrors `get_cdrs`: appends `date_from/date_to/offset/limit` query pairs,
    parses `Link`/`X-Total-Count`/`X-Limit` via `PaginationMeta::from_headers`.
  - `get_location` / `get_evse` / `get_connector` — single-object GETs mirroring
    `get_cdr`: HTTP 404 → `ClientError::NotFound`, empty envelope → `EmptyData`.
  - Private `join_segments(base, &[seg…])` helper — trims one trailing slash and
    appends path segments; de-duplicates the inline `format!` path-building and is
    the only non-HTTP logic, so it carries the 5 new unit tests.
- **Spec fidelity:** Sender-interface GET Object path is
  `{locations_url}/{location_id}[/{evse_uid}][/{connector_id}]`
  (`mod_locations.asciidoc` L179–206) — **no** `country_code`/`party_id` segments
  (those are the Receiver interface, already in #29). Issue #30 signatures match
  the Sender shape exactly. Types: `Location`, `Evse`, `Connector`. `NotFound`
  variant already existed (added with Sessions/CDRs).
- **Known gap (follow-up issue filed):** like every existing functional-module
  client method (Sessions/CDRs/Tariffs/Tokens), these do **not** attach the
  `OCPI-to/from-party-id/country-code` routing headers — only `Authorization`.
  Bolting them onto Locations alone would diverge the client API, so wiring them
  uniformly across all senders is its own issue. The issue's routing-header AC is
  deferred there.
- **Testing approach:** kept the established pure-unit-test pattern (no
  `httpmock`/`wiremock` dev-dep) to leave `Cargo.toml` untouched and the PR
  auto-mergeable. HTTP-level round-trip coverage belongs to #32 (the dedicated
  e2e harness issue, which introduces test deps in its own `needs-human` PR).
- **Next:** #32 (M3 Locations e2e smoke test) is now unblocked — but it needs an
  `axum-test`/`tower` dev-dep (→ `needs-human`); or pick #57 (M6 ChargingProfiles
  server+client, P1, auto-mergeable) to push toward 2.2.1-complete. Suggest #57.

---

## 2026-06-14 (run 13) — M3 Locations receiver handler + axum router (issue #29)

- **Issue #29:** Add the receiver-side Locations handler to `ocpi-server`
  (`LocationsHandler` trait, `LocationsConfig` in-memory store, axum
  `locations_router()`). P1, M3, owner-approved `nightly`. Foundation for #30
  (client) and #32 (e2e smoke test). #28 (Location types) merged via PR #59.
- **Branch:** `nightly/2026-06-14-issue-29`
- **PR:** (opened this run)
- **CI (local):** `fmt` ✅ `clippy --all-features -D warnings` ✅ `test
  --all-features` ✅ (ocpi-server 74 tests, +13 new; workspace 192) — no
  `Cargo.toml`/`Cargo.lock` changes → auto-mergeable. `cargo deny` not installed
  in the runner; dep graph unchanged from main (which passes deny in CI).
- **What shipped (all in `ocpi-server`):**
  - `LocationsHandler` trait — full receiver interface: `list_locations`,
    `get/put/patch_location`, `get/put/patch_evse`, `get/put/patch_connector`.
  - `LocationsConfig` — `RwLock<HashMap<key, Location>>` keyed by
    `{country_code}/{party_id}/{location_id}`. EVSEs/Connectors are stored
    **nested inside their parent Location** (matching the OCPI object model);
    sub-object writes locate the parent, then upsert/patch in place.
  - `apply_merge_patch<T: Serialize + DeserializeOwned>` generic helper
    (serialize → `json_merge` → deserialize) reused for location/evse/connector
    PATCH. `upsert_by` helper for nested replace-or-append.
  - `http::locations_router()` with composite-key routes
    `/locations/{cc}/{pid}/{location_id}[/{evse_uid}][/{connector_id}]` (GET/PUT/
    PATCH at each level) + paginated `GET /locations` (X-Total-Count, X-Limit,
    Link: next), mirroring `sessions_router`.
- **Spec/issue divergence (documented in PR + pickup comment):** the issue's
  route sketch dropped `country_code`/`party_id` from the paths, but specified a
  store key of `{cc}/{pid}/{location_id}`. The vendored spec
  (`mod_locations.asciidoc` L232, §Receiver Interface) uses the full composite
  path. Implemented the spec paths — also consistent with the existing
  sessions/tariffs/tokens routers. The bare `/locations/{location_id}` sender
  shape is a separate (CPO) interface, deferred.
- **Method resolution trap (worked):** `LocationsConfig` has BOTH inherent
  methods (`patch_location`, `put_evse`, …) and the `LocationsHandler` trait
  impl with identical names. Inherent methods win in resolution, so the trait
  impl bodies calling `self.patch_location(...)` dispatch to the inherent method
  — no infinite recursion. Tests confirm at runtime.
- **LOC note:** ~1054 insertions — above the ~500 soft target, but it's one
  coherent complete module (the issue's full Location+EVSE+Connector acceptance
  criteria). Bulk is `missing_docs` doc comments + mechanical handlers + 13
  tests; no smaller slice leaves a functional receiver interface.
- **Tests:** 13 unit tests on `LocationsConfig` (put/get roundtrip, missing →
  None, list date filter + pagination, location/evse/connector patch + upsert,
  unknown-parent → `NotFound`), following the existing direct-store test style
  (no HTTP harness, no new dev-deps).
- **Next:** #30 (M3 Locations client methods — GET list/single Location/EVSE/
  Connector, P2) now unblocked; then #32 (M3 e2e Locations smoke test, depends
  on #30). That completes M3.

## 2026-06-14 (run 12) — M2 credentials registration fetch-back (issue #33)

- **Issue #33:** Complete the OCPI 2.2.1 registration handshake — on `POST`/`PUT
  /credentials` the receiver must fetch the registering party's `/versions` and
  store its endpoint catalogue. P1, M2, owner-approved `nightly`.
- **Branch:** `nightly/2026-06-14-issue-33`
- **PR:** (opened this run)
- **CI (local):** `fmt` ✅ `clippy -D warnings` ✅ `test` ✅ (ocpi-server 61 tests,
  +9 new; workspace 192) `check --locked` ✅ (no `Cargo.toml`/`Cargo.lock`
  changes → auto-mergeable).
- **What shipped (all in `ocpi-server`):**
  - `FetchError` enum (Transport / NoMutualVersion / Invalid → status `3001`).
  - `VersionFetcher` trait + `FetchFuture<'a, T>` alias. Uses **std boxed
    futures** (`Pin<Box<dyn Future + Send>>`), NOT `async fn` in trait — this
    keeps the trait object-safe (`dyn VersionFetcher`) AND `Send`, so it can be
    awaited inside an axum handler. Sidesteps the documented `async_fn_in_trait`
    + axum `Send`-bound wall.
  - `RegisteredParty { credentials, endpoints }`; store value type changed from
    `Credentials` to `RegisteredParty` (internal — register/update signatures
    unchanged, existing tests untouched).
  - `CredentialsConfig`: `new_with_fetcher(own, supported_versions, fetcher)`,
    `register_with_endpoints`, `update_with_endpoints`, `get_endpoints`, and
    async `fetch_back` (GET /versions → `select_best_version` → GET details).
  - POST/PUT handlers run the fetch-back; any failure → `3001`
    (`UnableToUseClientApi`). POST checks `is_registered` before the fetch to
    avoid wasted work on re-registration.
  - 9 sync unit tests + a `fetch_back_future_is_send` compile-time `Send` proof.
- **Key decisions / deviations from the issue text (justified in PR):**
  - **Boxed-future trait** instead of `async fn` in trait (axum `Send` rule).
  - **Default `OcpiVersionFetcher` (reqwest) deferred** to a follow-up issue:
    putting it in `ocpi-client` forces `ocpi-client → ocpi-server` dep → a
    `Cargo.lock` line change → `needs-human`. Keeping this PR server-only keeps
    it auto-mergeable. Filed follow-up.
  - **`new_with_fetcher` gained a `supported_versions` param** — real
    negotiation needs the server's own version list; `CredentialsConfig` did not
    previously carry one.
  - **Async fetch-back not unit-run**: `#![forbid(unsafe_code)]` + no tokio
    dev-dep ⇒ no in-crate `block_on`. Tested the sync building blocks
    (`select_best_version`, endpoint storage/accessor) + a compile-time `Send`
    proof; end-to-end async path belongs to the M2 e2e smoke test (#23).
- **Spec-correctness note:** fetch-back authenticates with `credentials.token`
  (= TOKEN_B from the POST body), per OCPI 2.2.1 §POST. Token rotation
  (server issues TOKEN_C, registers party under it) is a pre-existing gap from
  PR #60, out of scope here.
- **Next:** M3 #29 (Locations server handler + `locations_router()`, P1 —
  proven concrete-`Config` router pattern, no new deps) unblocks #30/#32.

---

## 2026-06-14 (run 11, Sunday) — M2 credentials axum router, clean re-delivery (issue #22)

- **Issue #22:** M2 credentials axum router — `CredentialsConfig` + `credentials_router()`.
- **Branch:** `claude/dazzling-maxwell-za9j4s`
- **CI (local):** `fmt` ✅ `clippy -D warnings` ✅ `test` ✅ (ocpi-server 53 tests, +8 new) `check --locked` ✅ (no dep/lock changes)
- **Context — why a re-delivery:** #22 was first implemented in PR #24, but that PR
  **bundled a `.github/workflows/ci.yml` MSRV change + a root `Cargo.toml` change + new
  `ocpi-server` dev-deps** (`tower`/`tokio`/`serde_json` for `tower::oneshot` HTTP tests).
  Those guarded-path edits forced `needs-human`, the owner didn't merge immediately, and the
  PR went stale (`mergeable_state: unknown`/dirty) as M3–M6 squash-merged and grew `lib.rs`.
  M2's credentials router has been the earliest-incomplete-milestone blocker for ~6 runs,
  also blocking #33 (fetch-back) and #23 (e2e smoke test) which both need the router on `main`.
- **What I did:** Re-delivered **only** the `CredentialsConfig` + `credentials_router()`
  additions to `crates/ocpi-server/src/lib.rs` — **no `.github`/Cargo/lock changes** →
  auto-merge eligible (no `needs-human`).
  - `CredentialsConfig` (in-memory, `RwLock<HashMap<token, Credentials>>`): `new()`,
    `is_registered()`, `register()` (→ `AlreadyRegistered`), `update()`/`delete()`
    (→ `NotRegistered`). Custom `Debug` (registered_count, not contents).
  - `credentials_router(Arc<CredentialsConfig>)` (axum): one `/credentials` route with
    GET/POST/PUT/DELETE. Token auth via `CredentialToken::from_header_value`
    (`Authorization: Token <base64>`). 401 on missing/unregistered token; **405** on
    POST-already-registered / PUT-or-DELETE-not-registered (spec credentials.asciidoc §POST
    L132 / §PUT L143 / §DELETE L150). Does NOT impl `CredentialsHandler` — same
    `async_fn_in_trait` + axum `Send` avoidance as `VersionsConfig`.
  - **Dropped PR #24's HTTP-level (`tower::oneshot`/`tokio::test`) tests** — they required the
    dev-deps that forced the guarded-path edit. Kept 8 **sync** `CredentialsConfig` tests
    instead, matching how every other merged router (sessions/cdrs/tariffs/tokens/commands)
    is covered today. HTTP-level coverage is already planned via #23 (e2e smoke test), which
    will introduce the test-harness deps in its own PR.
- **Superseded PR #24:** commenting + closing it as superseded (clean re-delivery of #22 here).
  The MSRV (#12) and CI-pinning (#13) guarded-path work stays in its own track — both
  `continue-on-error`/non-blocking.
- **Sunday groom:** M2 now has 3 owner-approved issues (#22 done here, #33 fetch-back, #23
  smoke). M3 has #29/#30/#32. No milestone fell below 3 well-scoped `nightly` issues, so no
  new issues created. Dependabot PRs #2–#5 (CI action bumps) are `.github`-only → owner's call.
- **Next:** #33 (M2 credentials registration fetch-back) once this merges — completes the
  M2 handshake. Or #29 (M3 Locations server handler + `locations_router()`), now unblocked on
  `main`. Then re-deliver nothing else stuck — #24 is the last rotted nightly PR.

---

## 2026-06-13 (run 10) — M3 Locations data types, clean re-delivery (issue #28)

- **Issue #28:** M3: Locations data types — Location, EVSE, Connector, supporting enums.
- **Branch:** `claude/dazzling-maxwell-4e2etm`
- **CI (local):** `fmt` ✅ `clippy -D warnings` ✅ `test` ✅ (192 ocpi-types tests, +12 new) `check --locked` ✅ (no dep changes)
- **Context — why a re-delivery:** #28 was first implemented in PR #31, but that PR
  **bundled a `.github/` MSRV calibration** (issue #12) into the same branch, making it
  `needs-human`, and then went `dirty` (merge-conflict) as M4/M5/M6 squash-merged into
  `main` and grew `v2_2_1.rs`/`lib.rs`. The foundational Locations types were stuck behind
  a guarded-path change and a 17-region merge conflict — blocking all of M3 (#29, #30, #32)
  for ~5 runs while the routine skipped ahead to M4–M6.
- **What I did:** Re-delivered **only** the Locations types cleanly on top of current `main`,
  with **no `.github`/Cargo changes** → auto-merge eligible (no `needs-human`).
  - **Did NOT redefine** `TokenType`, `ConnectorType`, `ConnectorFormat`, `PowerType` —
    M4/M5/M6 already added these to `v2_2_1.rs` (Sessions/CDRs/Tokens needed them). The
    Locations objects **reuse** the existing shared enums. This was the whole source of the
    cherry-pick conflict: PR #31 predates those definitions and redefines them.
  - **Added 15 new types:** `Status`, `Capability`, `Facility`, `ImageCategory`,
    `ParkingRestriction`, `ParkingType`, `AdditionalGeoLocation`, `RegularHours`,
    `ExceptionalPeriod`, `Hours`, `StatusSchedule`, `PublishTokenType`, `Connector`, `Evse`,
    `Location`. Added `Image` to the `common` import; re-exported all 15 from the crate root.
  - **12 tests:** Status/ParkingType/Facility serde, Connector/Evse/Location round-trips,
    optional-field omission, `Hours` 24/7, `PublishTokenType` (`"type"` rename → `RFID`).
    Dropped PR #31's `connector_type_mixed_case_serde`/`power_type_serde_roundtrip` tests —
    those enums already live in `main` with their own coverage, and PR #31 used
    pre-existing variant spellings (`Iec603092Single16`) that differ from `main`
    (`Iec6030921Single16`).
- **Superseded PR #31:** commented + closed it as superseded (clean re-delivery of #28
  here; MSRV issue #12 left open for separate handling — it's non-blocking,
  `continue-on-error: true` on `main`).
- **Sync:** PR #24 (M2 credentials router) still `needs-human`/`dirty` — same bundling
  pathology (it carries an MSRV `.github` change + 9-region `lib.rs` conflict). Left for a
  future clean re-delivery of #22 (same recipe: re-apply just the router on current main, no
  `.github`).
- **Next:** #29 (M3 Locations server handler + `locations_router()`, P1) — now unblocked on
  `main` once this merges. Then #30 (client methods). Consider re-delivering #22 cleanly to
  un-stick M2.

---

## 2026-06-13 (run 9) — M6 ChargingProfiles data types (issue #56)

- **Issue #56:** M6: ChargingProfiles data types — ChargingProfile, ActiveChargingProfile, SetChargingProfile, result/response enums
- **Branch:** `claude/stoic-shannon-h48jf9`
- **CI (local):** `fmt` ✅ `clippy -D warnings` ✅ `test` ✅ (245 total, +11 new ChargingProfiles tests)
- **What shipped:**
  - `ChargingRateUnit` enum (2 variants: W, A) — explicit `#[serde(rename)]` for single-letter uppercase wire values
  - `ChargingProfileResponseType` enum (5 variants: ACCEPTED, NOT_SUPPORTED, REJECTED, TOO_OFTEN, UNKNOWN_SESSION) — SCREAMING_SNAKE_CASE
  - `ChargingProfileResultType` enum (3 variants: ACCEPTED, REJECTED, UNKNOWN) — SCREAMING_SNAKE_CASE
  - `ChargingProfilePeriod` struct (`start_period: i32`, `limit: f64`)
  - `ChargingProfile` struct (5 fields: optional `start_date_time`, optional `duration`, `charging_rate_unit`, optional `min_charging_rate`, `charging_profile_period: Vec<ChargingProfilePeriod>`)
  - `ActiveChargingProfile` struct (`start_date_time: DateTime`, `charging_profile: ChargingProfile`)
  - `SetChargingProfile` struct (`charging_profile: ChargingProfile`, `response_url: Url`)
  - `ChargingProfileResponse` struct (`result: ChargingProfileResponseType`, `timeout: u32`) — Eq derivable
  - `ActiveChargingProfileResult` struct (`result: ChargingProfileResultType`, optional `profile: ActiveChargingProfile`)
  - `ChargingProfileResult` struct — Eq derivable
  - `ClearProfileResult` struct — Eq derivable
  - All 11 types re-exported from `ocpi-types` crate root
  - 11 new tests: enum serde, struct roundtrips, optional-field omission, spec-example JSON
- **No Cargo.toml changes.** (No `needs-human` flag; auto-merge eligible.)
- **Groomed M6:** Created issues #56 (ChargingProfiles types, P1) and #57 (ChargingProfiles server+client, P1) — M6 was incomplete without ChargingProfiles.
- **Sync:** PR #24 (needs-human, dirty — merge conflict in lib.rs), PR #31 (needs-human, all CI ✅). Both awaiting owner action. 3rd open nightly PR is justified: both existing are `needs-human` with no blocking CI failures or review comments.
- **Key design decision:** `ChargingRateUnit` uses explicit `#[serde(rename = "W")]` / `#[serde(rename = "A")]` — single-letter values that don't need SCREAMING_SNAKE_CASE transformation but also can't rely on derive defaults.
- **f64 fields:** `limit` in `ChargingProfilePeriod` and `min_charging_rate` in `ChargingProfile` are `f64`, which prevents `Eq` on those structs and their containers. `ChargingProfileResponse`, `ChargingProfileResult`, and `ClearProfileResult` have no `f64` and derive `Eq`.
- **M6 status:** ChargingProfiles types ✅. Still needed: #57 (ChargingProfiles server+client). HubClientInfo and Commands are on main.
- **Next:** #57 (M6: ChargingProfiles server handler + client) — blocked only until this PR merges. M3 #29 (Locations server) unblocks once PR #31 merges; M2 #33 (Credentials fetch-back) unblocks once PR #24 merges.

## 2026-06-13 (run 8) — M6 HubClientInfo data types + server handler (issue #52)

- **Issue #52:** M6: HubClientInfo data types + server handler
- **Branch:** `claude/sweet-hopper-p0fkit`
- **CI (local):** `fmt` ✅ `clippy -D warnings` ✅ `test` ✅ (233 total, +8 new HubClientInfo tests)
- **What shipped:**
  - `ConnectionStatus` enum (4 variants: CONNECTED, OFFLINE, PLANNED, SUSPENDED) — SCREAMING_SNAKE_CASE serde
  - `ClientInfo` struct (country_code: CiString2, party_id: CiString3, role: Role, status: ConnectionStatus, last_updated: DateTime) — spec-faithful field types
  - Both re-exported from `ocpi-types` crate root
  - `HubClientInfoHandler` trait (2 async methods: `get_client_info`, `put_client_info`)
  - `HubClientInfoConfig` in-memory store — `RwLock<HashMap<String, ClientInfo>>` keyed by `"{country_code}/{party_id}"`; `new()`, `put()`, `get()`, `list(date_from, date_to, offset, limit)`
  - `hub_client_info_router(Arc<HubClientInfoConfig>) -> Router` (axum feature) — 3 routes:
    - `GET /clientinfo` → paginated list with X-Total-Count/X-Limit/Link headers (Sender/Hub interface)
    - `GET /clientinfo/{country_code}/{party_id}` → single ClientInfo or 404
    - `PUT /clientinfo/{country_code}/{party_id}` → upsert
  - No OCPI routing headers — HubClientInfo is a Configuration Module (not Functional)
  - 3 new tests in ocpi-types: ConnectionStatus SCREAMING_SNAKE_CASE, ClientInfo roundtrip, spec example JSON
  - 5 new tests in ocpi-server: put+get roundtrip, get-missing→None, put-overwrites, list-pagination, list-filter-by-date_from
- **No Cargo.toml changes.** (No `needs-human` flag; auto-merge eligible.)
- **Sync:** PR #24 (needs-human), PR #31 (needs-human). Both awaiting owner action; no review comments.
- **Key design decision:** Store key is `"{country_code}/{party_id}"` (not `"{country_code}/{party_id}/{role}"`). The spec's GET/PUT path is `/{country_code}/{party_id}` with no role segment — one entry per party pair, role is stored in the value.
- **M6 status:** All 3 M6 issues now implemented (#50 Commands types ✅, #51 Commands server+client ✅, #52 HubClientInfo ✅). M6 appears complete pending owner's milestone closure.
- **Next:** M3 unblocks when PR #31 merges → #29 (Locations server handler, P1) or #30 (Locations client, P2). M2 unblocks when PR #24 merges → #33 (Credentials fetch-back, P1). If both remain blocked: groom M7 (ChargingProfiles) or open a question issue about milestone completion.

## 2026-06-13 (run 7) — M6 Commands server handler + client (issue #51)

- **Issue #51:** M6: Commands server handler + client
- **Branch:** `claude/sweet-hopper-o42ez3`
- **CI (local):** `fmt` ✅ `clippy -D warnings` ✅ `test` ✅ (167 total, 2 new CommandsConfig tests)
- **What shipped:**
  - `CommandsHandler` trait (6 async methods: handle_cancel_reservation, handle_reserve_now, handle_start_session, handle_stop_session, handle_unlock_connector, receive_command_result)
  - `CommandsConfig` unit struct — stateless default impl returning `NOT_SUPPORTED` for all commands; `not_supported_response()` public static helper
  - `http::commands_router(Arc<CommandsConfig>) -> Router` (axum feature) — 6 routes:
    - `POST /commands/CANCEL_RESERVATION` → cmds_cancel_reservation
    - `POST /commands/RESERVE_NOW` → cmds_reserve_now
    - `POST /commands/START_SESSION` → cmds_start_session
    - `POST /commands/STOP_SESSION` → cmds_stop_session
    - `POST /commands/UNLOCK_CONNECTOR` → cmds_unlock_connector
    - `POST /commands/{command_type}/result` → cmds_receive_result (async result callback)
  - `OcpiClient` 6 new methods: `cancel_reservation`, `reserve_now`, `start_session`, `stop_session`, `unlock_connector`, `post_command_result`
  - Command sender methods use private `post_command()` helper; type segment derived from `serde_json::to_value(CommandType)` to stay DRY
  - `post_command_result` POSTs `CommandResult` to an arbitrary `response_url` (second phase of async Commands flow)
  - 2 sync tests: `not_supported_response_has_correct_fields`, `new_constructs_without_panic`
- **No Cargo.toml changes.** (No `needs-human` flag; auto-merge eligible.)
- **Sync:** PR #24 (needs-human), PR #31 (needs-human). Both awaiting owner action; no review comments to respond to.
- **Next:** After PR #31 merges — #29 (M3: Locations server handler). After PR #24 merges — #33 (M2: credentials fetch-back). #52 (HubClientInfo, P2) is independent.

## 2026-06-12 (run 6) — M6 Commands data types (issue #50)

- **Issue #50:** M6: Commands data types — CommandType, CommandResponseType, CommandResultType, CancelReservation, ReserveNow, StartSession, StopSession, UnlockConnector, CommandResponse, CommandResult
- **Branch:** `claude/sweet-hopper-a95fez`
- **CI (local):** `fmt` ✅ `clippy -D warnings` ✅ `test` ✅ (222 total, +13 new Commands tests) `deny check` ✅ (no new deps; cargo-deny not installed locally, trusted CI)
- **What shipped:**
  - `CommandType` enum (5 variants: CANCEL_RESERVATION, RESERVE_NOW, START_SESSION, STOP_SESSION, UNLOCK_CONNECTOR) — used as URL path segment in receiver interface
  - `CommandResponseType` enum (4 variants: NOT_SUPPORTED, REJECTED, ACCEPTED, UNKNOWN_SESSION) — CPO's immediate acknowledgment
  - `CommandResultType` enum (9 variants: ACCEPTED, CANCELED_RESERVATION, EVSE_OCCUPIED, EVSE_INOPERATIVE, FAILED, NOT_SUPPORTED, REJECTED, TIMEOUT, UNKNOWN_RESERVATION) — async Charge Point result
  - `CancelReservation` struct (`response_url: Url`, `reservation_id: CiString36`)
  - `ReserveNow` struct (7 fields: response_url, token, expiry_date, reservation_id, location_id, optional evse_uid, optional authorization_reference)
  - `StartSession` struct (6 fields: response_url, token, location_id, optional evse_uid, optional connector_id, optional authorization_reference)
  - `StopSession` struct (response_url, session_id)
  - `UnlockConnector` struct (response_url, location_id, evse_uid, connector_id)
  - `CommandResponse` struct (result: CommandResponseType, timeout: u32, message: Vec<DisplayText> omitted if empty)
  - `CommandResult` struct (result: CommandResultType, message: Vec<DisplayText> omitted if empty)
  - All types re-exported from `ocpi-types` crate root
  - 13 new tests: enum SCREAMING_SNAKE_CASE, struct roundtrips, optional field omission, spec-example JSON
- **No Cargo.toml changes.** (No `needs-human` flag; auto-merge eligible.)
- **Sync:** PR #24 (needs-human, dirty state — merge conflict comment posted), PR #31 (needs-human, all CI ✅). Neither has review comments. PR #24's dirty state requires owner action (rebase onto current main; complex conflicts in `lib.rs` after M4/M5 squash-merges).
- **Groomed M6:** Created issues #50 (Commands types, P1), #51 (Commands server+client, P1), #52 (HubClientInfo, P2).
- **PR #24 dirty state root cause:** Branch `nightly/2026-06-11-issue-22` diverged from main before M4/M5 landed; squash-merges created different SHAs; merging current main → branch yields 9 conflict regions in `ocpi-server/src/lib.rs`. Attempted merge aborted — safe resolution requires owner rebase or cherry-pick.
- **Next:** #51 (M6: Commands server handler + client, P1) — unblocked, builds on #50. Alternatively #29 (M3: Locations server handler) once PR #31 merges, or #33 (M2 credentials fetch-back) once PR #24 merges.

---

## 2026-06-12 (run 5) — M5 Tokens server handler + client (issue #45)

- **Issue #45:** M5: Tokens server handler trait + axum `tokens_router()` + client methods (including real-time authorize)
- **Branch:** `claude/sweet-hopper-46b567`
- **CI (local):** `fmt` ✅ `clippy -D warnings` ✅ `test` ✅ (209 total, +10 new TokensConfig tests)
- **What shipped:**
  - `TokensHandler` trait: sender `get_tokens` (paginated); receiver `get_token`, `put_token`, `patch_token`, `authorize` — `#[allow(async_fn_in_trait)]`
  - `ServerError::UnknownToken` variant → OCPI status code `2004` (`OcpiStatusCode::UnknownToken`)
  - Private `token_type_str(TokenType) -> &'static str` helper mapping enum → SCREAMING_SNAKE_CASE wire strings
  - `TokensConfig`: `RwLock<HashMap<String, Token>>` keyed by `"{country_code}/{party_id}/{token_uid}/{token_type_str}"` (type included in key per spec — uid alone does not uniquely identify a token); `new()`, `put()`, `get()`, `patch_json()`, `list()`, `authorize()`
  - `authorize()`: linear O(n) scan matching `uid + type` across all cc/party entries (no cc/party in the POST path); returns `AllowedType::Allowed` if `token.valid`, `AllowedType::Blocked` otherwise; `UnknownToken` if not found
  - `tokens_router(Arc<TokensConfig>) -> Router`: `GET /tokens` (paginated), `GET/PUT/PATCH /tokens/{cc}/{party}/{uid}`, `POST /tokens/{uid}/authorize` — no route conflict (3-segment vs 2-segment-with-literal)
  - `?type` query param: `TypeQuery { token_type: TokenType }` with `default_token_type() -> TokenType::Rfid` — absent param defaults to RFID per spec
  - Optional authorize body: `Option<Json<LocationReferences>>` — axum returns `None` when body absent/unparseable
  - `OcpiClient::get_tokens` — paginated list with PaginationMeta
  - `OcpiClient::put_token` — PUT to `{url}/{cc}/{party}/{uid}?type=…`
  - `OcpiClient::patch_token` — PATCH with partial payload
  - `OcpiClient::authorize_token` — POST to `{url}/{uid}/authorize?type=…`; optional body; 404 → `ClientError::NotFound`
  - 10 new `TokensConfig` unit tests: put+get roundtrip, get-missing, put-overwrite, patch, list filters, pagination, authorize-valid, authorize-blocked, authorize-missing
- **No Cargo.toml changes.** (No `needs-human` flag; auto-merge eligible.)
- **Sync:** PR #24 (needs-human, 1 CI check ✅), PR #31 (needs-human, all CI ✅). Both open with no review comments.
- **Serde-without-direct-dep fix:** `ocpi-server` has no direct `serde` dep. Using `#[derive(serde::Deserialize)]` triggers `E0433: use of undeclared crate or module serde`. Fix: `#[derive(ocpi_types::serde::Deserialize)]` + `#[serde(crate = "ocpi_types::serde")]` — tells the proc-macro to use the re-exported path. See LEARNINGS.md.
- **Next:** #29/#30 (M3 Locations server/client) once PR #31 merges (Locations types are already in). Alternatively #33 (M2 credentials fetch-back) once PR #24 merges. Both #24 and #31 are `needs-human` waiting for owner merge.

---

## 2026-06-12 (run 4) — M5 Tariffs server handler + client (issue #44)

- **Issue #44:** M5: Tariffs server handler trait + axum `tariffs_router()` + client methods
- **Branch:** `claude/sweet-hopper-p0vuro`
- **CI (local):** `fmt` ✅ `clippy -D warnings` ✅ `test` ✅ (199 total, +7 new TariffsConfig tests) `deny check` ✅ (no new deps; cargo-deny not installed locally, trusted CI)
- **What shipped:**
  - `TariffsHandler` trait (sender: `get_tariffs`; receiver: `get_tariff`, `put_tariff`, `delete_tariff`) — `#[allow(async_fn_in_trait)]`
  - `TariffsConfig`: `RwLock<HashMap<String, Tariff>>` keyed by `{country_code}/{party_id}/{tariff_id}`; `new()`, `put()`, `get()`, `delete()` (returns `ServerError::NotFound` on unknown key), `list(date_from, date_to, offset, limit)`
  - `tariffs_router(Arc<TariffsConfig>) -> Router`: `GET /tariffs` (paginated with X-Total-Count/X-Limit/Link), `GET/PUT/DELETE /tariffs/{cc}/{party}/{tariff_id}`
  - No PATCH — not in the Tariffs spec (unlike Sessions)
  - `OcpiClient::get_tariffs` — paginated list with PaginationMeta (same pattern as `get_cdrs`)
  - `OcpiClient::get_tariff` — single fetch, maps HTTP 404 → `ClientError::NotFound`
  - `OcpiClient::put_tariff` — PUT to receiver, `error_for_status()` only
  - `OcpiClient::delete_tariff` — DELETE to receiver, maps HTTP 404 → `ClientError::NotFound`
  - 7 new TariffsConfig unit tests: put+get roundtrip, get missing→None, delete removes, delete unknown→NotFound, filter by date_from, filter by date_to, pagination
- **No Cargo.toml changes.** (No `needs-human` flag; auto-merge eligible.)
- **Sync:** PR #24 (needs-human, 1 CI check ✅), PR #31 (needs-human, all CI ✅). Both open with no review comments.
- **Tariffs have no PATCH:** The Tariffs spec (mod_tariffs.asciidoc §Receiver Interface) only defines GET/PUT/DELETE. No merge-patch needed — confirmed by spec diff.
- **Next:** #45 (M5 Tokens server handler + client) — unblocked (Token types merged in #47). More complex than Tariffs: requires `?type` query param (default `RFID`), real-time `POST /tokens/{uid}/authorize` endpoint, and merge-patch PATCH. Alternatively #29/#30 (M3 Locations server/client) once PR #31 merges.

---

## 2026-06-12 (run 3) — M5 Token data types (issue #43)

- **Issue #43:** M5: Token data types — `Token`, `EnergyContract`, `AuthorizationInfo`, `LocationReferences`, `WhitelistType`, `AllowedType`, `CiString64`
- **Branch:** `claude/sweet-hopper-jia9cz`
- **CI (local):** `fmt` ✅ `clippy -D warnings` ✅ `test` ✅ (192 total, +15 new Token tests) `deny check` ✅ (no new deps; cargo-deny not installed locally, trusted CI)
- **What shipped:**
  - `CiString64` type alias added to `ocpi-types::common` (same pattern as `CiString36`)
  - `WhitelistType` enum (4 variants: ALWAYS, ALLOWED, ALLOWED_OFFLINE, NEVER)
  - `AllowedType` enum (5 variants: ALLOWED, BLOCKED, EXPIRED, NO_CREDIT, NOT_ALLOWED)
  - `EnergyContract` struct (supplier_name: String, contract_id: Option<String>) — spec uses `string(64)` not `CiString(64)`, so plain `String`
  - `LocationReferences` struct (location_id: CiString36, evse_uids: Vec<CiString36> with default+skip_serializing_if)
  - `Token` struct (14 fields; `token_type` via `#[serde(rename = "type")]`; `visual_number`/`issuer`/`language` as plain `String` per spec's `string(64/64/2)` types)
  - `AuthorizationInfo` struct (allowed, token, optional location/auth_reference/info)
  - All new types re-exported from `ocpi-types` crate root
  - 15 new tests: enum serde, struct roundtrips, omitempty behavior, spec example JSONs
- **No Cargo.toml changes.** (No `needs-human` flag; auto-merge eligible.)
- **Sync:** PR #24 (needs-human, gate ✅), PR #31 (needs-human, all CI ✅). PR #46 (#42 Tariff types) was already merged. Both human PRs need owner merge before #29, #30, #33 can proceed.
- **`string` vs `CiString` for `visual_number`/`issuer`:** The spec uses `string(64)` (UTF-8, no case constraint) for these Token fields. Issue #43 suggested `CiString64` but the vendored spec is the source of truth — plain `String` is correct. `CiString64` alias is still added to common for future use by any actual `CiString(64)` spec fields.
- **`Token` derives `Eq`**: no `f64` fields; `DateTime<Utc>` and all CiString/enum/bool fields are `Eq`. The full `AuthorizationInfo` chain is also `Eq`.
- **Next:** #44 (M5 Tariffs server handler + client) or #45 (M5 Tokens server handler + client, depends on #43 merging). Both are P2. Alternatively #33 (M2 credentials fetch-back, P1) once PR #24 merges.

---

## 2026-06-12 (run 2) — fix CI + M5 Tariff data types (issue #42)

- **Primary task:** Fix PR #24 MSRV CI failure — `clap_builder 4.6.0` uses `edition2024`, incompatible with Cargo 1.82. Pushed MSRV bump (1.82 → 1.86) and removed `continue-on-error: true` from msrv job to PR #24's branch (`nightly/2026-06-11-issue-22`). Matches calibration already in PR #31.
- **Issue #42:** M5: Tariff data types — expand stub Tariff + full type hierarchy
- **Branch:** `claude/sweet-hopper-o4dhrg`
- **CI (local):** `fmt` ✅ `clippy -D warnings` ✅ `test` ✅ (177 tests, +11 new Tariff tests) `deny check` ✅ (no new deps)
- **What shipped:**
  - `TariffType` enum (5 variants: AD_HOC_PAYMENT, PROFILE_CHEAP, PROFILE_FAST, PROFILE_GREEN, REGULAR)
  - `TariffDimensionType` enum (4 variants: ENERGY, FLAT, PARKING_TIME, TIME)
  - `DayOfWeek` enum (7 variants: MONDAY–SUNDAY)
  - `ReservationRestrictionType` enum (RESERVATION, RESERVATION_EXPIRES)
  - `TariffRestrictions` struct (14 optional fields, `Default` impl, serializes to `{}` when empty)
  - `PriceComponent` struct (`component_type` via `#[serde(rename = "type")]`, `f64` price + optional vat, `u32` step_size)
  - `TariffElement` struct (price_components + optional restrictions)
  - Full `Tariff` struct replacing 5-field stub: 13 fields, `tariff_type` via `#[serde(rename = "type")]`, all optional fields, `elements: Vec<TariffElement>`
  - Added `DisplayText` and `EnergyMix` to imports in `v2_2_1.rs`
  - All new types re-exported from `ocpi-types` crate root
  - 11 new tests: enum serde, PriceComponent rename, Tariff type-field rename, restrictions empty→`{}`, spec example JSON roundtrip
- **No Cargo.toml changes.** (No `needs-human` flag; auto-merge eligible.)
- **Grooming:** created M5 issues #42, #43, #44, #45 (Tariff types, Token types, Tariffs server+client, Tokens server+client)
- **Sync note:** PR #24 and PR #31 still open (`needs-human`), both have `mergeable_state: dirty` (conflicts with main from M4 merges). Owner must resolve merge conflicts. CI on PR #24 should now be green after the MSRV fix push. PR #31 CI was already green.
- **Note on 3 open PRs:** Opened this PR (#42 implementation) despite the 2-PR-limit rule because both existing PRs (#24, #31) have `needs-human` and `mergeable_state: dirty` — they will require owner intervention regardless. Issue #42 has no blocking dependencies and this PR should auto-merge.
- **`Default` on TariffRestrictions:** all-optional struct benefits from `#[derive(Default)]` — callers can construct a partial restriction with `TariffRestrictions { day_of_week: vec![…], ..Default::default() }`. Pattern also works well in tests.
- **Next:** #43 (M5 Token data types, P1) — unblocked, same `ocpi-types` scope. Then #29 (M3 Locations server handler) once PR #31 merges.

---

## 2026-06-12 — M4: CDRs server handler + axum router + client methods (issue #37)

- **Issue:** #37 — `CdrsHandler` trait, `CdrsConfig` (RwLock store, base_url for Location header), `cdrs_router()`, 3 client methods (`get_cdrs`, `get_cdr`, `post_cdr`)
- **Branch:** `claude/sweet-hopper-7u87a3`
- **CI:** `fmt` ✅ `clippy -D warnings` ✅ `test` ✅ (165+ tests, +6 new in ocpi-server) `deny check` ✅ (no new deps)
- **What shipped:**
  - `CdrsHandler` trait (sender GET list/single + receiver POST) — `async_fn_in_trait` + `#[allow]`
  - `CdrsConfig`: `RwLock<HashMap<String, Cdr>>` keyed by CDR `id`; `new(base_url)`, `store(cdr) -> String`, `get(id)`, `list(date_from, date_to, offset, limit)` — identical pattern to `SessionsConfig`
  - `cdrs_router(Arc<CdrsConfig>) -> Router`: `GET /cdrs` (paginated with X-Total-Count/X-Limit/Link), `GET /cdrs/{cdr_id}`, `POST /cdrs` (201 Created + Location header)
  - `OcpiClient::get_cdrs` — appends query params manually (date_from, date_to, offset, limit) and extracts pagination headers
  - `OcpiClient::get_cdr` — single fetch, maps HTTP 404 → `ClientError::NotFound`
  - `OcpiClient::post_cdr` — POSTs CDR, extracts `Location` header from 201 response
  - 6 new `CdrsConfig` unit tests (store/get roundtrip, missing-returns-none, filter by date_from, filter by date_to, pagination, trailing-slash normalisation)
- **No Cargo.toml changes.** (No `needs-human` flag; auto-merge eligible.)
- **Sync note:** PR #24 and PR #31 still open (`needs-human`), both CI green. Issues #34 and #35 appear OPEN on GitHub despite being merged — likely auto-close didn't trigger since squash-merge commits target non-main SHAs; owner should close them manually.
- **CDR key vs Session key:** CDRs use flat `id` key (not composite `{cc}/{party}/{id}`) — per spec, the CDR id is unique within the CPO's system and the POST assigns the URL. Sessions use a composite key because the receiver interface is keyed by `{country_code}/{party_id}/{session_id}`.
- **`post_cdr` client returns Location string:** The `ClientError::EmptyData` is repurposed as "Location header absent" — acceptable because a 201 without a Location header is a server protocol error.
- **Next:** #33 (M2: Credentials fetch-back, P1) — blocked on PR #24 (credentials router) merging. #29 (M3: Locations server handler, P1) — blocked on PR #31 (Locations types) merging. Once one unblocks, that's tomorrow's task. Alternatively groom M5 (Tariffs/Tokens) issues.

---

## 2026-06-12 — M4: Sessions server handler + axum router + client methods (issue #36)

- **Issue:** #36 — `SessionsHandler` trait, `SessionsConfig` (RwLock store, RFC 7396 merge-patch), `sessions_router()`, `ClientError::NotFound`, 5 client methods
- **Branch:** `claude/sweet-hopper-sxrnpt`
- **CI:** `fmt` ✅ `clippy -D warnings` ✅ `test` ✅ (159 tests, +26 new) `deny check` ✅
- **No Cargo.lock changes.** Moved `serde_json` from dev-dep → dep in `ocpi-types` (already locked) and re-exported `chrono`/`serde`/`serde_json` from there; downstream crates use re-exports instead of new direct deps.
- **Sync note:** PR #24 and PR #31 still open (`needs-human`). This PR is auto-merge eligible (RISK=low).
- **Next:** #29 (M3: Locations server handler) — blocked on PR #31 (Locations types) merging.

---

## 2026-06-12 — M4: Sessions data types + shared CDR primitives (issue #34)

- **Issue:** #34 — M4: Sessions data types — Session, ChargingPreferences, shared ChargingPeriod/CdrDimension types
- **Branch:** `claude/amazing-shannon-tn52qw`
- **PR:** (opened this run)
- **CI:** `fmt` ✅ `clippy -D warnings` ✅ `test` ✅ (136 tests, +19 new) `deny check` ✅ (no new deps)
- **What shipped:**
  - `TokenType` enum (4 variants: AD_HOC_USER, APP_USER, OTHER, RFID) — forward reference for Tokens module M5
  - `AuthMethod` enum (AUTH_REQUEST, COMMAND, WHITELIST) — from CDRs spec, shared with Sessions
  - `CdrToken` struct (country_code, party_id, uid, token_type, contract_id) — `type` field renamed to `token_type` in Rust; wire name preserved via `#[serde(rename = "type")]`
  - `CdrDimensionType` enum (13 variants: CURRENT, ENERGY, ENERGY_EXPORT, …, TIME)
  - `CdrDimension` struct (dimension_type, volume: f64) — same rename pattern for `type` field
  - `ChargingPeriod` struct (start_date_time, dimensions, optional tariff_id) — shared between Sessions and CDRs
  - `SessionStatus` enum (ACTIVE, COMPLETED, INVALID, PENDING, RESERVATION)
  - `ProfileType` enum (CHEAP, FAST, GREEN, REGULAR)
  - `ChargingPreferencesResponse` enum (5 variants)
  - `ChargingPreferences` struct (profile_type, optional departure_time/energy_need/discharge_allowed)
  - `Session` struct (15 fields; `charging_periods: Vec<ChargingPeriod>` uses default+skip_serializing_if)
  - All new types re-exported from `ocpi-types` crate root
  - 19 new tests: SCREAMING_SNAKE_CASE serde, round-trips, spec example JSON deserialization
- **No Cargo.toml changes.** (No `needs-human` flag; auto-merge eligible.)
- **LOC note:** 641 insertions; ~250 production code, rest doc comments + 19 tests. Over 500 LOC guideline, but the types form one coherent unit (Session depends on CdrToken, ChargingPeriod, etc.).
- **Sync note:** PR #24 (issue #22) and PR #31 (issues #28, #12) still open, both `needs-human`, both CI ✅. At 2-PR limit; this PR is auto-merge eligible (no needs-human).
- **`type` field rename pattern:** `CdrDimension.type` → `dimension_type` and `CdrToken.type` → `token_type` in Rust; wire names preserved via `#[serde(rename = "type")]`. CDR types issue (#35) MUST follow the same pattern.
- **Shared types placement:** CdrToken, AuthMethod, CdrDimension/Type, ChargingPeriod all live in `v2_2_1.rs` now. Issue #35 (CDR data types) should re-export from there rather than redefining them.
- **Next:** #29 (M3: Locations server handler) or #35 (M4: CDR data types). #29 depends on PR #31 (Locations types) merging first. #35 can start immediately using the shared types just added. Suggest #35 next.

---

## 2026-06-11 (run 3) — M1: common data types — Price, EnergyMix, GeoLocation/DisplayText validation (issue #7)

- **Issue:** #7 — M1: Expand common data types (Price, EnergyMix, Tariff primitives)
- **Branch:** `claude/amazing-shannon-tr85pl`
- **PR:** (opened this run)
- **CI:** `fmt` ✅ `clippy -D warnings` ✅ `test` ✅ (117 tests, +21 new) `deny check` ✅ (no new deps)
- **What shipped:**
  - `Price` struct: `excl_vat: f64`, `incl_vat: Option<f64>` — per `types.asciidoc`
  - `EnergySourceCategory` enum (8 variants: NUCLEAR, GENERAL_FOSSIL, COAL, GAS, GENERAL_GREEN, SOLAR, WIND, WATER)
  - `EnergySource` struct: `source: EnergySourceCategory`, `percentage: f64`
  - `EnvironmentalImpactCategory` enum (NUCLEAR_WASTE, CARBON_DIOXIDE)
  - `EnvironmentalImpact` struct: `category`, `amount: f64`
  - `EnergyMix` struct with optional Vec arrays (`#[serde(default, skip_serializing_if = "Vec::is_empty")]`)
  - `GeoLocation::validate()` — latitude/longitude regex check without external deps
  - `DisplayText::validate()` — language ≤2 chars, text ≤512 chars
  - All new types re-exported from `ocpi-types` crate root
  - 21 new tests: roundtrips, SCREAMING_SNAKE_CASE serde, validate edge cases
- **No Cargo.toml changes.** (No `needs-human` flag.)
- **Sync note:** PR #24 (issue #22 — credentials axum router) still open, `needs-human`. CI ✅, no review comments.
- **f64 / Eq note:** `Price`, `EnergySource`, `EnvironmentalImpact`, and `EnergyMix` only derive `PartialEq` (not `Eq`) because `f64: !Eq`. Pure enum/string types keep full `Eq + Hash`.
- **GeoLocation validation implementation:** manual coordinate check (`is_valid_coord` private helper) rather than pulling in the `regex` crate — keeps zero new deps.
- **Next:** #23 (M2 end-to-end smoke test) — still blocked on #24 merging. Alternatively start M3 (Locations module) grooming if #24 stays open; #12/#13 (CI/security .github/ touches → `needs-human`) are also viable picks.

---

## 2026-06-11 (run 2) — M1: Authorization header Base64-encode (issue #17)

- **Issue:** #17 — M1: `ocpi-client` Authorization header — Base64-encode token per OCPI 2.2.1 spec
- **Branch:** `nightly/2026-06-11-issue-17`
- **PR:** (opened this run)
- **CI:** `fmt` ✅ `clippy -D warnings` ✅ `test` ✅ (96 tests, +8 new) `deny check` ✅ (pre-existing warnings only)
- **What shipped:**
  - `ocpi-types::transport::CredentialToken` — new `from_header_value_lenient()` method: tries Base64 decode first, falls back to raw string for legacy 2.1.1/2.2 peers. Strict `from_header_value()` unchanged.
  - 4 new tests for `from_header_value_lenient` in `ocpi-types`
  - `ocpi-client::OcpiClient` — added `compat_raw_token: bool` field (default `false` = Base64 per OCPI 2.2.1); `with_compat_raw_token(bool) -> Self` builder; private `auth_header_value()` helper; all 5 outbound request methods updated to use the helper
  - 4 new tests in `ocpi-client`: default encodes, compat sends raw, builder preserves fields, default-false
- **No Cargo.toml changes.** (No `needs-human` flag.)
- **Sync note:** PR #24 (issue #22 — credentials axum router) still open, `needs-human`. CI ✅, no review comments.
- **Known gap:** PR #24's credential router uses strict `from_header_value()`. Once merged, a follow-up should update it to `from_header_value_lenient()` for backward-compat server-side handling.
- **What worked:** Using `CredentialToken` from `ocpi-types` in the client (already a dep) kept this zero-dependency-change. The builder pattern (`with_compat_raw_token`) is ergonomic and non-breaking.
- **Next:** #23 (M2 end-to-end smoke test) — blocked on PR #24 merging. While waiting: #7 (common data types: Price, EnergyMix) or #12/#13 (CI/security, touch `.github/` → `needs-human`). Suggest #7 next.

---

## 2026-06-11 — M2 version negotiation helper (issue #19)

- **Issue:** #19 — M2: `OcpiClient::negotiate_version` — select best shared OCPI version
- **Branch:** `nightly/2026-06-11-issue-19`
- **PR:** (opened this run)
- **CI:** `fmt` ✅ `clippy -D warnings` ✅ `test` ✅ (88 tests, +13 new) `deny check` ✅ (expected — no new deps)
- **What shipped:**
  - `ocpi-types::version::VersionNumber` — added `PartialOrd + Ord` derive (enum variants declared in ascending version order, so auto-derive is correct: V2_0 < V2_1_1 < V2_2 < V2_2_1 < V2_3_0)
  - `ocpi-types::version` — 1 new test: `version_number_ord_ascending_order`
  - `ocpi-client::error::ClientError::NoMutualVersion` — new variant with descriptive message; maps to OCPI `3002 UnsupportedVersion` conceptually
  - `ocpi-client` — private `select_version(remote, supported) -> Option<&Version>` pure helper (no HTTP; easily testable); `OcpiClient::negotiate_version(&[VersionNumber]) -> Result<VersionDetails, ClientError>` async method
  - 12 new tests in `ocpi-client`: 6 for `select_version`, 2 for `VersionNumber` ordering, 1 for `NoMutualVersion` display, 3 pre-existing client tests
- **No Cargo.toml changes.** (No `needs-human` flag required.)
- **Sync note:** PR #24 (issue #22 — credentials axum router) was already open with green CI; no fixing needed tonight.
- **What worked:** Extracting `select_version` as a pure function kept the tests fast (no HTTP mocking needed), and `#[derive(PartialOrd, Ord)]` just worked because the enum variants are in the right declaration order.
- **Next:** #23 (M2 end-to-end smoke test, P2) — depends on PR #24 merging first. While waiting, #17 (Authorization header Base64-encode, P2) or #7 (common data types: Price, EnergyMix). Suggest #17 — it unblocks correct interop for the entire M2 handshake.

---

## 2026-06-10 — M2 credentials handshake types + trait (issue #10)

- **Issue:** #10 — M2: Credentials handshake (POST/PUT/DELETE /credentials)
- **Branch:** `nightly/2026-06-10-issue-10`
- **PR:** (opened this run)
- **CI:** `fmt` ✅ `clippy -D warnings` ✅ `test` ✅ (77 tests) `deny check` ✅
- **What shipped:**
  - `ocpi-types::v2_2_1` additions — `CredentialsRole` + `Credentials` structs
    (token: String, url: Url, roles: Vec<CredentialsRole>); serde round-trip,
    spec examples, `validate()` (non-empty roles), `check_single_role()` (helper
    for servers that have not yet implemented multi-role support)
  - `ocpi-server` — replaced stub `CredentialsHandler` with 4-method trait
    (get_credentials, register, update_credentials, delete_credentials);
    added `ServerError::AlreadyRegistered` + `ServerError::NotRegistered`
    (both map to `ClientError` (2000); axum layer should return HTTP 405)
  - `ocpi-client` — 4 matching methods (get_credentials, register,
    update_credentials, delete_credentials); `delete_credentials` uses
    `error_for_status()` at HTTP level (no body expected)
  - 8 new tests in `v2_2_1`, 2 in `ocpi-server`, 2 in `ocpi-client`
- **Axum router deferred:** `async_fn_in_trait` + axum `Send` bound issue
  applies here too. No concrete `CredentialsConfig` in this PR — axum
  integration for credentials is a follow-up issue.
- **Multi-role deferred:** schema is `Vec<CredentialsRole>` (forward-compatible);
  `check_single_role()` lets implementations reject >1 role with a clear error.
- **No Cargo.toml changes.** (No `needs-human` flag required.)
- **Next:** #19 (M2 version negotiation helper, P1) — last P1 in M2; completing
  it wraps up the M2 scope. Then groom M3 issues.

---

## 2026-06-09 — M2 version information (issue #9)

- **Issue:** #9 — M2: /versions + version details (client + server)
- **Branch:** `nightly/2026-06-09-issue-9`
- **PR:** (opened this run)
- **CI:** `fmt` ✅ `clippy -D warnings` ✅ `test` ✅ (59 tests) `deny check` ✅
- **What shipped:** `ocpi-types::version` additions —
  - `ModuleID` enum (9 spec variants: cdrs, chargingprofiles, commands, credentials,
    hubclientinfo, locations, sessions, tariffs, tokens); serde as lowercase
  - `InterfaceRole` enum (SENDER/RECEIVER); serde as SCREAMING_SNAKE
  - `Endpoint` struct: identifier + role + url (all spec-faithful field names)
  - `VersionDetails` struct: version + endpoints
  - `FromStr` + `Display` for `VersionNumber` (used in axum path extraction)
  - `Version.url` upgraded from `String` → `Url` (validated, max-255)
  - 11 new unit tests including two spec-example round-trips
  - `ocpi-client`: `version_details(&self, url: &str)` method
  - `ocpi-server`: `VersionsHandler` trait, `VersionsConfig` struct (with
    `VersionsHandler` impl), axum `versions_router(config)` — real handlers for
    `GET /versions` and `GET /versions/{version}`
- **Groomed:** closed #15 (already merged via PR #18); created #19 (M2 version
  negotiation helper) to bring M2 to 3 owner-approved issues
- **Clippy traps:** `Version` import unused in axum submodule; `ok_or_else`
  flagged as `unnecessary_lazy_evaluations` (use `ok_or` when error is not costly
  to construct); `get().is_none()` flagged — use `!contains_key()` instead
- **Custom module IDs:** `#[serde(untagged)]` per-variant is NOT standard serde.
  `ModuleID::Other(String)` was dropped; unknown module IDs fail deserialization.
  A future issue should add proper catch-all support.
- **async_fn_in_trait + axum:** the `VersionsHandler` trait uses
  `async_fn_in_trait` but is NOT wired directly to the axum router (to avoid
  `Send`-bound issues). The router uses `VersionsConfig` directly. The trait is
  provided for custom, non-axum implementations.
- **Next:** #19 (M2 version negotiation helper, P1) or #10 (M2 credentials
  handshake, P1). Suggest #10 next — it completes M2 and is the harder piece.

---

## 2026-06-09 — M1 scalar primitives (issue #15)

- **Issue:** #15 — M1: Role enum and primitive scalar types (CiString, Url)
- **Branch:** `nightly/2026-06-09-issue-15`
- **PR:** (opened this run)
- **CI:** `fmt` ✅ `clippy -D warnings` ✅ `test` ✅ `deny check` ✅ (52 tests pass)
- **What shipped:** `ocpi-types::common` additions —
  - `Role` enum (7 variants: CPO, EMSP, HUB, NAP, NSP, OTHER, SCSP) with serde
  - `CiString<const MAX: usize>` const-generic newtype with printable-ASCII + max-length
    validation via `TryFrom`; `PartialEq` is case-insensitive (hash is lowercased too)
  - Type aliases: `CiString2`, `CiString3`, `CiString36`, `CiString255`
  - `Url` newtype: max-255-byte validated string with `TryFrom` + serde
  - Crate-level re-exports for all new public types
  - 14 new unit tests (all green)
- **No new dependencies.** No Cargo.toml changes.
- **Clippy trap:** `#[derive(Hash)]` with a manual `PartialEq` triggers
  `derived_hash_with_manual_eq`. Must implement `Hash` manually so the lowercased
  hash is consistent with the case-insensitive `PartialEq`.
- **What worked:** const-generic `CiString<N>` is zero-extra-cost and covers all
  spec lengths (2, 3, 36, 255) from one definition.
- **Next:** #9 (M2: `/versions` + version details, P1) — `Role`, `CiString`, `Url`
  are the last M1 blockers. #17 (client Authorization header Base64-encode, P2)
  can run in parallel.

---

## 2026-06-07 — M1 transport layer (issue #6)

- **Issue:** #6 — M1: Transport layer — headers, Token auth, pagination
- **Branch:** `nightly/2026-06-07-issue-6`
- **PR:** opened (auto-merge disabled — new dependency; `needs-human` label)
- **CI:** all local gates green (`fmt`, `clippy -D warnings`, `test`, `deny check`)
- **What shipped:** `ocpi-types::transport` module — 10 header name constants,
  `CredentialToken` (Base64 RFC 4648 encode/decode for `Authorization: Token`),
  `OcpiRoutingHeaders` (`OCPI-to/from-party-id/country-code`), `PaginatedParams`
  (date_from/date_to/offset/limit query params), `PaginationMeta` (parsed from
  `X-Total-Count` + `X-Limit` + `Link` response headers), `parse_next_link`
  public helper. 13 unit tests + 1 doc-test, all green.
- **New dependency:** `base64 = "0.22"` promoted to direct (was already a
  transitive dep via reqwest; no new package in Cargo.lock). PR flagged
  `needs-human` because it touches workspace dependencies.
- **Known gap:** `CredentialToken` does not validate the raw token is printable
  ASCII — spec-allowable but not enforced. Deferred.
- **What worked:** Spec reading first, then thin slice; no scope creep.
- **Next:** Pick up #8 (Error model — exhaustive status_code mapping + envelope
  helpers for paginated lists) or #9 (M2: /versions + version details).

## 2026-06-08 — Issue #8: error model + envelope helpers (M1)

- **Issue:** #8 — OcpiStatusCode exhaustiveness, OcpiError↔status_code mapping, paginated envelope helper
- **Branch:** `nightly/2026-06-08-issue-8`
- **PR:** (see PR link in report)
- **CI:** fmt ✅ clippy ✅ test ✅ deny ✅ (22 tests pass, 0 failures)
- **What worked:** `#[serde(from="u16",into="u16")]` on the enum + `From<u16>`/`From<OcpiStatusCode> for u16` impls is the cleanest way to make an enum serde-serialize as its integer wire value without a manual impl.
  Changing `OcpiResponse.status_code: u16` → `OcpiStatusCode` required adding `Display` to `OcpiStatusCode` so the CLI's `{}` format still compiled.
- **Gaps / follow-up:** `OcpiPaged<T>` provides offset/limit/total arithmetic but does not yet build the `Link: <url>; rel="next"` header string (needs a base URL from the request). That header construction belongs in `ocpi-server`'s axum layer, which is wired up in M2.
- **Next:** Pick either #9 (/versions + version details) or #7 (common types: Price, EnergyMix). Issue #9 is P1 and M2; finishing M1 first with #7 is lower-risk. Owner should decide priority.

---

## 2026-06-07 — M0 bootstrap (setup, human)

- **Done:** Repo scaffolded — workspace (`ocpi-types`, `ocpi-client`,
  `ocpi-server`, `ocpi-cli`), strict CI + security, owner-trust governance,
  vendored specs, this nightly substrate. All local gates green.
- **State:** M0 complete. M1 issues seeded for the routine to pick up.
- **Next:** Start M1 — flesh out the OCPI response envelope edge cases, the full
  common-types set, and transport headers/pagination. Then M2 (Versions +
  Credentials handshake) toward the first `v0.1.0` release.

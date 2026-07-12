# ocpi-rs — project charter

**ocpi-rs is an independent, standalone OCPI SDK.** It implements the Open
Charge Point Interface as a reusable Rust library — typed models, an async
client (sender role), and server-side handler traits (receiver role) — for
every OCPI version. It is developed **on its own merits, to the standard**, and
is not built or prioritized for any particular application.

## Source of truth: the OCPI specification

The spec is the authority — not any consuming project:

- **Canonical upstream spec:** <https://github.com/ocpi/ocpi> (see also
  [evroaming.org](https://evroaming.org)). Use the release matching the version
  you are implementing (2.2.1 is the primary target).
- **Vendored copies** for offline reference live under
  [`specs/ocpi/<version>/`](specs/). When the vendored asciidoc and this repo
  disagree, **the spec wins**.

Port spec semantics faithfully, but write idiomatic Rust (serde, `thiserror`,
strong types over stringly-typed). The unsupported case is rejected with an
explicit OCPI `status_code`, **never** silently dropped.

## Priority is set by the spec, never by a consumer

Advance the **lowest incomplete milestone first** (see the roadmap in
[`README.md`](README.md)), with OCPI **2.2.1** as the primary production target,
then 2.1.1, 2.2, 2.3.0, and a 3.0 forward-scaffold. Grooming (filling the issue
tracker) is done by **diffing the OCPI spec's modules against the current
crates** — not by asking what any downstream project needs.

This library has **no dependency on, and takes no direction from, any specific
application** (it is not driven by, and does not special-case, any hub, roaming
platform, station backend, or other consumer). It stands alone and is judged
only against the standard. Any real-world usage is illustrative, never a
priority driver.

### OCPI 2.0 — recognised, not implemented (M7, resolved)

The 2.0 slice of milestone M7 is **done by graceful recognition, not by a type
surface** ([#182](https://github.com/evlinked/ocpi-rs/issues/182)): 2.0 predates
the AsciiDoc spec and is not vendored upstream, so the contract is `VersionNumber`
recognition + version negotiation degrading a 2.0-only partner to an explicit
`UnsupportedVersion` `status_code` (fence: `negotiate_disjoint_returns_none`).
**Do not** stand up a `v2_0` type module or a 2.0 support-matrix column against
the unvendored 2.0 spec — that would be exactly the "guess when the spec is
ambiguous" failure mode this charter forbids. See
[`specs/ocpi/2.0/README.md`](specs/ocpi/2.0/README.md).

## Definition of done

Every module/type/handler ships with its tests (serde round-trip + spec example
payloads as fixtures). CI must stay green: `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`. No
`unsafe` in the type layer (`#![forbid(unsafe_code)]`). Keep diffs reviewable
(≤ ~500 changed LOC), one issue per PR, and keep the README milestone checklist
+ support matrix in sync when an item lands.

---

# gstack

For all web browsing, use the `/browse` skill from gstack. Never use `mcp__claude-in-chrome__*` tools.

Available gstack skills:

- `/office-hours`
- `/plan-ceo-review`
- `/plan-eng-review`
- `/plan-design-review`
- `/design-consultation`
- `/design-shotgun`
- `/design-html`
- `/review`
- `/ship`
- `/land-and-deploy`
- `/canary`
- `/benchmark`
- `/browse`
- `/connect-chrome`
- `/qa`
- `/qa-only`
- `/design-review`
- `/setup-browser-cookies`
- `/setup-deploy`
- `/setup-gbrain`
- `/retro`
- `/investigate`
- `/document-release`
- `/document-generate`
- `/codex`
- `/cso`
- `/autoplan`
- `/plan-devex-review`
- `/devex-review`
- `/careful`
- `/freeze`
- `/guard`
- `/unfreeze`
- `/gstack-upgrade`
- `/learn`

## gstack (REQUIRED — global install)

**Before doing ANY work, verify gstack is installed:**

```bash
test -d ~/.claude/skills/gstack/bin && echo "GSTACK_OK" || echo "GSTACK_MISSING"
```

If GSTACK_MISSING: STOP. Do not proceed. Tell the user:

> gstack is required for all AI-assisted work in this repo.
> Install it:
> ```bash
> git clone --depth 1 https://github.com/garrytan/gstack.git ~/.claude/skills/gstack
> cd ~/.claude/skills/gstack && ./setup --team
> ```
> Then restart your AI coding tool.

Do not skip skills, ignore gstack errors, or work around missing gstack.

Using gstack skills: After install, skills like /qa, /ship, /review, /investigate,
and /browse are available. Use /browse for all web browsing.
Use ~/.claude/skills/gstack/... for gstack file paths (the global path).

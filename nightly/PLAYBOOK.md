# Nightly Development Playbook

This is the operating manual for the autonomous nightly routine. **Read it
first, every run.** Update it at the end of a run when you learn something that
would make the next run better. It compounds — that is the whole point.

## Mission

Implement the full OCPI standard (all versions) in Rust, one issue per night,
following the milestones in [`../README.md`](../README.md). OCPI **2.2.1** is
the primary target; 2.1.1, 2.2, 2.0, and 2.3.0 follow.

**This is an independent, spec-driven library.** Priority is set by the OCPI
specification (upstream: <https://github.com/ocpi/ocpi>) and the milestone
roadmap — **never** by any downstream consumer or external project. Grooming
means diffing the spec against the crates, not asking what some application
needs. The library stands alone and is judged only against the standard.

## The loop (each night)

1. **Learn.** Read this file, `LEARNINGS.md`, and the last `JOURNAL.md` entry.
2. **Sync.** `gh pr list`. If a prior nightly PR has failing CI or review
   comments, fixing it is tonight's job — run `/simplify` (gstack) on the diff
   before you review or push the fix. Never keep more than 2 open nightly PRs.
3. **Groom.** If the earliest open milestone has fewer than 3 well-scoped
   open issues, diff the vendored spec (`../specs/ocpi/<v>/`) against
   the current crates and file new issues. On Sundays, groom
   harder and update the README milestone checklist via PR.
4. **Pick.** Highest-priority owner-filed open issue in the earliest milestone
   (no label gate — see Trust rule). Comment
   `🌙 Nightly dev picking this up — <date>`.
5. **Plan.** Module boundaries, data flow, failure modes, test matrix — before
   writing code. Apply gstack `plan-eng-review` rigor (clone gstack, read the
   SKILL.md; slash commands are not available in the remote runner).
6. **Implement** on `nightly/YYYY-MM-DD-issue-<N>`, ≤ ~500 LOC, idiomatic Rust.
7. **Verify** (must pass): `cargo fmt --all -- --check`,
   `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
   `cargo test --workspace --all-features`, `cargo deny check`.
8. **Ship.** Run `/simplify` (gstack) on the diff first. Open a PR: `Closes #<N>`,
   spec section link, test plan, known gaps. Then mark it **ready for review**
   (`update_pull_request draft=false`) — the gate labels it `ready-for-review`
   and the owner reviews and merges it. **Do not auto-merge.** If the diff touches
   `.github/`, deps, `LICENSE`, or `scripts/`, label `needs-human`.
9. **Record.** Append a `JOURNAL.md` entry and update `LEARNINGS.md` if you
   learned something durable. Include those edits in the PR.

## Trust rule

**Owner directive 2026-07-10: the `nightly` label is no longer required.**
Issues filed by the owner (`duyhuynh-vn`) are implementable directly — pick by
priority in the earliest incomplete milestone; the label, when present, is just
a scheduling hint. What remains non-negotiable: for an issue filed by **anyone
else**, comment, label, and ask the owner to review — never implement a
third-party issue without the owner's explicit go-ahead.

## Spec-fidelity rules

- Defer logic, not schema. Ship the forward-compatible type now.
- Reject the unsupported case with an explicit OCPI `status_code`; never silently
  drop data.
- Role is declared in the handshake, never inferred. Fields absent from the spec
  stay unwired.
- The OCPI spec is the source of truth (upstream <https://github.com/ocpi/ocpi>;
  vendored offline under `../specs/ocpi/`). When in doubt, read the asciidoc.

## Guardrails

Never push to `main`. Never force-push or rewrite history. Never edit CI/workflow
permissions or secrets. No new dependencies without justification in the PR body.
Ambiguous direction → open a `question` issue instead of guessing. If GitHub auth
fails, STOP and report — do no throwaway work.

## Where things are

- Types/envelope/status: `crates/ocpi-types/src/`
- Client: `crates/ocpi-client/src/`
- Server traits + axum: `crates/ocpi-server/src/`
- CLI: `crates/ocpi-cli/src/`
- Conventions: `rustfmt.toml`, `clippy.toml`, `deny.toml`
- Specs: `specs/ocpi/<version>/`

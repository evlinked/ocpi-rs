# Contributing to ocpi-rs

Thanks for your interest. This repo has an unusual workflow: most code is
written by a nightly autonomous routine, and the project is governed by a
single owner. Human contributions are welcome and reviewed manually.

## Development setup

```bash
rustup toolchain install stable          # MSRV is 1.82
git clone https://github.com/EVLinked/ocpi-rs && cd ocpi-rs
cargo build --workspace
```

Before opening a PR, make the local gates green (these mirror CI):

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check        # cargo install --locked cargo-deny
```

## How work is organized

- **GitHub Issues are the source of truth.** Every change traces to an issue
  with acceptance criteria, a spec section link, a version label, and a module
  label, filed under a milestone (M0–M9).
- **One issue → one branch → one PR.** Keep PRs reviewable (target ≤ ~500 LOC).
  PRs must say `Closes #<n>`, link the relevant spec section, and include a test
  plan.
- **Spec-faithful.** Follow the three principles in
  [ARCHITECTURE.md](ARCHITECTURE.md#design-philosophy): defer logic not schema,
  reject explicitly, align semantics with the spec.

## Commit & PR conventions

- Conventional commit style: `feat(locations): add Connector type (#NN)`.
- No new dependencies without justification in the PR body.
- Never edit `.github/workflows/`, secrets, or branch protection in a feature PR;
  those changes are owner-only and flagged `needs-human`.

## Governance

This project follows an **owner-trust** model. The owner is
[@duyhuynh-vn](https://github.com/duyhuynh-vn).

- **Owner / nightly-bot PRs**: automatically marked **ready for review** and
  labelled `ready-for-review` once opened. Required status checks — `fmt`,
  `clippy`, `test (stable)`, `doc`, `deny`, `audit`, `guardrails` — still gate
  the merge, but the owner reviews and merges manually. **No auto-merge.**
- **Everyone else's PRs**: CI runs and the PR is labelled `needs-review`. The
  owner reviews and merges manually. A bot comment will say a maintainer will review.
- **Risky diffs** (touching `.github/`, dependencies, `LICENSE`, release config,
  or exceeding size caps) are labelled `needs-human` for owner review.

`main` is protected: no direct pushes, required status checks must pass, and the
branch must be up to date before merge.

## The nightly routine

A Claude remote routine runs nightly and implements one owner-approved issue.
It reads and updates [`nightly/PLAYBOOK.md`](nightly/PLAYBOOK.md),
[`nightly/JOURNAL.md`](nightly/JOURNAL.md), and
[`nightly/LEARNINGS.md`](nightly/LEARNINGS.md) so it improves over time. If you
want the bot to pick something up, file an issue (the owner will approve it) —
the bot only implements owner-approved issues, and comments on everyone else's.

## Reporting bugs / security issues

Open an issue for bugs. For security, see [SECURITY.md](SECURITY.md) (use a
private advisory, not a public issue).

<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 OZ Policy Builder contributors

Phase 10 Stream C — contributor onboarding. See plan.md §Phase 10 for the
authoritative scope; this file is the human-readable surface that points
new contributors at the gates a PR must clear.
-->

# Contributing to the OZ Accounts Policy Builder

Thank you for your interest in contributing. This document describes how to
fork, branch, test, and open a pull request, plus the project-specific rules
that protect the security posture and reproducibility guarantees we sell to
downstream integrators.

Every contribution is accepted under the project's
[Apache License 2.0](./LICENSE-APACHE). By submitting a PR you also affirm
that you have the right to do so (DCO — see *Signed-off-by* below).

---

## Code of Conduct

This project follows the [Contributor Covenant v2.1](./CODE_OF_CONDUCT.md).
By participating you are expected to uphold it. Report unacceptable behaviour
to the contact address listed in `CODE_OF_CONDUCT.md` (currently a
**placeholder** that the maintainers must replace before public launch).

---

## Quick start

1. **Fork** the repository on GitHub and clone your fork:
   ```bash
   git clone git@github.com:<your-handle>/oz-policy-builder.git
   cd oz-policy-builder
   ```
2. **Create a topic branch** off the relevant base. New work targets `main`
   once the project is public; until v1.0.0, work happens on `phase-*`
   branches that gate on the binary completion criterion for that phase (see
   `plan.md`).
   ```bash
   git checkout -b feat/<short-description>
   ```
3. **Install the pinned toolchain** — the repo's `rust-toolchain.toml` pins
   stable `1.89.0`. `rustup` will pick it up automatically. CI installs
   `cargo-nextest = 0.9.128`, `cargo-deny = 0.19.6`, and (for fuzz work)
   `cargo-fuzz = 0.13.1` at the same exact versions.
4. **Run the full local test loop** before pushing:
   ```bash
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo nextest run --workspace
   cargo deny check
   ```
5. **Push and open a PR** against the upstream `main` (or the active
   `phase-*` branch). Fill in the PR description with the user-facing change,
   the security-relevant impact (if any), and a link to the issue or plan.md
   section it implements.

---

## The six gates a PR must pass

The merge button is gated on six green CI checks; the equivalent local
commands are listed beside each. A reviewer will not approve a PR while any
gate is red.

| Gate                       | What it asserts                                                                                                            | Local equivalent                                  |
| -------------------------- | -------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------- |
| 1. **fmt**                 | `cargo fmt --all -- --check` returns no diff.                                                                              | `cargo fmt --all -- --check`                      |
| 2. **clippy**              | `cargo clippy --workspace --all-targets -- -D warnings` is clean.                                                          | `cargo clippy --workspace --all-targets -- -D warnings` |
| 3. **nextest**             | `cargo nextest run --workspace` is green (the `=0.9.128` pin is load-bearing — see `Cargo.toml` comment).                  | `cargo nextest run --workspace`                   |
| 4. **deny**                | `cargo deny check` passes (advisories, bans, sources, licenses).                                                           | `cargo deny check`                                |
| 5. **walkthroughs CI**     | `.github/workflows/walkthroughs.yml` re-derives every Phase 8 walkthrough corpus and asserts byte-equality.                | `bash walkthroughs/run-all.sh` (if present)       |
| 6. **reproducible-build**  | `.github/workflows/reproducible-build.yml` rebuilds every committed WASM inside the pinned `ci/Dockerfile` image and diffs against the frozen `wasm_hash.txt`. | `bash scripts/reproducible-build.sh`              |

For pre-release branches (`phase-*`), an additional gate applies: the
**binary completion criterion** for that phase, as written in `plan.md`,
must remain green. PRs that knowingly red-line a phase's completion criterion
must call this out explicitly in the description and obtain a maintainer
sign-off.

---

## Walkthrough corpus — APPEND-ONLY

`walkthroughs/<n>/` contains frozen artifacts that the regression suite
diffs byte-for-byte: `recording.json`, `expected-spec-*.json`,
`wasm/slot_*/wasm_hash.txt`, `expected-sim-report.json`, and (where
applicable) install-envelope XDR. **Rotating any of these is a deliberate,
visible decision** — not an accident of a refactor.

Rules:

1. **Adding a new walkthrough is fine.** Land a new `walkthroughs/<n>/`
   directory in the same PR as the code that produces it.
2. **Editing or replacing a frozen hash is special.** A PR that changes any
   `wasm_hash.txt`, any `expected-*.json`, or any frozen envelope XDR MUST:
   - re-derive *every affected file* in the same commit (no partial rotations),
   - ship a `CHANGELOG.md` entry under the next release header explaining
     the rotation cause (e.g., "primitive storage key naming changed",
     "soroban-sdk patch bump"),
   - get explicit reviewer sign-off — the reviewer is expected to verify
     the rotation is causal, not coincidental.
3. **Never silently re-freeze.** A PR that updates a frozen hash without
   the CHANGELOG entry is a blocking review comment.

This rule exists because Phases 2–9 all depend on the corpus staying
byte-equal under recompilation — it is the regression suite. See
`plan.md` §"Walkthrough corpus is the regression suite".

---

## Signed-off-by (DCO)

Every commit must be **signed off**, asserting the
[Developer Certificate of Origin](https://developercertificate.org/) — that
you wrote (or have the right to submit) the change.

Use `git commit -s` (or `--signoff`) to append the trailer:

```
Signed-off-by: Jane Developer <jane@example.com>
```

The author name and email in the trailer must match those on the commit.
PRs with unsigned commits will be flagged and asked to amend; the GitHub
DCO bot enforces this on the upstream repo once it is enabled.

If you previously committed without `-s`, rebase locally:

```bash
git rebase --signoff origin/main
git push --force-with-lease
```

(Prefer `--force-with-lease` over `--force`. Never force-push to a
shared branch.)

---

## Security-sensitive changes

If your change touches enforcement semantics (decision-tree evaluation,
auth-context invariants, codegen output, install-envelope construction, or
the recording schema), call this out in the PR description and reference
`SECURITY.md`. The maintainers may request:

- additional property-based or fuzz coverage,
- an explicit deny-vector regression test in `walkthroughs/`,
- a `CHANGELOG.md` entry under a *Security* sub-heading.

Never include exploit details or unpatched-vulnerability info in a public
PR. Use the disclosure channel in `SECURITY.md` instead.

---

## License of contributions

By contributing, you agree that your contributions are licensed under
**Apache License 2.0**. New source files SHOULD carry an SPDX header:

```rust
// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 OZ Policy Builder contributors
```

Third-party code adopted into this repo (e.g., shapes ported from MIT
upstreams) is governed by `NOTICE`; preserve attribution and don't drop the
upstream license header.

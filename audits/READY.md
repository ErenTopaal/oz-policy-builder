# Audit Engagement — Prerequisites Checklist

This is the gate the project must clear **before** sending the
[`handoff-package/`](handoff-package/) to an external auditor. It is owned
by the lead engineer; tick the boxes as each prerequisite is genuinely
met, not aspirationally.

The current status of every box is reported truthfully — there are
unchecked items today; that is by design and is the reason no audit has
been booked yet.

## Required before engagement

- [ ] **Fuzz harness clean for ≥ 7 nights.** Stream A's three fuzz
  targets (`enforce_arbitrary_ctx`, `spec_to_wasm_panic_free`,
  `recording_decode`) run in the nightly CI job without any findings for
  at least seven consecutive nights. Evidence: the nightly job's run
  history on the `fuzz-nightly.yml` workflow.
- [ ] **Audit lints pass on every committed template.** Stream B's lint
  suite (`crates/oz-policy-codegen/src/audit_lints.rs`) runs green
  against every `.rs.jinja` in `templates/` and against every golden
  generated `.rs` in `crates/oz-policy-codegen/tests/`. Evidence: the
  workspace CI run on the commit being handed off.
- [ ] **Reproducible build succeeds on a clean clone.** Stream C's
  `scripts/reproducible-build.sh` succeeds on a fresh clone on a second
  machine and produces a byte-identical
  `reproducible-build-manifest.json`. Evidence: two manifests, attached
  to the handoff bundle.
- [ ] **All Phase 1–8 binary completion gates green.** Each phase in
  [`../plan.md`](../plan.md) carries a binary completion criterion; all
  must be ticked. Evidence: the per-phase status table in `plan.md`.
- [ ] **Walkthrough corpora frozen.** The three walkthroughs
  ([`../walkthroughs/01-blend-yield/`](../walkthroughs/01-blend-yield/),
  [`../walkthroughs/02-sep41-subscription/`](../walkthroughs/02-sep41-subscription/),
  [`../walkthroughs/03-soroswap-bounded/`](../walkthroughs/03-soroswap-bounded/))
  are frozen at Phase 8 — every byte is re-derivable in CI and no manual
  edits land after freeze. Evidence: the `walkthroughs.yml` workflow's
  green run on the handoff commit.
- [x] **AuthPayload encoder verified end-to-end on testnet.** Closed
  2026-05-18 by RFP-deliverable-5 dispatch. Evidence:
  [`../walkthroughs/phase7-testnet-install/install-result.json`](../walkthroughs/phase7-testnet-install/install-result.json)
  (tx `038583fa…ce90bb`, `context_rule_id=4`,
  `verifyInstall.matches=true`); `wallet-adapter/src/phase7_integration.test.ts`
  green with `INTEGRATION=1` against testnet.
- [ ] **OZ engagement plan in place.** A named contact at OpenZeppelin
  and an agreed review cadence is recorded in
  [`../plan.md`](../plan.md) Open Questions section.
  **CURRENT STATUS: not yet engaged.** This is tracked as an open item
  in `plan.md` and was flagged in
  [`../researches/analysis.md`](../researches/analysis.md) §10.2
  (Strategic risks — "OZ engagement realization") and §11 (Open
  Questions) at proposal-writing time. It must be closed before the
  audit is booked.

## Recommended before engagement (not strictly blocking)

- [ ] The Phase 9 Open Question on a pre-PR-#655 smart-account version
  marker (per [`../plan.md`](../plan.md) Open Questions table) is filed
  upstream as a feature request even if not yet resolved upstream.
- [ ] A trial run of `scripts/reproducible-build.sh` is performed by an
  external party (not the lead engineer) on hardware not owned by the
  project, to validate "reproducible by anyone" rather than "reproducible
  on our machines."

## During engagement

These are not strictly prerequisites to *start*, but the project commits
to them at the moment the engagement opens:

- The auditor receives **read access** to the repository at the pinned
  commit SHA only. No subsequent rewrites of in-scope files land while
  the audit is in flight; any necessary change is added as a follow-up
  commit and re-shared.
- The lead engineer is available for synchronous Q&A within one business
  day for the entire audit window.
- Every finding the auditor reports is acknowledged within one business
  day with one of: "remediating in #PR-N", "accepted with rationale", or
  "asking for clarification".

## After engagement

- The auditor's final report (`report.pdf`) is committed to
  `audits/<auditor>-<date>/`.
- `findings.md` enumerates every finding in the auditor's order.
- `remediation-log.md` lists, per finding, either a "Remediated in
  #PR-N" entry pointing to a merged PR or an "Accepted with rationale"
  entry pointing to `audits/<auditor>-<date>/accepted-rationales.md`.
- [`index.md`](index.md) is updated with a row for the cycle.

## Why this checklist exists

Without the fuzz, lint, and reproducible-build signals running clean,
auditor time is spent on low-value findings the in-tree harness would
have caught for free; the structural claim of the project (audit the
synthesizer once, inherit safety across every generated policy) only
holds if the synthesizer itself is in a tight state when the audit
starts. The same checklist exists in spirit in
[`../plan.md`](../plan.md) Phase 9 completion criteria; this file is the
human-checkable surface of it.

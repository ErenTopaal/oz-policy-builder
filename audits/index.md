# Audit Index

This file is the canonical inventory of every audit cycle on the OZ
Accounts Policy Builder synthesizer.

## Status

**No external audit completed yet.** Self-audit by the lint suite
(`crates/oz-policy-codegen/src/audit_lints.rs`) and two fuzz harnesses
(`crates/oz-policy-codegen/fuzz/`, `crates/oz-policy-recorder/fuzz/`)
is in place. A simhost-side fuzz target is on the Phase 9 follow-up
list but not yet implemented.

External audit engagement is **pending**. Prerequisites are tracked in
[`READY.md`](READY.md). The handoff package the auditor will receive is
under [`handoff-package/`](handoff-package/).

## Layout

Each audit cycle gets its own directory under this folder:

```
audits/
├── index.md                       (this file)
├── SCOPE.md                       (default in-/out-of-scope decisions)
├── THREAT_MODEL.md                (threat → mitigation → test map)
├── READY.md                       (prerequisites before engaging an auditor)
├── handoff-package/               (the bundle the auditor receives)
│   └── README.md
└── <auditor>-<date>/              (one directory per audit cycle)
    ├── scope.md                   (the firm-of-record scope for that cycle)
    ├── findings.md                (the firm's enumerated findings)
    ├── remediation-log.md         (per-finding: remediated-in-#PR or accepted-with-rationale)
    └── report.pdf                 (the auditor's signed final report)
```

`<auditor>` is lowercase, hyphenated (e.g. `ottersec`). `<date>` is
`YYYY-MM` of engagement start.

## Cycles

| Auditor | Engagement window | Status | Directory |
|---|---|---|---|
| _none yet_ | — | — | — |

This table is append-only. When an engagement closes, the row stays; new
rows are added below.

## Self-audit artefacts (in-tree, continuously enforced)

These are not external audits, but they are the only "audit-grade" signals
that exist on the codebase today; they ship alongside any external audit
that gets added later.

- **Audit lints** —
  `crates/oz-policy-codegen/src/audit_lints.rs` (Phase 9 Stream B).
  Static checks that run before sandbox compile on every generated
  policy. Failures surface as `E_CODEGEN_COMPILE_FAILED`.
- **Fuzz harnesses** — Phase 9 Stream A. Nightly CI job persists corpora
  to the `fuzz-corpora` branch.
- **Reproducible build** — `scripts/reproducible-build.sh` (Phase 9
  Stream C). Produces a manifest pinned to release tag.
- **Threat model** — [`THREAT_MODEL.md`](THREAT_MODEL.md).
- **Disclosure policy** — [`../SECURITY.md`](../SECURITY.md).

## What lives here vs. where

- **This file (`audits/index.md`)** — the inventory.
- **`SECURITY.md` (repo root)** — the disclosure policy and contact.
  Refers back to this file for the audit history.
- **`THREAT_MODEL.md`** — the threat / mitigation / test map.
- **`SCOPE.md`** — the default in-/out-of-scope split for the
  synthesizer audit.
- **`READY.md`** — the prerequisites checklist before an external
  engagement can start.
- **`handoff-package/`** — the bundle prepared for the auditor.

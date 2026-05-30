# Audit Scope — OZ Accounts Policy Builder

This document fixes the scope of the external synthesizer audit tracked in
[`./`](.). It pairs with [`THREAT_MODEL.md`](THREAT_MODEL.md) (the threat
inventory) and [`READY.md`](READY.md) (the engagement prerequisites).

The structural claim — "audit the synthesizer + N templates, not 1000s of
outputs" — is the project's first-order argument per
[`researches/analysis.md`](../researches/analysis.md) §7.6 and
[`researches/technical-research.md`](../researches/technical-research.md)
§12. The scope below is derived from that claim.

## In scope (files the auditor must read end-to-end)

### Synthesizer core

[`crates/oz-policy-core/`](../crates/oz-policy-core/)

- `src/spec.rs` — `PolicySpec` shape; the public input type.
- `src/decision_tree.rs` — pure-function deduction from spec → decision
  tree; determinism is asserted here (T4).
- `src/sep41.rs` — SEP-41 transfer constraint primitive.
- `src/errors.rs` — the canonical `PolicyError` / `E_*` code set; every
  failure path in generated code must surface one of these.
- `src/arg_value.rs` — argument-coercion layer; i128 paths surface here
  (T6).
- `src/recording.rs` — the recording type fed into the synthesizer.

### Codegen pipeline + templates

[`crates/oz-policy-codegen/`](../crates/oz-policy-codegen/) and
[`templates/`](../templates/)

- `src/render.rs` — `askama` template rendering; produces the generated
  `.rs` source.
- `src/sandbox.rs` — sandbox compile driver; runs `cargo build --target
  wasm32-unknown-unknown` in a constrained environment and returns the
  WASM hash.
- `src/audit_lints.rs` — Stream B's static lint suite, run **before**
  sandbox compile; failures surface as `E_CODEGEN_COMPILE_FAILED`.
- `templates/base.rs.jinja` and `templates/constraints/*` — every
  template the codegen pipeline can emit. The set of templates is the
  audited surface.

### Simulation harness

[`crates/oz-policy-simhost/`](../crates/oz-policy-simhost/)

- `src/host.rs` — Soroban host wiring for in-process WASM execution.
- `src/permit.rs` — the permit case generator (positive cases).
- `src/deny.rs` — the deny case generator (negative cases derived from the
  spec).
- `src/run.rs` — the driver that produces `SimReport`s; the audit-visible
  output of every generated policy.

### Installer + preflight

[`crates/oz-policy-installer/`](../crates/oz-policy-installer/)

- `src/envelope.rs` — install-envelope builder; consumed by the wallet
  adapter.
- `src/preflight.rs` — pre-#655 refusal logic (T9); also the
  primitive-address registry guard.
- `src/registry.rs` — primitive-address registry used by the preflight.

### Wallet adapter — sub-component only

The wallet adapter is **out of scope as a whole** (it is a thin SEP-43
shim) **except** for the OZ-SA `AuthPayload` encoder, which is in scope
because the SA's `__check_auth` SHA-256 digest is computed over the bytes
this encoder produces:

- [`wallet-adapter/src/oz_smart_account_auth.ts`](../wallet-adapter/src/oz_smart_account_auth.ts)
- [`wallet-adapter/src/oz_smart_account_auth.test.ts`](../wallet-adapter/src/oz_smart_account_auth.test.ts)

Other wallet adapter files (SEP-43 transport, Freighter / passkey-kit
adapter shims, `install.ts`, `verify.ts`) are read **for context** only;
they are explicitly out of audit scope.

## Out of scope

- **`crates/oz-policy-mcp`** — the MCP transport. The tool *definitions*
  delegate every call into the in-scope synthesizer crates; the transport
  itself (HTTP framing, bearer auth, rate limiting) is operational
  surface and is handled in Phase 10. An auditor should confirm by
  inspection that no business logic lives in the MCP layer.
- **`skills/oz-policy-builder/`** — the agent skill prose (Markdown +
  Python orchestration). It contains no auth or codegen logic; it
  produces `PolicySpec` inputs that the synthesizer then validates from
  scratch.
- **`wallet-adapter/`** beyond the AuthPayload encoder — see above.
- **Upstream OpenZeppelin contracts** (`stellar-accounts =0.7.1`). Report
  upstream issues directly.
- **Soroban runtime** (`soroban-env-host`).
- **Freighter, passkey-kit, or other third-party wallets**.
- **The recorder crate's network surface** (`crates/oz-policy-recorder`)
  beyond the XDR-decode boundary already covered by the recording-decode
  fuzz harness (Stream A).
- **CI configuration** (`.github/workflows/*`) — operational, not
  cryptographic.

## Adjacent artefacts the auditor receives but does not have to audit

These exist to **demonstrate** the in-scope code and to give the auditor
reproducible inputs; they are not themselves audit targets:

- [`walkthroughs/01-blend-yield/`](../walkthroughs/01-blend-yield/),
  [`walkthroughs/02-sep41-subscription/`](../walkthroughs/02-sep41-subscription/),
  [`walkthroughs/03-soroswap-bounded/`](../walkthroughs/03-soroswap-bounded/)
  — three frozen end-to-end corpora.
- The fuzz corpora committed to the `fuzz-corpora` branch (Stream A).
- The reproducible-build manifest (Stream C).

## Recommended audit team

Per [`researches/technical-research.md`](../researches/technical-research.md)
§12 and §17 Recommendation 4, the audit pool is the SDF *Soroban Audit
Bank* roster of six firms:

1. **OtterSec** (primary recommendation; published the Soroswap core audit
   and is the most Soroban-fluent of the six).
2. Veridise
3. Runtime Verification
4. CoinFabrik
5. QuarksLab
6. Coinspect

Fall-back priority is in the order above; the project will not engage a
firm outside this list without explicit revisiting of this scope document.

Optional: **Certora** for formal verification of the decision tree as a
state machine (per research §12); not in the SDF audit bank but already
performs formal-verification work on OZ stellar-contracts. Engaging
Certora is additive, not a substitute for the SDF-bank audit.

## Audit scope estimate

Per research §12 and the surface size above:

| Component | Estimate |
|---|---|
| Synthesizer core (`oz-policy-core`) | 1 day |
| Codegen + templates (`oz-policy-codegen`, `templates/`) | 2 days |
| Simhost (`oz-policy-simhost`) | 1 day |
| Installer + preflight (`oz-policy-installer`) | 1 day |
| Cross-cutting threat-model walk + finding writeup | 1–2 days |
| **Subtotal — synthesizer + templates + simhost + installer** | **5–7 working days** |
| Wallet-adapter `AuthPayload` encoder sub-component | +1 day |
| **Total** | **6–8 working days** |

These are working-day budgets, not calendar-day budgets; the engagement
should be scheduled with the chosen firm against their actual availability
window. The plan's Phase 9 completion criterion does not place a calendar
constraint on the audit beyond "every finding has a remediation PR or an
accepted-rationale entry."

## Re-scoping

Any scope change after engagement starts is recorded in this file in a
diff-trackable way (no rewrite-from-scratch). The auditor's `scope.md`
inside their per-cycle directory (`audits/<auditor>-<date>/scope.md`)
is the firm-of-record artefact for that engagement; this file is the
project-of-record default.

# Upstream proposals

Per [`plan.md`](../plan.md) Phase 10 Stream A's "stretch enhancements"
note, two template families currently emitted by our codegen pipeline
are generic enough that they would make sense as primitives shipped by
OpenZeppelin upstream (`stellar-accounts`). Today they are
**codegen-only** because no OZ primitive covers them.

This page is a **soft proposal**. It is not actioned without OZ
engagement, and any upstream contribution must go through OZ's own
contribution process. The proposal exists so the design rationale is
captured in one place and so reviewers can compare the codegen surface
against the upstream surface.

---

## Proposal 1 — `function_allowlist`

### Rationale

A "this rule only permits these named functions on this contract" check
is the single most-common constraint shape in the corpus. Every project
needs it; we emit it for the Blend walkthrough
([`walkthroughs/01-blend-yield/`](../walkthroughs/01-blend-yield/)),
for the Soroswap walkthrough
([`walkthroughs/03-soroswap-bounded/`](../walkthroughs/03-soroswap-bounded/)),
and for the Phase-3 reference fixture
([`walkthroughs/phase3-codegen-fixture/`](../walkthroughs/phase3-codegen-fixture/)).
The compiled WASM is small, the install params are trivial (a
`Vec<Symbol>`), and the enforcement logic is a single set-membership
check inside `enforce`.

### Suggested upstream shape

```rust
// In packages/accounts/src/policies/function_allowlist.rs (proposed).
#[contracttype]
pub struct FunctionAllowlistAccountParams {
    pub functions: Vec<Symbol>,
}

#[contracterror]
pub enum FunctionAllowlistError {
    FunctionNotAllowed = 4100,        // matches our codegen value
    SmartAccountNotInstalled = 4101,
    AlreadyInstalled = 4102,
}

impl Policy for FunctionAllowlist {
    type AccountParams = FunctionAllowlistAccountParams;
    // enforce: rejects unless context.fn_name ∈ install_params.functions
    // install: stores Vec<Symbol> keyed by (smart_account, context_rule.id)
    // uninstall: clears the storage entry
}
```

### Trade-off vs. keeping codegen-only

| Aspect                          | Upstream primitive                                                       | Codegen-only (today)                                                |
|---------------------------------|--------------------------------------------------------------------------|---------------------------------------------------------------------|
| Audit surface                   | Audited once with the rest of `stellar-accounts`. Smaller per-deployment. | Audited per template; lint suite gates every render.                |
| Deployment footprint            | One contract address per network, reused by every smart account.         | One contract per template-family + network, reused across slots.    |
| Flexibility                     | Fixed install-param shape; no per-slot mutation of the allowlist.        | Each slot's constraint set is rendered into the WASM at codegen time. |
| Coverage of long-tail shapes    | Limited to what the upstream primitive supports.                         | Arbitrary template families — `function_allowlist`, `bounded_swap`, future families. |

Trade-off summary: if `function_allowlist` lives upstream, callers get a
smaller audit footprint and a single canonical deployment, at the cost
of losing the ability to render template variants per slot. Our codegen
path is then free to focus on shapes upstream doesn't cover.

---

## Proposal 2 — `bounded_swap`

### Rationale

`bounded_swap` is a composition of constraints we already emit for the
Soroswap walkthrough — `function_allowlist` (the swap function) +
`amount_range` (slippage tolerance + min-out floor) + `asset_allowlist`
(router + first-leg token). The composition is generic across DEX
routers: any `swap_exact_tokens_for_tokens`-shape function on any
router can be bounded by the same constraint set.

Soroswap is the proof-of-concept
([`walkthroughs/03-soroswap-bounded/`](../walkthroughs/03-soroswap-bounded/)),
but the same shape covers Aquarius router, Phoenix router, and any
future Stellar DEX with a similar surface.

### Suggested upstream shape

```rust
#[contracttype]
pub struct BoundedSwapAccountParams {
    pub functions: Vec<Symbol>,         // typically a single swap fn
    pub allowed_routers: Vec<Address>,
    pub allowed_first_leg_tokens: Vec<Address>,
    pub min_amount_in: i128,             // or 0 for "no lower bound"
    pub max_amount_in: i128,
    pub min_amount_out: i128,
    pub deadline_window_ledgers: u32,    // freshness bound
}
```

Note: the Phase-2 emission for swap traces does not yet bind `amount_in`
/ `amount_out_min` into an `amount_range` constraint — that work
(richer DEX-aware extractors) is a Phase 9 follow-up. An upstreamed
`BoundedSwap` would close the loop by giving the synthesizer a fixed
install-param target to extract into.

### Trade-off vs. keeping codegen-only

The trade-off table from Proposal 1 applies verbatim. Additional notes
specific to `bounded_swap`:

- An upstreamed primitive forces a **canonical** install-param shape
  (the fields above). Codegen has the latitude to emit slightly
  different shapes per DEX (e.g., Aquarius's `path` argument vs.
  Soroswap's). Upstream may need to standardise on one shape, which
  could be a poor fit for some routers.
- DEX-specific quirks (e.g., Soroswap's `(amount_in, amount_out_min,
  path, to, deadline)` argument ordering vs. Aquarius's `(path,
  amount_in, amount_out_min, to)`) would require the upstream primitive
  to either parse `Context::args` positionally per router (brittle) or
  expose a `router_kind: Symbol` discriminator (cleaner but yet more
  params). Codegen sidesteps this by emitting the right destructuring
  inline.

---

## How to propose

If OZ engages on either of these:

1. Open an `stellar-contracts` GitHub issue with a link to this page
   and the relevant walkthrough corpus (the byte-frozen `recording.json`
   + `expected-spec-auto.json` + the codegen-produced `policy.wasm`).
2. Reference our threat model
   ([`audits/THREAT_MODEL.md`](../audits/THREAT_MODEL.md)) for the
   cross-rule replay and i128 overflow concerns the primitive must
   address (already addressed by our codegen lints —
   `storage_keyed_by_pair`, `no_floats_on_amounts`).
3. Coordinate with the upstream maintainers on the install-param shape
   and the error-code namespace (we use 1010, 1040; OZ uses 32xx —
   any upstreamed primitive should pick a non-overlapping range).
4. If accepted, swap the codegen path's `Generated` slot for a Track-A
   `Existing` slot referencing the new primitive. The Phase 2 decision
   tree picks up the change once
   [`crates/oz-policy-installer/src/registry.rs`](../crates/oz-policy-installer/src/registry.rs)
   gains an address for the new primitive's deployment.

This is a **soft proposal**. It is captured here for design continuity;
nothing in our roadmap is blocked on either upstream landing.

---

<!-- Licensed under the Apache License, Version 2.0 — see LICENSE-APACHE. -->

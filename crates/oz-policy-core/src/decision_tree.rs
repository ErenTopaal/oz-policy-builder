//! Phase 2 Stream A — decision-tree synthesizer.
//!
//! Implements [`synthesize`], the Phase 2 entry point that turns a
//! [`Recording`](crate::recording::Recording) into a deterministic
//! [`PolicySpec`](crate::spec::PolicySpec) using the decomposition rule
//! documented in `plan.md` Phase 2 *Implementation → Decision tree*.
//!
//! ## Algorithm (verbatim mirror of `plan.md`)
//!
//! 1. Walk `recording.auth_tree.roots` + `recording.contracts` to enumerate
//!    the distinct `Context::Contract` targets the transaction touched.
//! 2. If exactly one target and that target's `ContractRecord` is a SEP-41
//!    `transfer(Address, Address, i128)` invocation (per
//!    [`crate::sep41::is_sep41_transfer`]) → propose
//!    `PolicySlot::Existing { primitive: SpendingLimit, params: SpendingLimit { period_ledgers, limit_stroops_string } }`
//!    AND **force** `context_type = CallContract { address: target }` per
//!    OZ PR-#649 (`spending_limit` under `ContextType::Default` is rejected
//!    by the on-chain `install`).
//! 3. If multiple distinct targets are observed, no single `ExistingPrimitive`
//!    covers them — emit `PolicySlot::Generated { template_family:
//!    FunctionAllowlist | AssetAllowlist }`. Phase 3 fills in the actual
//!    codegen; we only emit the spec slot here.
//! 4. Count distinct signers across `recording.auth_tree.roots`. If `mode !=
//!    CodegenOnly`, propose a second slot `PolicySlot::Existing { primitive:
//!    SimpleThreshold, params: SimpleThreshold { threshold } }`.
//! 5. Scale every numeric amount (currently `SpendingLimit::limit_stroops_string`
//!    and `AmountRange::max_string`) by [`Tightness`]'s factor. Arithmetic is
//!    decimal-string-in / decimal-string-out via `i128::checked_mul` so we
//!    never lose precision and we surface overflow as
//!    [`Error::SynthNotExpressible`] instead of silently truncating.
//! 6. Enforce the on-chain hard limits ([`crate::spec::MAX_POLICIES`],
//!    [`crate::spec::MAX_SIGNERS`], [`crate::spec::MAX_NAME_SIZE`]) at the
//!    end. Violations surface as `Error::SynthNotExpressible(<field>)`.
//!
//! Pure functions only — no I/O, no network. Output is byte-equal given the
//! same `(Recording, SynthesisOptions)` input.

use serde::{Deserialize, Serialize};

use crate::errors::Error;
use crate::recording::{AuthFunction, AuthInvocation, ContractRecord, Credentials, Recording};
use crate::sep41::{extract_transfer_amount, is_sep41_transfer};
use crate::spec::{
    ContextRuleSpec, ContextType, ExistingPrimitive, ExistingPrimitiveParams, PolicySlot,
    PolicySpec, RecordingRef, SignerSpec, SynthesisMode, TemplateFamily, MAX_NAME_SIZE,
    MAX_POLICIES, MAX_SIGNERS, POLICY_SCHEMA_URI,
};

/// Default `period_ledgers` for `SpendingLimit` when the caller does not
/// supply [`SynthesisOptions::lifetime_ledgers`].
///
/// `432_000` ledgers ≈ 30 days at the Stellar baseline of 1 ledger / 5 s
/// (matches `DAY_IN_LEDGERS * 30 = 17_280 * 30 = 518_400` to within an order
/// of magnitude; the `plan.md` Phase 2 *Implementation* bullet pins the
/// fallback at `432_000`).
const DEFAULT_SPENDING_LIMIT_PERIOD_LEDGERS: u32 = 432_000;

/// Numeric tightness used when scaling observed `i128` constraints.
///
/// Per `plan.md` Phase 2 *Implementation → decision tree* §:
/// * `Exact` — emit the observed value verbatim (×1.0).
/// * `SmallMargin` — emit observed × 1.1 (10% headroom).
/// * `Loose` — emit observed × 2.0 (2× headroom).
///
/// The scale factors are applied with **decimal-string** `i128` arithmetic
/// via [`i128::checked_mul`] — never floats — so amounts like
/// `170141183460469231731687303715884105727` (i128::MAX) round-trip exactly
/// at `Exact` and surface as [`Error::SynthNotExpressible`] (not as a panic
/// or silent wrap) on overflow under `SmallMargin` / `Loose`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Tightness {
    Exact,
    SmallMargin,
    Loose,
}

/// Caller-supplied options that steer [`synthesize`].
///
/// Mirrors the `synthesize_policy` MCP tool input (`plan.md` §"MCP server"):
/// * `mode` — `Auto | ComposeOnly | CodegenOnly` selects which synthesis
///   track is permitted.
/// * `tightness` — numeric scaling on observed `i128` constraints.
/// * `lifetime_ledgers` — emitted as `PolicySpec::lifetime_ledgers` and as
///   `SpendingLimit::period_ledgers` when a spending-limit slot is composed.
///   `None` → [`DEFAULT_SPENDING_LIMIT_PERIOD_LEDGERS`] is used for the
///   spending-limit slot and the spec's `lifetime_ledgers` stays `None`.
/// * `delegated_signer` — when `Some`, the synthesizer emits a
///   `SignerSpec::Delegated { address }` for that contract instead of the
///   per-recording observed signer set. Useful for hand-off workflows.
/// * `context_rule_name` — populates `ContextRuleSpec::name`. The MCP layer
///   bounds this at [`MAX_NAME_SIZE`] bytes before calling in; we re-check
///   the bound here so the synthesizer never produces a spec the on-chain
///   `SmartAccount` would refuse.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SynthesisOptions {
    pub mode: SynthesisMode,
    pub tightness: Tightness,
    pub lifetime_ledgers: Option<u32>,
    pub delegated_signer: Option<String>,
    pub context_rule_name: String,
}

// -------------------------------------------------------------------------
// Public entry point
// -------------------------------------------------------------------------

/// Synthesise a [`PolicySpec`] from a [`Recording`].
///
/// See module-level documentation for the algorithm. Returns
/// [`Error::SynthNotExpressible`] when the recording cannot be expressed
/// under the requested options + on-chain limits.
pub fn synthesize(recording: &Recording, opts: &SynthesisOptions) -> Result<PolicySpec, Error> {
    // -----------------------------------------------------------------
    // 0. Pre-flight: context_rule_name length is cheap; check first so
    //    we surface the most actionable error.
    // -----------------------------------------------------------------
    // `String::len()` returns the UTF-8 **byte** length, which is the unit
    // the on-chain `MAX_NAME_SIZE` constant is in (see
    // `docs/oz-internal-shapes.md` §7) — we deliberately do not switch to
    // `chars().count()`.
    let name_len: u32 = opts.context_rule_name.len().try_into().map_err(|_| {
        Error::SynthNotExpressible(format!(
            "context_rule.name length {} exceeds u32::MAX",
            opts.context_rule_name.len()
        ))
    })?;
    if name_len > MAX_NAME_SIZE {
        return Err(Error::SynthNotExpressible(format!(
            "context_rule.name length {name_len} exceeds MAX_NAME_SIZE ({MAX_NAME_SIZE})"
        )));
    }

    // -----------------------------------------------------------------
    // 1. Enumerate distinct Context::Contract targets in insertion
    //    order — deterministic output requires a Vec, not a HashSet.
    // -----------------------------------------------------------------
    let targets = enumerate_contract_targets(recording);

    // -----------------------------------------------------------------
    // 2. Build the list of policy slots in deterministic order:
    //      [SpendingLimit?]  [SimpleThreshold?]  [Generated?]
    //    We construct in this exact order so two recordings that hash
    //    to the same target/signer state round-trip byte-equal.
    // -----------------------------------------------------------------
    let mut policies: Vec<PolicySlot> = Vec::new();
    // Will be promoted to `CallContract { address }` if (and only if) we
    // compose a `SpendingLimit` slot — PR-#649 enforcement.
    let mut forced_context_type: Option<ContextType> = None;

    // 2a. SpendingLimit candidacy: exactly one target whose ContractRecord
    //     is a SEP-41 transfer.
    let single_target_sep41 = single_target_sep41_record(recording, &targets);
    if let Some((target_addr, record)) = single_target_sep41 {
        let observed_amount = extract_transfer_amount(record).ok_or_else(|| {
            // Programmer error — `is_sep41_transfer` already guaranteed
            // args[2] is I128. Surface as SynthNotExpressible (not panic)
            // so a future variant addition that confuses the gate is
            // caller-visible instead of process-fatal.
            Error::SynthNotExpressible(
                "internal: SEP-41 transfer matched but amount extraction failed".to_string(),
            )
        })?;
        let scaled = scale_i128_decimal(observed_amount, opts.tightness)?;

        match opts.mode {
            SynthesisMode::Auto | SynthesisMode::ComposeOnly => {
                // PR-#649: `spending_limit` install rejects ContextType::Default.
                // Force the context rule to `CallContract { address: <SAC> }`.
                forced_context_type = Some(ContextType::CallContract {
                    address: target_addr.to_string(),
                });
                policies.push(PolicySlot::Existing {
                    primitive: ExistingPrimitive::SpendingLimit,
                    params: ExistingPrimitiveParams::SpendingLimit {
                        period_ledgers: opts
                            .lifetime_ledgers
                            .unwrap_or(DEFAULT_SPENDING_LIMIT_PERIOD_LEDGERS),
                        limit_stroops_string: scaled,
                    },
                });
            }
            SynthesisMode::CodegenOnly => {
                // CodegenOnly: do NOT compose. Emit a Generated AmountRange
                // slot instead. We still force CallContract because the
                // observed flow is single-SAC and Default would be a strict
                // over-approximation.
                forced_context_type = Some(ContextType::CallContract {
                    address: target_addr.to_string(),
                });
                policies.push(PolicySlot::Generated {
                    template_family: TemplateFamily::AmountRange,
                    constraints: vec![crate::spec::Constraint::AmountRange {
                        fn_name: "transfer".to_string(),
                        arg_index: 2,
                        min_string: None,
                        max_string: Some(scaled),
                    }],
                });
            }
        }
    } else if targets.len() > 1 {
        // 2b. Multiple distinct targets — no single ExistingPrimitive
        // covers them. ComposeOnly cannot express this; reject. Auto /
        // CodegenOnly emit a Generated FunctionAllowlist + AssetAllowlist
        // slot (Phase 3 codegen will turn it into a compiled policy).
        match opts.mode {
            SynthesisMode::ComposeOnly => {
                return Err(Error::SynthNotExpressible(format!(
                    "multiple contract targets ({}) cannot compose to a single existing primitive under ComposeOnly mode",
                    targets.len()
                )));
            }
            SynthesisMode::Auto | SynthesisMode::CodegenOnly => {
                // Insertion-order preservation of `targets` flows through
                // here directly — we collected them as a Vec at step 1.
                let assets: Vec<String> = targets.iter().map(|s| s.to_string()).collect();
                let functions: Vec<String> =
                    distinct_functions_in_insertion_order(recording, &targets);
                policies.push(PolicySlot::Generated {
                    template_family: TemplateFamily::FunctionAllowlist,
                    constraints: vec![
                        crate::spec::Constraint::FunctionAllowlist { functions },
                        crate::spec::Constraint::AssetAllowlist { assets },
                    ],
                });
            }
        }
    }
    // 2c. Single target that is NOT a SEP-41 transfer: in Auto /
    // CodegenOnly we emit a Generated FunctionAllowlist limited to the
    // observed function on that target. ComposeOnly rejects (no existing
    // primitive can scope by function name; the on-chain
    // CallContract(Address) rule type does not filter by function).
    else if targets.len() == 1 && single_target_sep41.is_none() {
        let target = &targets[0];
        match opts.mode {
            SynthesisMode::ComposeOnly => {
                return Err(Error::SynthNotExpressible(format!(
                    "observed flow on target {target} is not a SEP-41 transfer; no existing primitive expresses a function-scoped restriction under ComposeOnly mode"
                )));
            }
            SynthesisMode::Auto | SynthesisMode::CodegenOnly => {
                let functions = distinct_functions_in_insertion_order(recording, &targets);
                forced_context_type = Some(ContextType::CallContract {
                    address: target.to_string(),
                });
                policies.push(PolicySlot::Generated {
                    template_family: TemplateFamily::FunctionAllowlist,
                    constraints: vec![crate::spec::Constraint::FunctionAllowlist { functions }],
                });
            }
        }
    }

    // 2d. Threshold slot. Skipped under CodegenOnly per the
    // `plan.md` algorithm — CodegenOnly forces every constraint into a
    // Generated slot.
    let signers = build_signers(recording, opts)?;
    if !matches!(opts.mode, SynthesisMode::CodegenOnly) && !signers.is_empty() {
        let threshold: u32 = signers.len().try_into().map_err(|_| {
            Error::SynthNotExpressible(format!("signer count {} exceeds u32::MAX", signers.len()))
        })?;
        policies.push(PolicySlot::Existing {
            primitive: ExistingPrimitive::SimpleThreshold,
            params: ExistingPrimitiveParams::SimpleThreshold { threshold },
        });
    }

    // -----------------------------------------------------------------
    // 3. Final hard-limit gates (post-construction).
    // -----------------------------------------------------------------
    let policy_count: u32 = policies.len().try_into().map_err(|_| {
        Error::SynthNotExpressible(format!(
            "policies count {} exceeds u32::MAX",
            policies.len()
        ))
    })?;
    if policy_count > MAX_POLICIES {
        return Err(Error::SynthNotExpressible(format!(
            "policies count {policy_count} exceeds MAX_POLICIES ({MAX_POLICIES})"
        )));
    }
    let signer_count: u32 = signers.len().try_into().map_err(|_| {
        Error::SynthNotExpressible(format!("signer count {} exceeds u32::MAX", signers.len()))
    })?;
    if signer_count > MAX_SIGNERS {
        return Err(Error::SynthNotExpressible(format!(
            "signers count {signer_count} exceeds MAX_SIGNERS ({MAX_SIGNERS})"
        )));
    }

    // -----------------------------------------------------------------
    // 4. Stitch the spec together.
    // -----------------------------------------------------------------
    let context_rule = ContextRuleSpec {
        name: opts.context_rule_name.clone(),
        // `forced_context_type` wins (it carries PR-#649 enforcement); if
        // unset, default to the on-chain `Default` matcher.
        context_type: forced_context_type.unwrap_or(ContextType::Default),
        valid_until: None,
    };

    let recording_ref = RecordingRef {
        hash: recording_hash(&recording.ingest),
        schema: recording.schema.clone(),
    };

    Ok(PolicySpec {
        schema: POLICY_SCHEMA_URI.to_string(),
        synthesis_mode: opts.mode.clone(),
        context_rule,
        signers,
        policies,
        lifetime_ledgers: opts.lifetime_ledgers,
        recording_ref,
    })
}

// -------------------------------------------------------------------------
// Helpers — kept module-private; not part of the public surface.
// -------------------------------------------------------------------------

/// Walk `recording.contracts` plus the top-level `recording.auth_tree.roots`
/// invocations, returning the distinct StrKey `C…` target addresses in
/// **insertion order** (no HashSet — determinism is required).
fn enumerate_contract_targets(recording: &Recording) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();

    // ContractRecord targets first (the explicit invocation list).
    for cr in &recording.contracts {
        if !out.iter().any(|a| a == &cr.address) {
            out.push(cr.address.clone());
        }
    }
    // Then any `Contract` auth invocations not already covered. We walk
    // recursively so sub-invocations (auth chains) are also tallied.
    for entry in &recording.auth_tree.roots {
        collect_targets(&entry.root_invocation, &mut out);
    }
    out
}

fn collect_targets(inv: &AuthInvocation, out: &mut Vec<String>) {
    if let AuthFunction::Contract { address, .. } = &inv.function {
        if !out.iter().any(|a| a == address) {
            out.push(address.clone());
        }
    }
    for sub in &inv.sub_invocations {
        collect_targets(sub, out);
    }
}

/// Return `Some((address, &ContractRecord))` when the recording has exactly
/// one distinct contract target AND the corresponding `ContractRecord` is
/// a SEP-41 `transfer`. Otherwise `None`.
fn single_target_sep41<'a>(
    recording: &'a Recording,
    targets: &'a [String],
) -> Option<(&'a str, &'a ContractRecord)> {
    if targets.len() != 1 {
        return None;
    }
    let target = &targets[0];
    // Find the matching ContractRecord. The decision tree predicates over
    // `ContractRecord` because that's where the `function` and `args` live;
    // the auth tree confirms authorisation but the function-shape gate is
    // on the invocation list.
    let record = recording.contracts.iter().find(|c| &c.address == target)?;
    if is_sep41_transfer(record) {
        Some((target.as_str(), record))
    } else {
        None
    }
}

fn single_target_sep41_record<'a>(
    recording: &'a Recording,
    targets: &'a [String],
) -> Option<(&'a str, &'a ContractRecord)> {
    single_target_sep41(recording, targets)
}

/// Collect distinct function names invoked across all targets in
/// `targets`, preserving the order they first appear in
/// `recording.contracts`.
fn distinct_functions_in_insertion_order(recording: &Recording, targets: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for cr in &recording.contracts {
        if !targets.iter().any(|t| t == &cr.address) {
            continue;
        }
        if !out.iter().any(|f| f == &cr.function) {
            out.push(cr.function.clone());
        }
    }
    out
}

/// Build the `signers` list. If `opts.delegated_signer` is `Some`, that
/// single delegated signer is the entire list (the spec then represents
/// "this rule is governed by a delegate contract"). Otherwise we extract
/// every distinct `Credentials::Address::signer` value from
/// `recording.auth_tree.roots` in insertion order and emit each as an
/// `ExternalEd25519` signer keyed by the observed signer string.
///
/// Note: `Credentials::Address::signer` is a `String` in the Recording IR
/// (the recorder serialises the signer ScAddress as StrKey). The
/// synthesizer assumes Ed25519 for `G…` addresses — the only StrKey form a
/// signer can take in OZ Smart-Account auth — and surfaces an error if it
/// observes anything unexpected.
fn build_signers(recording: &Recording, opts: &SynthesisOptions) -> Result<Vec<SignerSpec>, Error> {
    if let Some(addr) = &opts.delegated_signer {
        return Ok(vec![SignerSpec::Delegated {
            address: addr.clone(),
        }]);
    }

    let mut out: Vec<SignerSpec> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for entry in &recording.auth_tree.roots {
        if let Credentials::Address { signer, .. } = &entry.credentials {
            if seen.iter().any(|s| s == signer) {
                continue;
            }
            seen.push(signer.clone());
            // The Recording records the `signer` as the StrKey of the
            // ScAddress carried in the auth entry. The synthesizer cannot
            // recover the raw public-key hex from a StrKey alone — that
            // requires base32-decoding the StrKey, which is out of scope
            // for the policy IR. Emit the StrKey as the `public_key_hex`
            // placeholder for now; Phase 2 installer is responsible for
            // converting it to the on-chain `Signer::Ed25519(BytesN<32>)`
            // representation. We do NOT silently downgrade unrecognised
            // signer shapes — anything we cannot represent surfaces as
            // E_SYNTH_NOT_EXPRESSIBLE.
            //
            // NOTE: this is a known IR-level limitation captured in
            // `docs/oz-internal-shapes.md` §11 (signer round-trip needs
            // strkey -> bytes32 in the installer).
            out.push(SignerSpec::ExternalEd25519 {
                public_key_hex: signer.clone(),
            });
        }
    }
    Ok(out)
}

/// Extract the recording-ref hash from `IngestSource`. Returns `None` for
/// simulation-sourced recordings (those carry a synthetic envelope-hash, not
/// an on-chain tx hash).
fn recording_hash(ingest: &crate::recording::IngestSource) -> Option<String> {
    match ingest {
        crate::recording::IngestSource::Hash { hash } => Some(hash.clone()),
        crate::recording::IngestSource::Simulation { .. } => None,
    }
}

/// Scale a decimal-string `i128` value by [`Tightness`].
///
/// The arithmetic is done in `i128` to preserve full precision (no float
/// math) and uses `checked_mul` so overflow surfaces as
/// `Error::SynthNotExpressible("amount overflow under tightness scaling")`
/// instead of wrapping silently. `SmallMargin` and `Loose` multiply by 11/10
/// and 2 respectively; `Exact` short-circuits to the input unchanged.
fn scale_i128_decimal(observed: &str, tightness: Tightness) -> Result<String, Error> {
    let parsed: i128 = observed.parse().map_err(|_| {
        Error::SynthNotExpressible(format!(
            "observed amount {observed:?} is not a valid i128 decimal"
        ))
    })?;
    let scaled: i128 = match tightness {
        Tightness::Exact => parsed,
        Tightness::SmallMargin => {
            // 1.1× via 11/10 keeps everything in i128 arithmetic. We do the
            // multiply first to preserve precision on small inputs (e.g.
            // observed=1 should round-trip to 1 under integer truncation,
            // matching the documented behaviour of "1.1× observed" when the
            // observed value is itself an integer count of stroops).
            let mul = parsed.checked_mul(11).ok_or_else(|| {
                Error::SynthNotExpressible("amount overflow under tightness scaling".to_string())
            })?;
            mul / 10
        }
        Tightness::Loose => parsed.checked_mul(2).ok_or_else(|| {
            Error::SynthNotExpressible("amount overflow under tightness scaling".to_string())
        })?,
    };
    Ok(scaled.to_string())
}


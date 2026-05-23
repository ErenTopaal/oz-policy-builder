//! Phase 4 Stream B — boundary-mutation deny-vector generator.
//!
//! For a given `(spec, recording, seed)` triple this module emits a
//! deterministic `Vec<DenyVector>` covering every constraint primitive carried
//! by `spec.policies`. Each vector encodes a `(payload, contexts)` pair that
//! Stream A's host driver (`crate::host`) replays through the smart account's
//! `__check_auth` entrypoint; the expected behavior is that the policy panics
//! with `expected_error_code`.
//!
//! # Error-code provenance
//!
//! Every `expected_error_code` is either:
//!
//! * **OZ-defined** (3xxx range), sourced from `docs/oz-internal-shapes.md` §5
//!   tables — `SimpleThresholdError::NotAllowed = 3202`,
//!   `WeightedThresholdError::NotAllowed = 3213`,
//!   `SpendingLimitError::SpendingLimitExceeded = 3221`,
//!   `SpendingLimitError::NotAllowed = 3223`.
//! * **Generated-policy code** (1xxx range), sourced from
//!   `templates/base.rs.jinja:80-117` — `PolicyError::FunctionNotAllowed =
//!   1010`, `ArgumentMismatch = 1020`, `AmountOutOfRange = 1030`,
//!   `AssetNotAllowed = 1040`, `TimeWindowViolated = 1050`,
//!   `CallFrequencyExceeded = 1060`, `SequenceOrderingViolated = 1070`.
//!
//! No code in this file fabricates a code; the constants below name their
//! source.
//!
//! # Determinism contract
//!
//! `generate_deny_vectors(spec, recording, seed)` is a pure function of its
//! inputs. The boundary-mutation strategies use `i128::checked_add` /
//! `i128::checked_sub` saturating to `i128::MAX` / `i128::MIN` so a degenerate
//! spec — e.g. `max = i128::MAX` — still produces a well-defined vector. The
//! per-primitive `proptest::strategy::Strategy` impls below remain available
//! for the unit tests, which seed their own `TestRunner` with
//! `RngAlgorithm::ChaCha` so each strategy is also reproducible in isolation.

use crate::host::{AuthPayload, TestContext};
use oz_policy_core::recording::Recording;
use oz_policy_core::spec::{
    ArgMatcher, Constraint, ExistingPrimitive, ExistingPrimitiveParams, PolicySlot, PolicySpec,
};
use oz_policy_core::ArgValue;
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Error-code constants (named so their on-chain provenance is self-documenting)
// ---------------------------------------------------------------------------

/// `SimpleThresholdError::NotAllowed`. Source: `docs/oz-internal-shapes.md` §5,
/// `simple_threshold.rs:204-208` (verified via `gh api` against v0.7.1: the
/// `enforce` body panics with this variant when
/// `authenticated_signers.len() < threshold`).
pub const OZ_SIMPLE_THRESHOLD_NOT_ALLOWED: u32 = 3202;

/// `WeightedThresholdError::NotAllowed`. Source: `docs/oz-internal-shapes.md`
/// §5. Mirrors `SimpleThresholdError::NotAllowed` semantics, panicked when the
/// sum of authenticated signer weights is below `threshold`.
pub const OZ_WEIGHTED_THRESHOLD_NOT_ALLOWED: u32 = 3213;

/// `SpendingLimitError::SpendingLimitExceeded`. Source:
/// `docs/oz-internal-shapes.md` §5 / `spending_limit.rs:258-260` (verified via
/// `gh api` against v0.7.1).
pub const OZ_SPENDING_LIMIT_EXCEEDED: u32 = 3221;

/// `SpendingLimitError::NotAllowed`. Source:
/// `docs/oz-internal-shapes.md` §5 / `spending_limit.rs:286-292` (verified via
/// `gh api` against v0.7.1: the `enforce` body falls through to this variant
/// when the inner call is not a SEP-41 `transfer(Address, Address, i128)` —
/// i.e. when `fn_name != "transfer"`).
pub const OZ_SPENDING_LIMIT_NOT_ALLOWED: u32 = 3223;

// Generated-policy `PolicyError::*` codes — single source of truth is
// `templates/base.rs.jinja:80-117`. The naming below mirrors the variant
// identifiers exactly so future template renames trigger a compile-time
// search-and-replace here, too.

/// `PolicyError::FunctionNotAllowed`. Source: `templates/base.rs.jinja:91`.
pub const POLICY_FUNCTION_NOT_ALLOWED: u32 = 1010;

/// `PolicyError::ArgumentMismatch`. Source: `templates/base.rs.jinja:95`.
pub const POLICY_ARGUMENT_MISMATCH: u32 = 1020;

/// `PolicyError::AmountOutOfRange`. Source: `templates/base.rs.jinja:99`.
pub const POLICY_AMOUNT_OUT_OF_RANGE: u32 = 1030;

/// `PolicyError::AssetNotAllowed`. Source: `templates/base.rs.jinja:103`.
pub const POLICY_ASSET_NOT_ALLOWED: u32 = 1040;

/// `PolicyError::TimeWindowViolated`. Source: `templates/base.rs.jinja:107`.
pub const POLICY_TIME_WINDOW_VIOLATED: u32 = 1050;

/// `PolicyError::CallFrequencyExceeded`. Source: `templates/base.rs.jinja:111`.
pub const POLICY_CALL_FREQUENCY_EXCEEDED: u32 = 1060;

/// `PolicyError::SequenceOrderingViolated`. Source: `templates/base.rs.jinja:115`.
pub const POLICY_SEQUENCE_ORDERING_VIOLATED: u32 = 1070;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// One deny vector — a `(payload, contexts)` pair that the harness replays
/// against `__check_auth` and expects to panic with `expected_error_code`.
///
/// Implements `Serialize` + `Deserialize` so the Phase 4 determinism gate
/// (`generate_deny_vectors_is_byte_equal_for_same_seed`) can JSON-encode two
/// vectors and compare them as strings, and so the CLI's
/// `simulate --extra-deny <json>` flag (Stream A's `run.rs`) can round-trip
/// extra deny vectors through disk. `Eq` is derivable now that
/// `oz_policy_core::ArgValue` is `Eq` (Phase 4 Stream-A extension; no float
/// variants exist in the `ScVal` shape — see `host.rs:134-136`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DenyVector {
    /// Stable identifier for this vector (`"<primitive>_<mutation>"`).
    /// Required to be byte-equal across two invocations with the same
    /// `(spec, recording, seed)`.
    pub name: String,
    pub payload: AuthPayload,
    pub contexts: Vec<TestContext>,
    /// Discriminant of the `PolicyError` / `*Error` enum variant the harness
    /// expects the policy WASM to panic with.
    pub expected_error_code: u32,
}

// ---------------------------------------------------------------------------
// Public generator
// ---------------------------------------------------------------------------

/// Generate boundary-mutation deny vectors for each constraint primitive in
/// `spec.policies`.
///
/// The generator is **deterministic**: two calls with the same
/// `(spec, recording, seed)` produce byte-equal `Vec<DenyVector>` outputs.
/// `seed` controls the `proptest::test_runner::TestRunner`'s ChaCha-backed
/// RNG; that RNG is only consulted for primitives whose mutation has a
/// strategy-driven boundary value (currently `AmountRange` and the
/// `ArgumentPattern` `Range` matcher).
///
/// If `spec.policies` references a primitive this generator does not yet
/// handle, the function emits a `DenyVector` named `"TODO: <primitive>"` with
/// `expected_error_code = 0` so the downstream harness fails loudly rather
/// than silently skipping (per the Stream-B brief, "no silent skip of
/// primitives").
pub fn generate_deny_vectors(
    spec: &PolicySpec,
    recording: &Recording,
    seed: u64,
) -> Vec<DenyVector> {
    let mut out = Vec::new();
    let mut runner = make_runner(seed);

    for (slot_index, slot) in spec.policies.iter().enumerate() {
        match slot {
            PolicySlot::Existing { primitive, params } => match (primitive, params) {
                (
                    ExistingPrimitive::SpendingLimit,
                    ExistingPrimitiveParams::SpendingLimit {
                        limit_stroops_string,
                        ..
                    },
                ) => {
                    push_spending_limit_vectors(
                        &mut out,
                        slot_index,
                        recording,
                        limit_stroops_string,
                        &mut runner,
                    );
                }
                (
                    ExistingPrimitive::SimpleThreshold,
                    ExistingPrimitiveParams::SimpleThreshold { threshold },
                ) => {
                    push_threshold_vectors(
                        &mut out,
                        slot_index,
                        recording,
                        *threshold,
                        OZ_SIMPLE_THRESHOLD_NOT_ALLOWED,
                        "simple_threshold",
                    );
                }
                (
                    ExistingPrimitive::WeightedThreshold,
                    ExistingPrimitiveParams::WeightedThreshold { threshold, .. },
                ) => {
                    push_threshold_vectors(
                        &mut out,
                        slot_index,
                        recording,
                        *threshold,
                        OZ_WEIGHTED_THRESHOLD_NOT_ALLOWED,
                        "weighted_threshold",
                    );
                }
                // Mismatched (primitive, params) shape — the type system
                // allows this combination but the synthesizer should never
                // emit it. Surface it as a loud TODO rather than silently
                // skipping.
                _ => out.push(unhandled_vector(slot_index, "existing_primitive_mismatch")),
            },
            PolicySlot::Generated { constraints, .. } => {
                for (cidx, constraint) in constraints.iter().enumerate() {
                    push_generated_vectors(
                        &mut out,
                        slot_index,
                        cidx,
                        constraint,
                        recording,
                        &mut runner,
                    );
                }
            }
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Per-primitive emitters
// ---------------------------------------------------------------------------

fn push_spending_limit_vectors(
    out: &mut Vec<DenyVector>,
    slot_index: usize,
    recording: &Recording,
    limit_stroops_string: &str,
    _runner: &mut TestRunner,
) {
    let Some(transfer_record) = first_sep41_transfer(recording) else {
        // No SEP-41 transfer to mutate — the spec is malformed (Phase 2
        // decision tree should have rejected it before reaching here). Emit
        // a loud TODO so we don't silently lose coverage.
        out.push(unhandled_vector(slot_index, "spending_limit_no_transfer"));
        return;
    };

    let limit: i128 = limit_stroops_string.parse().unwrap_or(0);
    let signer_addresses = signer_addresses_from(recording);

    // Vector 1 — amount_2x_cap: 2 * limit, saturating on overflow.
    let two_x = limit.saturating_mul(2);
    out.push(DenyVector {
        name: format!("slot{slot_index}_spending_limit_amount_2x_cap"),
        payload: AuthPayload {
            signer_addresses: signer_addresses.clone(),
            context_rule_ids: vec![0],
        },
        contexts: vec![mutate_transfer_amount(transfer_record, two_x)],
        expected_error_code: OZ_SPENDING_LIMIT_EXCEEDED,
    });

    // Vector 2 — amount_just_over_cap: limit + 1, saturating.
    let just_over = limit.saturating_add(1);
    out.push(DenyVector {
        name: format!("slot{slot_index}_spending_limit_amount_just_over_cap"),
        payload: AuthPayload {
            signer_addresses: signer_addresses.clone(),
            context_rule_ids: vec![0],
        },
        contexts: vec![mutate_transfer_amount(transfer_record, just_over)],
        expected_error_code: OZ_SPENDING_LIMIT_EXCEEDED,
    });

    // Vector 3 — wrong_function: `transfer` -> `approve`. Verified against
    // `spending_limit.rs:286-292` (v0.7.1): the enforce body falls through to
    // `panic_with_error!(e, SpendingLimitError::NotAllowed)` when fn_name is
    // not `transfer` AND the context rule is `CallContract(_)` (already
    // enforced at install time). So the runtime check fires, not the rule-
    // level filter — see the brief's note on this primitive.
    let mut wrong_fn_ctx = TestContext {
        contract_address: transfer_record.address.clone(),
        function_name: "approve".to_string(),
        args: transfer_record.args.clone(),
    };
    // SEP-41 approve takes (from, spender, amount, expiration_ledger). Pad
    // args[3] with a placeholder so the call shape is well-formed even
    // though the policy rejects on fn_name first.
    if wrong_fn_ctx.args.len() == 3 {
        wrong_fn_ctx.args.push(ArgValue::U32(0));
    }
    out.push(DenyVector {
        name: format!("slot{slot_index}_spending_limit_wrong_function"),
        payload: AuthPayload {
            signer_addresses,
            context_rule_ids: vec![0],
        },
        contexts: vec![wrong_fn_ctx],
        expected_error_code: OZ_SPENDING_LIMIT_NOT_ALLOWED,
    });
}

fn push_threshold_vectors(
    out: &mut Vec<DenyVector>,
    slot_index: usize,
    recording: &Recording,
    threshold: u32,
    expected_error_code: u32,
    primitive_name: &str,
) {
    let mut signers = signer_addresses_from(recording);

    // Take `threshold - 1` signers; if the recording exposes fewer signers
    // than the threshold the synthesizer's own validation should have
    // rejected the spec, but we still emit a vector with whatever's available
    // (capped at `threshold - 1`) so the harness exercises the under-
    // threshold path.
    let want = threshold.saturating_sub(1) as usize;
    if signers.len() > want {
        signers.truncate(want);
    }

    // Use the first contract record as the context shape if available; the
    // policy rejects on signer count before reading the call args, so the
    // exact shape isn't load-bearing — it just needs to be a well-formed
    // `Context::Contract`.
    let contexts = recording
        .contracts
        .first()
        .map(|c| vec![context_from_record(c)])
        .unwrap_or_default();

    out.push(DenyVector {
        name: format!("slot{slot_index}_{primitive_name}_signer_count_below_threshold"),
        payload: AuthPayload {
            signer_addresses: signers,
            context_rule_ids: vec![0],
        },
        contexts,
        expected_error_code,
    });
}

fn push_generated_vectors(
    out: &mut Vec<DenyVector>,
    slot_index: usize,
    constraint_index: usize,
    constraint: &Constraint,
    recording: &Recording,
    runner: &mut TestRunner,
) {
    let signer_addresses = signer_addresses_from(recording);
    let make_ctx = |args: Vec<ArgValue>, fn_name: &str, contract_addr: &str| TestContext {
        contract_address: contract_addr.to_string(),
        function_name: fn_name.to_string(),
        args,
    };
    let first_record = recording.contracts.first();

    match constraint {
        Constraint::FunctionAllowlist { functions } => {
            // Pick the deterministic "not-in-allowlist" placeholder unless it
            // unexpectedly collides (defense in depth).
            let mut name = "definitely_not_in_allowlist".to_string();
            while functions.iter().any(|f| f == &name) {
                name.push('_');
            }
            let (contract_addr, args) = first_record
                .map(|r| (r.address.clone(), r.args.clone()))
                .unwrap_or_else(|| (placeholder_contract_address(), vec![]));
            out.push(DenyVector {
                name: format!(
                    "slot{slot_index}_c{constraint_index}_function_allowlist_wrong_function"
                ),
                payload: AuthPayload {
                    signer_addresses,
                    context_rule_ids: vec![0],
                },
                contexts: vec![make_ctx(args, &name, &contract_addr)],
                expected_error_code: POLICY_FUNCTION_NOT_ALLOWED,
            });
        }

        Constraint::ArgumentPattern {
            fn_name,
            arg_index,
            matcher,
        } => {
            let (contract_addr, mut args) = first_record
                .map(|r| (r.address.clone(), r.args.clone()))
                .unwrap_or_else(|| (placeholder_contract_address(), vec![]));
            // Ensure args[arg_index] exists.
            while args.len() <= *arg_index as usize {
                args.push(ArgValue::U32(0));
            }
            let mutated = mutate_arg_for_mismatch(matcher, &args[*arg_index as usize], runner);
            args[*arg_index as usize] = mutated;
            out.push(DenyVector {
                name: format!("slot{slot_index}_c{constraint_index}_argument_pattern_mismatch"),
                payload: AuthPayload {
                    signer_addresses,
                    context_rule_ids: vec![0],
                },
                contexts: vec![make_ctx(args, fn_name, &contract_addr)],
                expected_error_code: POLICY_ARGUMENT_MISMATCH,
            });
        }

        Constraint::AmountRange {
            fn_name,
            arg_index,
            min_string,
            max_string,
        } => {
            let (contract_addr, base_args) = first_record
                .map(|r| (r.address.clone(), r.args.clone()))
                .unwrap_or_else(|| (placeholder_contract_address(), vec![]));

            // Vector — amount_above_max
            if let Some(max_str) = max_string {
                let max: i128 = max_str.parse().unwrap_or(0);
                let above = pick_above_max(max, runner);
                let mut args = base_args.clone();
                set_arg_i128(&mut args, *arg_index as usize, above);
                out.push(DenyVector {
                    name: format!("slot{slot_index}_c{constraint_index}_amount_range_above_max"),
                    payload: AuthPayload {
                        signer_addresses: signer_addresses.clone(),
                        context_rule_ids: vec![0],
                    },
                    contexts: vec![make_ctx(args, fn_name, &contract_addr)],
                    expected_error_code: POLICY_AMOUNT_OUT_OF_RANGE,
                });
            }

            // Vector — amount_below_min
            if let Some(min_str) = min_string {
                let min: i128 = min_str.parse().unwrap_or(0);
                let below = pick_below_min(min, runner);
                let mut args = base_args.clone();
                set_arg_i128(&mut args, *arg_index as usize, below);
                out.push(DenyVector {
                    name: format!("slot{slot_index}_c{constraint_index}_amount_range_below_min"),
                    payload: AuthPayload {
                        signer_addresses,
                        context_rule_ids: vec![0],
                    },
                    contexts: vec![make_ctx(args, fn_name, &contract_addr)],
                    expected_error_code: POLICY_AMOUNT_OUT_OF_RANGE,
                });
            }
        }

        Constraint::AssetAllowlist { assets } => {
            let mut wrong_asset =
                "CDISALLOWEDXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX".to_string();
            // Defense in depth: lengthen the placeholder if it ever clashes
            // with a listed asset.
            while assets.iter().any(|a| a == &wrong_asset) {
                wrong_asset.push('X');
            }
            let (_, args) = first_record
                .map(|r| (r.address.clone(), r.args.clone()))
                .unwrap_or_else(|| (placeholder_contract_address(), vec![]));
            let fn_name = first_record
                .map(|r| r.function.clone())
                .unwrap_or_else(|| "invoke".to_string());
            out.push(DenyVector {
                name: format!("slot{slot_index}_c{constraint_index}_asset_allowlist_wrong_asset"),
                payload: AuthPayload {
                    signer_addresses,
                    context_rule_ids: vec![0],
                },
                contexts: vec![make_ctx(args, &fn_name, &wrong_asset)],
                expected_error_code: POLICY_ASSET_NOT_ALLOWED,
            });
        }

        Constraint::TimeWindow {
            start_ledger,
            end_ledger,
        } => {
            // `TestContext` (as defined in the Stream-B brief) carries no
            // ledger_seq field. The harness (Stream A) must set the host's
            // ledger sequence before replaying each vector. The vector's
            // `name` carries the target ledger value so Stream A's run.rs
            // can recover it via a `before_window_start_at_<ledger>` /
            // `after_window_end_at_<ledger>` suffix.
            //
            // Coordination note for Stream A: if `TestContext` eventually
            // grows a `ledger_seq_override: Option<u32>` field, replace the
            // name-encoding with the typed field. The error code stays the
            // same.
            let (contract_addr, args, fn_name) = first_record
                .map(|r| (r.address.clone(), r.args.clone(), r.function.clone()))
                .unwrap_or_else(|| (placeholder_contract_address(), vec![], "invoke".to_string()));

            let before = start_ledger.saturating_sub(1);
            out.push(DenyVector {
                name: format!(
                    "slot{slot_index}_c{constraint_index}_time_window_before_window_start_at_{before}"
                ),
                payload: AuthPayload {
                    signer_addresses: signer_addresses.clone(),
                    context_rule_ids: vec![0],
                },
                contexts: vec![make_ctx(args.clone(), &fn_name, &contract_addr)],
                expected_error_code: POLICY_TIME_WINDOW_VIOLATED,
            });

            let after = end_ledger.saturating_add(1);
            out.push(DenyVector {
                name: format!(
                    "slot{slot_index}_c{constraint_index}_time_window_after_window_end_at_{after}"
                ),
                payload: AuthPayload {
                    signer_addresses,
                    context_rule_ids: vec![0],
                },
                contexts: vec![make_ctx(args, &fn_name, &contract_addr)],
                expected_error_code: POLICY_TIME_WINDOW_VIOLATED,
            });
        }

        Constraint::CallFrequency {
            max_calls,
            window_ledgers: _,
        } => {
            // N+1 calls of the same context. The harness runs each context
            // in turn against the same host (no ledger advance between
            // them); the last invocation must panic with
            // CallFrequencyExceeded.
            let n_plus_one = max_calls.saturating_add(1) as usize;
            let (contract_addr, args, fn_name) = first_record
                .map(|r| (r.address.clone(), r.args.clone(), r.function.clone()))
                .unwrap_or_else(|| (placeholder_contract_address(), vec![], "invoke".to_string()));
            let single = make_ctx(args, &fn_name, &contract_addr);
            let contexts = (0..n_plus_one).map(|_| single.clone()).collect();
            out.push(DenyVector {
                name: format!(
                    "slot{slot_index}_c{constraint_index}_call_frequency_n_plus_one_calls_in_window"
                ),
                payload: AuthPayload {
                    signer_addresses,
                    context_rule_ids: vec![0],
                },
                contexts,
                expected_error_code: POLICY_CALL_FREQUENCY_EXCEEDED,
            });
        }

        Constraint::SequenceOrdering { phases } => {
            // Skip phase 0 by invoking phases[1] directly. If phases has < 2
            // entries the synthesizer should have rejected the spec; emit a
            // loud TODO in that case.
            if phases.len() < 2 {
                out.push(unhandled_vector(
                    slot_index,
                    "sequence_ordering_too_few_phases",
                ));
                return;
            }
            let (contract_addr, args) = first_record
                .map(|r| (r.address.clone(), r.args.clone()))
                .unwrap_or_else(|| (placeholder_contract_address(), vec![]));
            out.push(DenyVector {
                name: format!(
                    "slot{slot_index}_c{constraint_index}_sequence_ordering_phase_skipped"
                ),
                payload: AuthPayload {
                    signer_addresses,
                    context_rule_ids: vec![0],
                },
                contexts: vec![make_ctx(args, &phases[1], &contract_addr)],
                expected_error_code: POLICY_SEQUENCE_ORDERING_VIOLATED,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Proptest strategies (one per Constraint variant that has a numeric boundary)
// ---------------------------------------------------------------------------

/// Strategy for amounts strictly above `max`, bounded so the test runner
/// always terminates. The upper bound `max + 1_000_000` is wide enough to
/// flush rolling-window edge cases but stays well clear of `i128::MAX`.
pub fn arb_amount_above_max(max: i128) -> impl Strategy<Value = i128> {
    let upper = max.saturating_add(1_000_000);
    let lower = max.saturating_add(1);
    (lower..=upper).boxed()
}

/// Strategy for amounts strictly below `min`. Symmetric counterpart to
/// [`arb_amount_above_max`].
pub fn arb_amount_below_min(min: i128) -> impl Strategy<Value = i128> {
    let lower = min.saturating_sub(1_000_000);
    let upper = min.saturating_sub(1);
    (lower..=upper).boxed()
}

/// Strategy for ledger sequences strictly before `start_ledger`. Wraps to 0
/// at the lower boundary so the strategy is total over `u32`.
pub fn arb_ledger_before_window(start_ledger: u32) -> impl Strategy<Value = u32> {
    let upper = start_ledger.saturating_sub(1);
    (0u32..=upper).boxed()
}

/// Strategy for ledger sequences strictly after `end_ledger`.
pub fn arb_ledger_after_window(end_ledger: u32) -> impl Strategy<Value = u32> {
    let lower = end_ledger.saturating_add(1);
    let upper = lower.saturating_add(1_000_000);
    (lower..=upper).boxed()
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn make_runner(seed: u64) -> TestRunner {
    let cfg = Config {
        // 1 case is enough for the deterministic mutation — the strategy is
        // used to *pick* a boundary value, not to fuzz-search.
        cases: 1,
        ..Config::default()
    };
    // ChaCha-backed RNG with the caller-supplied seed (low 8 bytes; high 24
    // bytes zeroed). `RngAlgorithm::ChaCha` is `proptest`'s canonical
    // reproducible RNG choice.
    let mut seed_bytes = [0u8; 32];
    seed_bytes[..8].copy_from_slice(&seed.to_le_bytes());
    let rng = TestRng::from_seed(RngAlgorithm::ChaCha, &seed_bytes);
    TestRunner::new_with_rng(cfg, rng)
}

fn first_sep41_transfer(
    recording: &Recording,
) -> Option<&oz_policy_core::recording::ContractRecord> {
    recording
        .contracts
        .iter()
        .find(|c| oz_policy_core::is_sep41_transfer(c))
}

fn signer_addresses_from(recording: &Recording) -> Vec<String> {
    let mut out = Vec::new();
    for entry in &recording.auth_tree.roots {
        if let oz_policy_core::recording::Credentials::Address { signer, .. } = &entry.credentials {
            if !out.contains(signer) {
                out.push(signer.clone());
            }
        }
    }
    out
}

fn context_from_record(record: &oz_policy_core::recording::ContractRecord) -> TestContext {
    TestContext {
        contract_address: record.address.clone(),
        function_name: record.function.clone(),
        args: record.args.clone(),
    }
}

fn mutate_transfer_amount(
    record: &oz_policy_core::recording::ContractRecord,
    new_amount: i128,
) -> TestContext {
    let mut args = record.args.clone();
    if args.len() >= 3 {
        args[2] = ArgValue::I128(new_amount.to_string());
    } else {
        while args.len() < 3 {
            args.push(ArgValue::I128("0".to_string()));
        }
        args[2] = ArgValue::I128(new_amount.to_string());
    }
    TestContext {
        contract_address: record.address.clone(),
        function_name: record.function.clone(),
        args,
    }
}

fn set_arg_i128(args: &mut Vec<ArgValue>, index: usize, value: i128) {
    while args.len() <= index {
        args.push(ArgValue::I128("0".to_string()));
    }
    args[index] = ArgValue::I128(value.to_string());
}

fn pick_above_max(max: i128, runner: &mut TestRunner) -> i128 {
    let strategy = arb_amount_above_max(max);
    match strategy.new_tree(runner) {
        Ok(tree) => tree.current(),
        // Strategy refused (degenerate bounds) — fall back to the closest
        // valid value so we still emit a meaningful vector.
        Err(_) => max.saturating_add(1),
    }
}

fn pick_below_min(min: i128, runner: &mut TestRunner) -> i128 {
    let strategy = arb_amount_below_min(min);
    match strategy.new_tree(runner) {
        Ok(tree) => tree.current(),
        Err(_) => min.saturating_sub(1),
    }
}

fn placeholder_contract_address() -> String {
    "CPLACEHOLDERPLACEHOLDERPLACEHOLDERPLACEHOLDERPLACEHOLDERAAA".to_string()
}

fn unhandled_vector(slot_index: usize, primitive: &str) -> DenyVector {
    DenyVector {
        name: format!("TODO: slot{slot_index}_{primitive}"),
        payload: AuthPayload {
            signer_addresses: vec![],
            context_rule_ids: vec![],
        },
        contexts: vec![],
        // expected_error_code = 0 is deliberately reserved as the "no-real-
        // code" sentinel. Stream A's harness must treat code 0 as a hard
        // failure ("primitive not yet handled").
        expected_error_code: 0,
    }
}

fn mutate_arg_for_mismatch(
    matcher: &ArgMatcher,
    current: &ArgValue,
    runner: &mut TestRunner,
) -> ArgValue {
    match matcher {
        ArgMatcher::Exact { value } => not_equal_argvalue(value, current),
        ArgMatcher::Allowlist { values } => not_in_argvalue_set(values, current),
        ArgMatcher::Blocklist { values } => {
            // For a Blocklist the mismatch is: pick a value that IS on the
            // blocklist (i.e., would be rejected). Use the first listed
            // value if present; otherwise fall back to `not_equal_argvalue`
            // so we still emit a different value.
            values
                .first()
                .cloned()
                .unwrap_or_else(|| not_equal_argvalue(current, current))
        }
        ArgMatcher::Range {
            min_string,
            max_string,
        } => {
            // Try max+1 first, else min-1, else fall back to a sentinel.
            if let Some(max_str) = max_string {
                let max: i128 = max_str.parse().unwrap_or(0);
                ArgValue::I128(pick_above_max(max, runner).to_string())
            } else if let Some(min_str) = min_string {
                let min: i128 = min_str.parse().unwrap_or(0);
                ArgValue::I128(pick_below_min(min, runner).to_string())
            } else {
                // Degenerate range with neither bound; emit a sentinel.
                ArgValue::I128("0".to_string())
            }
        }
    }
}

fn not_equal_argvalue(reference: &ArgValue, fallback: &ArgValue) -> ArgValue {
    // Produce a value of the same general shape as `reference` that differs
    // from it. Falls back to a `fallback`-shaped sentinel when no obvious
    // "next" exists.
    match reference {
        ArgValue::Bool(b) => ArgValue::Bool(!b),
        ArgValue::U32(n) => ArgValue::U32(n.wrapping_add(1)),
        ArgValue::I32(n) => ArgValue::I32(n.wrapping_add(1)),
        ArgValue::U64(s) => {
            let n: u64 = s.parse().unwrap_or(0);
            ArgValue::U64(n.wrapping_add(1).to_string())
        }
        ArgValue::I64(s) => {
            let n: i64 = s.parse().unwrap_or(0);
            ArgValue::I64(n.wrapping_add(1).to_string())
        }
        ArgValue::U128(s) => {
            let n: u128 = s.parse().unwrap_or(0);
            ArgValue::U128(n.wrapping_add(1).to_string())
        }
        ArgValue::I128(s) => {
            let n: i128 = s.parse().unwrap_or(0);
            ArgValue::I128(n.wrapping_add(1).to_string())
        }
        ArgValue::Symbol(s) => ArgValue::Symbol(format!("{s}_x")),
        ArgValue::Address(_) => {
            ArgValue::Address("CMISMATCHXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXY".to_string())
        }
        _ => match fallback {
            ArgValue::U32(_) => ArgValue::U32(0xDEAD_BEEF),
            _ => ArgValue::U32(0xDEAD_BEEF),
        },
    }
}

fn not_in_argvalue_set(set: &[ArgValue], current: &ArgValue) -> ArgValue {
    // Walk a sequence of candidates derived from `current` (then a fixed
    // sentinel) until we find one that isn't in `set`. Bounded: emits at
    // most |set| + 2 candidates so we always terminate.
    let mut candidate = not_equal_argvalue(current, current);
    let mut tries = 0;
    while set.iter().any(|v| v == &candidate) && tries < set.len() + 2 {
        candidate = not_equal_argvalue(&candidate, current);
        tries += 1;
    }
    candidate
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use oz_policy_core::recording::{
        AuthEntry, AuthFunction, AuthInvocation, AuthTree, ContractRecord, Credentials,
        IngestSource, Recording, RECORDING_SCHEMA_URI,
    };
    use oz_policy_core::spec::{
        ContextRuleSpec, ContextType, PolicySpec, RecordingRef, SignerSpec, SynthesisMode,
        TemplateFamily, POLICY_SCHEMA_URI,
    };

    // ---------------------------------------------------------------
    // Test-fixture helpers
    // ---------------------------------------------------------------

    fn token_address() -> String {
        "CAQCFVLOBK5GIULPNZRGSXFJYDQRQXFLEAEKBNXG2QGGYKGV5MAYDOLJ".to_string()
    }

    fn signer_g() -> String {
        "GBXGQJUVLA45FVUMHE5NHRH4ESDBP3K2ZE5WUYBL5RMRXLB2RZAB4U2X".to_string()
    }

    fn sep41_transfer_recording() -> Recording {
        Recording {
            schema: RECORDING_SCHEMA_URI.to_string(),
            network_passphrase: "Test SDF Network ; September 2015".to_string(),
            ingest: IngestSource::Hash {
                hash: "deadbeef".to_string(),
            },
            ledger: Some(1000),
            contracts: vec![ContractRecord {
                address: token_address(),
                function: "transfer".to_string(),
                args: vec![
                    ArgValue::Address(signer_g()),
                    ArgValue::Address(
                        "GDESTXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX".to_string(),
                    ),
                    ArgValue::I128("100".to_string()),
                ],
            }],
            auth_tree: AuthTree {
                roots: vec![AuthEntry {
                    credentials: Credentials::Address {
                        signer: signer_g(),
                        nonce: "0".to_string(),
                        signature_expiration_ledger: 2000,
                        signature: ArgValue::Bytes {
                            hex: "00".to_string(),
                        },
                    },
                    root_invocation: AuthInvocation {
                        function: AuthFunction::Contract {
                            address: token_address(),
                            function: "transfer".to_string(),
                            args: vec![],
                        },
                        sub_invocations: vec![],
                    },
                    source_op_index: 0,
                }],
            },
            state_changes: vec![],
            events: vec![],
        }
    }

    fn base_spec(policies: Vec<PolicySlot>) -> PolicySpec {
        PolicySpec {
            schema: POLICY_SCHEMA_URI.to_string(),
            synthesis_mode: SynthesisMode::Auto,
            context_rule: ContextRuleSpec {
                name: "rule".to_string(),
                context_type: ContextType::CallContract {
                    address: token_address(),
                },
                valid_until: None,
            },
            signers: vec![SignerSpec::ExternalEd25519 {
                public_key_hex: "11".repeat(32),
            }],
            policies,
            lifetime_ledgers: None,
            recording_ref: RecordingRef {
                hash: Some("deadbeef".to_string()),
                schema: RECORDING_SCHEMA_URI.to_string(),
            },
        }
    }

    fn spending_limit_slot(limit: &str) -> PolicySlot {
        PolicySlot::Existing {
            primitive: ExistingPrimitive::SpendingLimit,
            params: ExistingPrimitiveParams::SpendingLimit {
                period_ledgers: 17280,
                limit_stroops_string: limit.to_string(),
            },
        }
    }

    fn simple_threshold_slot(threshold: u32) -> PolicySlot {
        PolicySlot::Existing {
            primitive: ExistingPrimitive::SimpleThreshold,
            params: ExistingPrimitiveParams::SimpleThreshold { threshold },
        }
    }

    fn weighted_threshold_slot(threshold: u32) -> PolicySlot {
        PolicySlot::Existing {
            primitive: ExistingPrimitive::WeightedThreshold,
            params: ExistingPrimitiveParams::WeightedThreshold {
                weights: vec![],
                threshold,
            },
        }
    }

    fn generated_slot(constraints: Vec<Constraint>, family: TemplateFamily) -> PolicySlot {
        PolicySlot::Generated {
            template_family: family,
            constraints,
        }
    }

    // ---------------------------------------------------------------
    // Per-primitive tests
    // ---------------------------------------------------------------

    #[test]
    fn spending_limit_emits_amount_2x_and_just_over_and_wrong_function() {
        let spec = base_spec(vec![spending_limit_slot("100")]);
        let rec = sep41_transfer_recording();
        let vectors = generate_deny_vectors(&spec, &rec, 42);
        assert!(!vectors.is_empty(), "must emit vectors for SpendingLimit");
        let names: Vec<&str> = vectors.iter().map(|v| v.name.as_str()).collect();
        assert!(
            names.iter().any(|n| n.contains("amount_2x_cap")),
            "expected amount_2x_cap, got {names:?}"
        );
        assert!(
            names.iter().any(|n| n.contains("amount_just_over_cap")),
            "expected amount_just_over_cap, got {names:?}"
        );
        assert!(
            names.iter().any(|n| n.contains("wrong_function")),
            "expected wrong_function, got {names:?}"
        );
        // Amount-overrun vectors expect the on-chain SpendingLimitExceeded
        // code; the wrong_function vector expects the runtime-fallthrough
        // NotAllowed code (see spending_limit.rs:286-292 verification in the
        // module-level docs).
        let overruns: Vec<&DenyVector> = vectors
            .iter()
            .filter(|v| v.name.contains("amount"))
            .collect();
        assert!(!overruns.is_empty());
        for v in &overruns {
            assert_eq!(v.expected_error_code, OZ_SPENDING_LIMIT_EXCEEDED);
        }
        let wrong_fn: &DenyVector = vectors
            .iter()
            .find(|v| v.name.contains("wrong_function"))
            .unwrap();
        assert_eq!(wrong_fn.expected_error_code, OZ_SPENDING_LIMIT_NOT_ALLOWED);
    }

    #[test]
    fn simple_threshold_emits_below_threshold_vector() {
        let spec = base_spec(vec![simple_threshold_slot(2)]);
        let rec = sep41_transfer_recording();
        let vectors = generate_deny_vectors(&spec, &rec, 1);
        assert_eq!(vectors.len(), 1);
        assert!(vectors[0]
            .name
            .contains("simple_threshold_signer_count_below_threshold"));
        assert_eq!(
            vectors[0].expected_error_code,
            OZ_SIMPLE_THRESHOLD_NOT_ALLOWED
        );
        // threshold=2, recording has 1 signer; want = threshold - 1 = 1
        // signer surviving the truncate. Since recording exposes exactly one
        // signer, the resulting payload also has one signer.
        assert_eq!(vectors[0].payload.signer_addresses.len(), 1);
    }

    #[test]
    fn weighted_threshold_emits_below_threshold_vector() {
        let spec = base_spec(vec![weighted_threshold_slot(3)]);
        let rec = sep41_transfer_recording();
        let vectors = generate_deny_vectors(&spec, &rec, 1);
        assert_eq!(vectors.len(), 1);
        assert!(vectors[0]
            .name
            .contains("weighted_threshold_signer_count_below_threshold"));
        assert_eq!(
            vectors[0].expected_error_code,
            OZ_WEIGHTED_THRESHOLD_NOT_ALLOWED
        );
    }

    #[test]
    fn function_allowlist_emits_wrong_function_vector() {
        let spec = base_spec(vec![generated_slot(
            vec![Constraint::FunctionAllowlist {
                functions: vec!["transfer".to_string()],
            }],
            TemplateFamily::FunctionAllowlist,
        )]);
        let rec = sep41_transfer_recording();
        let vectors = generate_deny_vectors(&spec, &rec, 1);
        assert_eq!(vectors.len(), 1);
        assert!(vectors[0]
            .name
            .contains("function_allowlist_wrong_function"));
        assert_eq!(vectors[0].expected_error_code, POLICY_FUNCTION_NOT_ALLOWED);
        // The mutated function name must not be on the allowlist.
        assert_eq!(
            vectors[0].contexts[0].function_name,
            "definitely_not_in_allowlist"
        );
    }

    #[test]
    fn argument_pattern_exact_emits_mismatch_vector() {
        let spec = base_spec(vec![generated_slot(
            vec![Constraint::ArgumentPattern {
                fn_name: "transfer".to_string(),
                arg_index: 2,
                matcher: ArgMatcher::Exact {
                    value: ArgValue::I128("100".to_string()),
                },
            }],
            TemplateFamily::ArgumentPattern,
        )]);
        let rec = sep41_transfer_recording();
        let vectors = generate_deny_vectors(&spec, &rec, 1);
        assert_eq!(vectors.len(), 1);
        assert!(vectors[0].name.contains("argument_pattern_mismatch"));
        assert_eq!(vectors[0].expected_error_code, POLICY_ARGUMENT_MISMATCH);
        // The mutated args[2] must differ from the Exact matcher's value.
        let mutated = &vectors[0].contexts[0].args[2];
        assert_ne!(mutated, &ArgValue::I128("100".to_string()));
    }

    #[test]
    fn argument_pattern_allowlist_emits_mismatch_outside_list() {
        let spec = base_spec(vec![generated_slot(
            vec![Constraint::ArgumentPattern {
                fn_name: "transfer".to_string(),
                arg_index: 2,
                matcher: ArgMatcher::Allowlist {
                    values: vec![
                        ArgValue::I128("100".to_string()),
                        ArgValue::I128("200".to_string()),
                    ],
                },
            }],
            TemplateFamily::ArgumentPattern,
        )]);
        let rec = sep41_transfer_recording();
        let vectors = generate_deny_vectors(&spec, &rec, 1);
        assert_eq!(vectors.len(), 1);
        let mutated = &vectors[0].contexts[0].args[2];
        assert_ne!(mutated, &ArgValue::I128("100".to_string()));
        assert_ne!(mutated, &ArgValue::I128("200".to_string()));
    }

    #[test]
    fn amount_range_emits_above_and_below_vectors() {
        let spec = base_spec(vec![generated_slot(
            vec![Constraint::AmountRange {
                fn_name: "transfer".to_string(),
                arg_index: 2,
                min_string: Some("10".to_string()),
                max_string: Some("100".to_string()),
            }],
            TemplateFamily::AmountRange,
        )]);
        let rec = sep41_transfer_recording();
        let vectors = generate_deny_vectors(&spec, &rec, 1);
        assert_eq!(vectors.len(), 2);
        let names: Vec<&str> = vectors.iter().map(|v| v.name.as_str()).collect();
        assert!(names.iter().any(|n| n.contains("amount_range_above_max")));
        assert!(names.iter().any(|n| n.contains("amount_range_below_min")));
        for v in &vectors {
            assert_eq!(v.expected_error_code, POLICY_AMOUNT_OUT_OF_RANGE);
            assert!(matches!(v.contexts[0].args[2], ArgValue::I128(_)));
        }
    }

    #[test]
    fn asset_allowlist_emits_wrong_asset_vector() {
        let spec = base_spec(vec![generated_slot(
            vec![Constraint::AssetAllowlist {
                assets: vec![token_address()],
            }],
            TemplateFamily::AssetAllowlist,
        )]);
        let rec = sep41_transfer_recording();
        let vectors = generate_deny_vectors(&spec, &rec, 1);
        assert_eq!(vectors.len(), 1);
        assert!(vectors[0].name.contains("asset_allowlist_wrong_asset"));
        assert_eq!(vectors[0].expected_error_code, POLICY_ASSET_NOT_ALLOWED);
        assert_ne!(vectors[0].contexts[0].contract_address, token_address());
    }

    #[test]
    fn time_window_emits_before_and_after_vectors() {
        let spec = base_spec(vec![generated_slot(
            vec![Constraint::TimeWindow {
                start_ledger: 100,
                end_ledger: 200,
            }],
            TemplateFamily::TimeWindow,
        )]);
        let rec = sep41_transfer_recording();
        let vectors = generate_deny_vectors(&spec, &rec, 1);
        assert_eq!(vectors.len(), 2);
        let names: Vec<&str> = vectors.iter().map(|v| v.name.as_str()).collect();
        assert!(names
            .iter()
            .any(|n| n.contains("before_window_start_at_99")));
        assert!(names.iter().any(|n| n.contains("after_window_end_at_201")));
        for v in &vectors {
            assert_eq!(v.expected_error_code, POLICY_TIME_WINDOW_VIOLATED);
        }
    }

    #[test]
    fn call_frequency_emits_n_plus_one_vector() {
        let spec = base_spec(vec![generated_slot(
            vec![Constraint::CallFrequency {
                max_calls: 3,
                window_ledgers: 1000,
            }],
            TemplateFamily::CallFrequency,
        )]);
        let rec = sep41_transfer_recording();
        let vectors = generate_deny_vectors(&spec, &rec, 1);
        assert_eq!(vectors.len(), 1);
        assert!(vectors[0]
            .name
            .contains("call_frequency_n_plus_one_calls_in_window"));
        assert_eq!(
            vectors[0].expected_error_code,
            POLICY_CALL_FREQUENCY_EXCEEDED
        );
        assert_eq!(vectors[0].contexts.len(), 4);
    }

    #[test]
    fn sequence_ordering_emits_phase_skipped_vector() {
        let spec = base_spec(vec![generated_slot(
            vec![Constraint::SequenceOrdering {
                phases: vec!["init".to_string(), "claim".to_string()],
            }],
            TemplateFamily::SequenceOrdering,
        )]);
        let rec = sep41_transfer_recording();
        let vectors = generate_deny_vectors(&spec, &rec, 1);
        assert_eq!(vectors.len(), 1);
        assert!(vectors[0].name.contains("sequence_ordering_phase_skipped"));
        assert_eq!(
            vectors[0].expected_error_code,
            POLICY_SEQUENCE_ORDERING_VIOLATED
        );
        // The mutated invocation targets phases[1] when the host state is at
        // phase_index=0.
        assert_eq!(vectors[0].contexts[0].function_name, "claim");
    }

    #[test]
    fn sequence_ordering_with_too_few_phases_emits_loud_todo() {
        let spec = base_spec(vec![generated_slot(
            vec![Constraint::SequenceOrdering {
                phases: vec!["init".to_string()],
            }],
            TemplateFamily::SequenceOrdering,
        )]);
        let rec = sep41_transfer_recording();
        let vectors = generate_deny_vectors(&spec, &rec, 1);
        assert_eq!(vectors.len(), 1);
        assert!(vectors[0].name.starts_with("TODO:"));
        assert_eq!(vectors[0].expected_error_code, 0);
    }

    // ---------------------------------------------------------------
    // Composition + determinism
    // ---------------------------------------------------------------

    #[test]
    fn composition_function_allowlist_plus_amount_range_plus_call_frequency() {
        let spec = base_spec(vec![generated_slot(
            vec![
                Constraint::FunctionAllowlist {
                    functions: vec!["transfer".to_string()],
                },
                Constraint::AmountRange {
                    fn_name: "transfer".to_string(),
                    arg_index: 2,
                    min_string: Some("1".to_string()),
                    max_string: Some("100".to_string()),
                },
                Constraint::CallFrequency {
                    max_calls: 2,
                    window_ledgers: 500,
                },
            ],
            TemplateFamily::FunctionAllowlist,
        )]);
        let rec = sep41_transfer_recording();
        let vectors = generate_deny_vectors(&spec, &rec, 7);
        // FunctionAllowlist=1 + AmountRange=2 (above+below) + CallFrequency=1
        // = 4 vectors total. Per the brief, ≥3 distinct vectors covering each
        // primitive.
        assert!(
            vectors.len() >= 3,
            "expected ≥3 vectors, got {}",
            vectors.len()
        );
        let codes: std::collections::BTreeSet<u32> =
            vectors.iter().map(|v| v.expected_error_code).collect();
        assert!(codes.contains(&POLICY_FUNCTION_NOT_ALLOWED));
        assert!(codes.contains(&POLICY_AMOUNT_OUT_OF_RANGE));
        assert!(codes.contains(&POLICY_CALL_FREQUENCY_EXCEEDED));
    }

    #[test]
    fn generate_deny_vectors_is_byte_equal_for_same_seed() {
        let spec = base_spec(vec![
            spending_limit_slot("1000"),
            generated_slot(
                vec![
                    Constraint::FunctionAllowlist {
                        functions: vec!["transfer".to_string()],
                    },
                    Constraint::AmountRange {
                        fn_name: "transfer".to_string(),
                        arg_index: 2,
                        min_string: Some("1".to_string()),
                        max_string: Some("100".to_string()),
                    },
                    Constraint::TimeWindow {
                        start_ledger: 100,
                        end_ledger: 200,
                    },
                ],
                TemplateFamily::FunctionAllowlist,
            ),
        ]);
        let rec = sep41_transfer_recording();
        let a = generate_deny_vectors(&spec, &rec, 42);
        let b = generate_deny_vectors(&spec, &rec, 42);
        let aj = serde_json::to_string(&a).expect("serialize a");
        let bj = serde_json::to_string(&b).expect("serialize b");
        assert_eq!(
            aj, bj,
            "deterministic generator drifted between two runs with same seed"
        );
    }

    #[test]
    fn generate_deny_vectors_same_seed_same_output_for_strategy_driven_primitive() {
        // Sanity: with a primitive that drives a strategy-backed boundary
        // (AmountRange) two runs with the same seed must still produce the
        // exact same vector. This guards against the strategy plumbing
        // accidentally pulling system entropy.
        let spec = base_spec(vec![generated_slot(
            vec![Constraint::AmountRange {
                fn_name: "transfer".to_string(),
                arg_index: 2,
                min_string: None,
                max_string: Some("100".to_string()),
            }],
            TemplateFamily::AmountRange,
        )]);
        let rec = sep41_transfer_recording();
        let a1 = generate_deny_vectors(&spec, &rec, 1);
        let a2 = generate_deny_vectors(&spec, &rec, 1);
        assert_eq!(
            serde_json::to_string(&a1).unwrap(),
            serde_json::to_string(&a2).unwrap()
        );
    }

    // ---------------------------------------------------------------
    // Proptest-driven sanity checks on the strategy bounds
    // ---------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 50,
            .. ProptestConfig::default()
        })]

        #[test]
        fn arb_amount_above_max_is_strictly_above(
            max in (i128::MIN / 2)..(i128::MAX / 2),
        ) {
            let mut runner = make_runner(0);
            let strategy = arb_amount_above_max(max);
            let tree = strategy.new_tree(&mut runner).unwrap();
            let v = tree.current();
            prop_assert!(v > max, "{v} should be > {max}");
        }

        #[test]
        fn arb_amount_below_min_is_strictly_below(
            min in (i128::MIN / 2)..(i128::MAX / 2),
        ) {
            let mut runner = make_runner(0);
            let strategy = arb_amount_below_min(min);
            let tree = strategy.new_tree(&mut runner).unwrap();
            let v = tree.current();
            prop_assert!(v < min, "{v} should be < {min}");
        }

        #[test]
        fn arb_ledger_after_window_is_strictly_after(end_ledger in 0u32..u32::MAX - 1_000_001) {
            let mut runner = make_runner(0);
            let strategy = arb_ledger_after_window(end_ledger);
            let tree = strategy.new_tree(&mut runner).unwrap();
            let v = tree.current();
            prop_assert!(v > end_ledger, "{v} should be > {end_ledger}");
        }

        #[test]
        fn every_deny_vector_has_sensible_error_code(
            limit in 1i64..1_000_000_000i64,
        ) {
            let spec = base_spec(vec![generated_slot(
                vec![Constraint::AmountRange {
                    fn_name: "transfer".to_string(),
                    arg_index: 2,
                    min_string: Some("0".to_string()),
                    max_string: Some(limit.to_string()),
                }],
                TemplateFamily::AmountRange,
            )]);
            let rec = sep41_transfer_recording();
            let vectors = generate_deny_vectors(&spec, &rec, 1);
            prop_assert!(!vectors.is_empty());
            for v in &vectors {
                // Sensible = non-zero, in OZ's 3xxx range or our 1xxx range.
                let c = v.expected_error_code;
                prop_assert!(
                    (3000..=3299).contains(&c) || (1000..=1099).contains(&c),
                    "code {c} not in OZ 3xxx or generated 1xxx range",
                );
            }
        }
    }
}

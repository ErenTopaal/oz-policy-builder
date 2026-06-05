//! decision-tree synthesizer. pure function: recording -> policyspec.
//! deterministic, no i/o.

use serde::{Deserialize, Serialize};

use crate::errors::Error;
use crate::recording::{AuthFunction, AuthInvocation, ContractRecord, Credentials, Recording};
use crate::sep41::{extract_transfer_amount, is_sep41_transfer};
use crate::spec::{
    ContextRuleSpec, ContextType, ExistingPrimitive, ExistingPrimitiveParams, PolicySlot,
    PolicySpec, RecordingRef, SignerSpec, SynthesisMode, TemplateFamily, MAX_NAME_SIZE,
    MAX_POLICIES, MAX_SIGNERS, POLICY_SCHEMA_URI,
};

/// fallback `period_ledgers` (~30 days at 5s/ledger).
const DEFAULT_SPENDING_LIMIT_PERIOD_LEDGERS: u32 = 432_000;

/// scale factor for observed `i128` amounts.
/// exact=×1, smallmargin=×1.1, loose=×2. uses checked_mul so overflow surfaces
/// as `SynthNotExpressible`, never panics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Tightness {
    Exact,
    SmallMargin,
    Loose,
}

/// caller-supplied options that steer [`synthesize`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SynthesisOptions {
    pub mode: SynthesisMode,
    pub tightness: Tightness,
    pub lifetime_ledgers: Option<u32>,
    pub delegated_signer: Option<String>,
    pub context_rule_name: String,
}

/// synthesise a [`PolicySpec`] from a [`Recording`].
pub fn synthesize(recording: &Recording, opts: &SynthesisOptions) -> Result<PolicySpec, Error> {
    // preflight: name length (utf-8 bytes — matches on-chain MAX_NAME_SIZE unit).
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

    // enumerate distinct Context::Contract targets in insertion order.
    let targets = enumerate_contract_targets(recording);

    // build slots in fixed order: [SpendingLimit?] [SimpleThreshold?] [Generated?]
    let mut policies: Vec<PolicySlot> = Vec::new();
    // PR-#649: promoted to CallContract only when SpendingLimit is composed.
    let mut forced_context_type: Option<ContextType> = None;

    // spendinglimit candidacy: single target + SEP-41 transfer.
    let single_target_sep41 = single_target_sep41_record(recording, &targets);
    if let Some((target_addr, record)) = single_target_sep41 {
        let observed_amount = extract_transfer_amount(record).ok_or_else(|| {
            // shouldn't happen — is_sep41_transfer guaranteed args[2] is I128.
            Error::SynthNotExpressible(
                "internal: SEP-41 transfer matched but amount extraction failed".to_string(),
            )
        })?;
        let scaled = scale_i128_decimal(observed_amount, opts.tightness)?;

        match opts.mode {
            SynthesisMode::Auto | SynthesisMode::ComposeOnly => {
                // PR-#649: spending_limit install rejects ContextType::Default.
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
                // codegenonly: emit a Generated AmountRange slot, still force
                // CallContract since the observed flow is single-SAC.
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
        // multiple targets — no single primitive covers them.
        match opts.mode {
            SynthesisMode::ComposeOnly => {
                return Err(Error::SynthNotExpressible(format!(
                    "multiple contract targets ({}) cannot compose to a single existing primitive under ComposeOnly mode",
                    targets.len()
                )));
            }
            SynthesisMode::Auto | SynthesisMode::CodegenOnly => {
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
    // single non-SEP-41 target: emit Generated FunctionAllowlist (or reject
    // under ComposeOnly — no primitive scopes by function name).
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

    // threshold slot — skipped under CodegenOnly (forces every constraint generated).
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

    // final hard-limit gates.
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

    // stitch the spec together.
    let context_rule = ContextRuleSpec {
        name: opts.context_rule_name.clone(),
        // forced_context_type carries PR-#649; default otherwise.
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

/// distinct strkey `C…` target addresses in insertion order (Vec, not HashSet).
fn enumerate_contract_targets(recording: &Recording) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();

    // explicit invocation list first.
    for cr in &recording.contracts {
        if !out.iter().any(|a| a == &cr.address) {
            out.push(cr.address.clone());
        }
    }
    // then any Contract auth invocations, walked recursively.
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

/// `Some(...)` when there's exactly one target and it's a SEP-41 transfer.
fn single_target_sep41<'a>(
    recording: &'a Recording,
    targets: &'a [String],
) -> Option<(&'a str, &'a ContractRecord)> {
    if targets.len() != 1 {
        return None;
    }
    let target = &targets[0];
    // function/args live on ContractRecord; auth tree only confirms authorisation.
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

/// distinct function names invoked across `targets`, in first-seen order.
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

/// build the `signers` list. honors `opts.delegated_signer` if set, else
/// extracts distinct ed25519 signers from auth tree in insertion order.
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
            // signer stored as strkey; installer converts to Bytes32. emit
            // strkey as `public_key_hex` placeholder here.
            out.push(SignerSpec::ExternalEd25519 {
                public_key_hex: signer.clone(),
            });
        }
    }
    Ok(out)
}

/// recording-ref hash from `IngestSource` (None for simulation).
fn recording_hash(ingest: &crate::recording::IngestSource) -> Option<String> {
    match ingest {
        crate::recording::IngestSource::Hash { hash } => Some(hash.clone()),
        crate::recording::IngestSource::Simulation { .. } => None,
    }
}

/// scale a decimal-string `i128` by tightness. uses checked_mul; overflow
/// surfaces as `SynthNotExpressible`.
fn scale_i128_decimal(observed: &str, tightness: Tightness) -> Result<String, Error> {
    let parsed: i128 = observed.parse().map_err(|_| {
        Error::SynthNotExpressible(format!(
            "observed amount {observed:?} is not a valid i128 decimal"
        ))
    })?;
    let scaled: i128 = match tightness {
        Tightness::Exact => parsed,
        Tightness::SmallMargin => {
            // 1.1× via 11/10 — multiply first to keep precision on small inputs.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arg_value::ArgValue;
    use crate::recording::{
        AuthEntry, AuthFunction, AuthInvocation, AuthTree, ContractRecord, Credentials,
        IngestSource, Recording, RECORDING_SCHEMA_URI,
    };
    use crate::spec::{
        Constraint, ContextType, ExistingPrimitive, ExistingPrimitiveParams, PolicySlot,
        SignerSpec, SynthesisMode, TemplateFamily,
    };

    fn base_recording() -> Recording {
        Recording {
            schema: RECORDING_SCHEMA_URI.to_string(),
            network_passphrase: "Test SDF Network ; September 2015".to_string(),
            ingest: IngestSource::Hash {
                hash: "deadbeef".to_string(),
            },
            ledger: Some(1234),
            contracts: vec![],
            auth_tree: AuthTree { roots: vec![] },
            state_changes: vec![],
            events: vec![],
        }
    }

    fn sep41_transfer_record(target: &str, from: &str, to: &str, amount: &str) -> ContractRecord {
        ContractRecord {
            address: target.to_string(),
            function: "transfer".to_string(),
            args: vec![
                ArgValue::Address(from.to_string()),
                ArgValue::Address(to.to_string()),
                ArgValue::I128(amount.to_string()),
            ],
        }
    }

    fn auth_entry_with_signer(signer: &str, target: &str, fn_name: &str) -> AuthEntry {
        AuthEntry {
            credentials: Credentials::Address {
                signer: signer.to_string(),
                nonce: "1".to_string(),
                signature_expiration_ledger: 0,
                signature: ArgValue::Void,
            },
            root_invocation: AuthInvocation {
                function: AuthFunction::Contract {
                    address: target.to_string(),
                    function: fn_name.to_string(),
                    args: vec![],
                },
                sub_invocations: vec![],
            },
            source_op_index: 0,
        }
    }

    fn opts_compose_only() -> SynthesisOptions {
        SynthesisOptions {
            mode: SynthesisMode::ComposeOnly,
            tightness: Tightness::Exact,
            lifetime_ledgers: Some(432_000),
            delegated_signer: None,
            context_rule_name: "rule".to_string(),
        }
    }

    /// SEP-41 transfer → SpendingLimit + CallContract (PR-#649).
    #[test]
    fn sep41_transfer_emits_spending_limit_with_call_contract() {
        let mut rec = base_recording();
        rec.contracts
            .push(sep41_transfer_record("CUSDC", "GFROM", "GTO", "5000000"));
        rec.auth_tree
            .roots
            .push(auth_entry_with_signer("GSIGNER", "CUSDC", "transfer"));

        let spec = synthesize(&rec, &opts_compose_only()).expect("spec");

        // SpendingLimit first, SimpleThreshold second.
        assert_eq!(spec.policies.len(), 2);
        match &spec.policies[0] {
            PolicySlot::Existing {
                primitive: ExistingPrimitive::SpendingLimit,
                params:
                    ExistingPrimitiveParams::SpendingLimit {
                        period_ledgers,
                        limit_stroops_string,
                    },
            } => {
                assert_eq!(*period_ledgers, 432_000);
                assert_eq!(limit_stroops_string, "5000000");
            }
            other => panic!("expected SpendingLimit, got {other:?}"),
        }
        // PR-#649: context_type must be CallContract { CUSDC }.
        match &spec.context_rule.context_type {
            ContextType::CallContract { address } => assert_eq!(address, "CUSDC"),
            other => panic!("expected CallContract, got {other:?}"),
        }
        match &spec.policies[1] {
            PolicySlot::Existing {
                primitive: ExistingPrimitive::SimpleThreshold,
                params: ExistingPrimitiveParams::SimpleThreshold { threshold },
            } => assert_eq!(*threshold, 1),
            other => panic!("expected SimpleThreshold, got {other:?}"),
        }
    }

    /// multi-contract under Auto → Generated FunctionAllowlist + AssetAllowlist.
    #[test]
    fn multi_contract_emits_generated_function_allowlist() {
        let mut rec = base_recording();
        rec.contracts
            .push(sep41_transfer_record("CUSDC", "GFROM", "GTO", "1"));
        // second distinct target with a non-transfer function.
        rec.contracts.push(ContractRecord {
            address: "CBLEND".to_string(),
            function: "claim".to_string(),
            args: vec![ArgValue::Address("GFROM".to_string())],
        });
        rec.auth_tree
            .roots
            .push(auth_entry_with_signer("GSIGNER", "CUSDC", "transfer"));

        let opts = SynthesisOptions {
            mode: SynthesisMode::Auto,
            tightness: Tightness::Exact,
            lifetime_ledgers: None,
            delegated_signer: None,
            context_rule_name: "rule".to_string(),
        };
        let spec = synthesize(&rec, &opts).expect("spec");
        assert_eq!(spec.policies.len(), 2);
        match &spec.policies[0] {
            PolicySlot::Generated {
                template_family: TemplateFamily::FunctionAllowlist,
                constraints,
            } => {
                // insertion-order preserved: CUSDC->transfer, CBLEND->claim.
                let funcs = constraints
                    .iter()
                    .find_map(|c| match c {
                        Constraint::FunctionAllowlist { functions } => Some(functions.clone()),
                        _ => None,
                    })
                    .expect("FunctionAllowlist constraint");
                assert_eq!(funcs, vec!["transfer".to_string(), "claim".to_string()]);
                let assets = constraints
                    .iter()
                    .find_map(|c| match c {
                        Constraint::AssetAllowlist { assets } => Some(assets.clone()),
                        _ => None,
                    })
                    .expect("AssetAllowlist constraint");
                assert_eq!(assets, vec!["CUSDC".to_string(), "CBLEND".to_string()]);
            }
            other => panic!("expected Generated FunctionAllowlist, got {other:?}"),
        }
        // multi-contract has no SAC-scoped SpendingLimit; context stays Default.
        assert_eq!(spec.context_rule.context_type, ContextType::Default);
    }

    /// smallmargin tightness scales observed amount by 1.1×.
    #[test]
    fn small_margin_scales_observed_amount() {
        let mut rec = base_recording();
        rec.contracts
            .push(sep41_transfer_record("CUSDC", "GFROM", "GTO", "1000"));
        rec.auth_tree
            .roots
            .push(auth_entry_with_signer("GSIGNER", "CUSDC", "transfer"));

        let opts = SynthesisOptions {
            mode: SynthesisMode::Auto,
            tightness: Tightness::SmallMargin,
            lifetime_ledgers: Some(432_000),
            delegated_signer: None,
            context_rule_name: "rule".to_string(),
        };
        let spec = synthesize(&rec, &opts).expect("spec");
        match &spec.policies[0] {
            PolicySlot::Existing {
                primitive: ExistingPrimitive::SpendingLimit,
                params:
                    ExistingPrimitiveParams::SpendingLimit {
                        limit_stroops_string,
                        ..
                    },
            } => assert_eq!(limit_stroops_string, "1100"),
            other => panic!("expected SpendingLimit, got {other:?}"),
        }
    }

    /// codegenonly forces Generated AmountRange even on clean SEP-41 transfer.
    #[test]
    fn codegen_only_mode_forces_generated_for_sep41() {
        let mut rec = base_recording();
        rec.contracts
            .push(sep41_transfer_record("CUSDC", "GFROM", "GTO", "5000000"));
        rec.auth_tree
            .roots
            .push(auth_entry_with_signer("GSIGNER", "CUSDC", "transfer"));

        let opts = SynthesisOptions {
            mode: SynthesisMode::CodegenOnly,
            tightness: Tightness::Exact,
            lifetime_ledgers: Some(432_000),
            delegated_signer: None,
            context_rule_name: "rule".to_string(),
        };
        let spec = synthesize(&rec, &opts).expect("spec");
        assert_eq!(spec.policies.len(), 1);
        match &spec.policies[0] {
            PolicySlot::Generated {
                template_family: TemplateFamily::AmountRange,
                constraints,
            } => {
                assert!(matches!(
                    constraints[0],
                    Constraint::AmountRange {
                        ref fn_name,
                        arg_index: 2,
                        ..
                    } if fn_name == "transfer"
                ));
            }
            other => panic!("expected Generated AmountRange, got {other:?}"),
        }
        // PR-#649 still wins: CallContract even in CodegenOnly.
        match &spec.context_rule.context_type {
            ContextType::CallContract { address } => assert_eq!(address, "CUSDC"),
            other => panic!("expected CallContract, got {other:?}"),
        }
    }

    /// signer count above MAX_SIGNERS → SynthNotExpressible.
    #[test]
    fn signer_count_above_max_signers_errors() {
        let mut rec = base_recording();
        rec.contracts
            .push(sep41_transfer_record("CUSDC", "GFROM", "GTO", "1"));
        for i in 0..=MAX_SIGNERS {
            // 16 distinct signers.
            rec.auth_tree.roots.push(auth_entry_with_signer(
                &format!("GSIGNER{i}"),
                "CUSDC",
                "transfer",
            ));
        }
        let err = synthesize(&rec, &opts_compose_only()).expect_err("expected error");
        match err {
            Error::SynthNotExpressible(msg) => {
                assert!(msg.contains("signers"), "msg should name signers: {msg}");
            }
            other => panic!("expected SynthNotExpressible, got {other:?}"),
        }
    }

    /// policy count above MAX_POLICIES → SynthNotExpressible.
    /// hard to drive above 5 from a real recording, so we lock the gate
    /// invariant + the "policies" keyword in the error message (load-bearing
    /// for MCP UX).
    #[test]
    fn policy_count_above_max_policies_errors() {
        // lock the invariant + error keyword for the future branch.
        let mut rec = base_recording();
        rec.contracts
            .push(sep41_transfer_record("CUSDC", "GFROM", "GTO", "1"));
        rec.auth_tree
            .roots
            .push(auth_entry_with_signer("GSIGNER", "CUSDC", "transfer"));
        let spec = synthesize(&rec, &opts_compose_only()).expect("spec");
        assert!(
            (spec.policies.len() as u32) <= MAX_POLICIES,
            "synthesize must never emit more than MAX_POLICIES slots; got {}",
            spec.policies.len()
        );
        // lock the keyword so future wording drift is loud.
        assert_eq!(MAX_POLICIES, 5);
        let probe = Error::SynthNotExpressible(format!(
            "policies count 6 exceeds MAX_POLICIES ({MAX_POLICIES})"
        ));
        assert!(probe.to_string().contains("policies"));
    }

    /// 21-byte context_rule_name → SynthNotExpressible (MAX_NAME_SIZE=20).
    #[test]
    fn context_rule_name_too_long_errors() {
        let mut rec = base_recording();
        rec.contracts
            .push(sep41_transfer_record("CUSDC", "GFROM", "GTO", "1"));
        rec.auth_tree
            .roots
            .push(auth_entry_with_signer("GSIGNER", "CUSDC", "transfer"));

        let opts = SynthesisOptions {
            mode: SynthesisMode::Auto,
            tightness: Tightness::Exact,
            lifetime_ledgers: None,
            delegated_signer: None,
            context_rule_name: "x".repeat((MAX_NAME_SIZE + 1) as usize), // 21 bytes.
        };
        let err = synthesize(&rec, &opts).expect_err("expected error");
        match err {
            Error::SynthNotExpressible(msg) => {
                assert!(msg.contains("MAX_NAME_SIZE"), "msg: {msg}");
            }
            other => panic!("expected SynthNotExpressible, got {other:?}"),
        }
    }

    /// PR-#649: SpendingLimit always forces CallContract — never Default.
    #[test]
    fn spending_limit_always_forces_call_contract() {
        for tightness in [Tightness::Exact, Tightness::SmallMargin, Tightness::Loose] {
            let mut rec = base_recording();
            rec.contracts
                .push(sep41_transfer_record("CUSDC", "GFROM", "GTO", "100"));
            rec.auth_tree
                .roots
                .push(auth_entry_with_signer("GSIGNER", "CUSDC", "transfer"));
            let opts = SynthesisOptions {
                mode: SynthesisMode::Auto,
                tightness,
                lifetime_ledgers: Some(432_000),
                delegated_signer: None,
                context_rule_name: "rule".to_string(),
            };
            let spec = synthesize(&rec, &opts).expect("spec");
            match &spec.context_rule.context_type {
                ContextType::CallContract { address } => assert_eq!(address, "CUSDC"),
                other => panic!("PR-#649: expected CallContract, got {other:?}"),
            }
        }
    }

    /// loose scaling overflow surfaces as SynthNotExpressible.
    #[test]
    fn loose_scaling_overflow_surfaces_as_error() {
        let mut rec = base_recording();
        rec.contracts.push(ContractRecord {
            address: "CUSDC".to_string(),
            function: "transfer".to_string(),
            args: vec![
                ArgValue::Address("GFROM".to_string()),
                ArgValue::Address("GTO".to_string()),
                // i128::MAX, ×2 overflows.
                ArgValue::I128("170141183460469231731687303715884105727".to_string()),
            ],
        });
        let opts = SynthesisOptions {
            mode: SynthesisMode::Auto,
            tightness: Tightness::Loose,
            lifetime_ledgers: Some(432_000),
            delegated_signer: None,
            context_rule_name: "rule".to_string(),
        };
        let err = synthesize(&rec, &opts).expect_err("expected overflow error");
        match err {
            Error::SynthNotExpressible(msg) => {
                assert!(msg.contains("overflow"), "msg: {msg}");
            }
            other => panic!("expected SynthNotExpressible, got {other:?}"),
        }
    }

    /// delegated_signer overrides observed auth-tree signers.
    #[test]
    fn delegated_signer_overrides_observed_signers() {
        let mut rec = base_recording();
        rec.contracts
            .push(sep41_transfer_record("CUSDC", "GFROM", "GTO", "1"));
        rec.auth_tree
            .roots
            .push(auth_entry_with_signer("GSIGNER1", "CUSDC", "transfer"));
        rec.auth_tree
            .roots
            .push(auth_entry_with_signer("GSIGNER2", "CUSDC", "transfer"));

        let opts = SynthesisOptions {
            mode: SynthesisMode::Auto,
            tightness: Tightness::Exact,
            lifetime_ledgers: Some(432_000),
            delegated_signer: Some("CDELEGATE".to_string()),
            context_rule_name: "rule".to_string(),
        };
        let spec = synthesize(&rec, &opts).expect("spec");
        assert_eq!(spec.signers.len(), 1);
        match &spec.signers[0] {
            SignerSpec::Delegated { address } => assert_eq!(address, "CDELEGATE"),
            other => panic!("expected Delegated, got {other:?}"),
        }
    }

    /// composeonly + multiple targets → SynthNotExpressible.
    #[test]
    fn compose_only_rejects_multi_contract() {
        let mut rec = base_recording();
        rec.contracts
            .push(sep41_transfer_record("CUSDC", "GFROM", "GTO", "1"));
        rec.contracts.push(ContractRecord {
            address: "CBLEND".to_string(),
            function: "claim".to_string(),
            args: vec![],
        });
        rec.auth_tree
            .roots
            .push(auth_entry_with_signer("GSIGNER", "CUSDC", "transfer"));
        let err = synthesize(&rec, &opts_compose_only()).expect_err("expected error");
        match err {
            Error::SynthNotExpressible(msg) => {
                assert!(msg.contains("multiple"), "msg: {msg}");
            }
            other => panic!("expected SynthNotExpressible, got {other:?}"),
        }
    }
}

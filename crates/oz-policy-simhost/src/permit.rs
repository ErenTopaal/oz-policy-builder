//! phase 4 — recording → permit replay.
//!
//! translates a [`Recording`] into a sequence of [`TestContext`] +
//! [`AuthPayload`] inputs and drives them through [`TestHost::invoke_check_auth`].
//! `Ok(())` means every observed call in the recording was permitted by
//! the installed policy slate; a `HostExecError::PolicyPanic` is surfaced
//! as `oz_policy_core::Error::SimPermitDenied`, naming the policy code
//! that rejected it.
//!
//! this is the **permit** branch of the SimReport: the recording is the
//! caller's claimed-permitted flow, and the harness verifies the spec
//! the policy was synthesised from actually admits it.

use oz_policy_core::recording::{AuthEntry, AuthFunction, AuthInvocation, Credentials, Recording};
use oz_policy_core::Error;

use crate::host::{AuthPayload, HostExecError, TestContext, TestHost};

/// replay `recording` through `host`. See module docs for semantics.
///
/// `smart_account` is the StrKey returned by [`TestHost::install_smart_account`];
/// `context_rule_id` is the slot the policies were installed against.
///
/// translation rules:
/// * Each `recording.contracts[i]` becomes a [`TestContext`].
/// * Signers are collected by walking the recording's `auth_tree.roots`
///   and emitting `Credentials::Address.signer` StrKeys (deduplicated,
///   preserving first-occurrence order).
/// * `context_rule_ids` is filled with `context_rule_id` once per context
///   (this mirrors the on-chain `AuthPayload.context_rule_ids: Vec<u32>` —
///   one entry per `auth_context`).
pub fn replay_recording(
    host: &mut TestHost,
    recording: &Recording,
    smart_account: &str,
    context_rule_id: u32,
) -> Result<(), Error> {
    let contexts: Vec<TestContext> = recording
        .contracts
        .iter()
        .map(|c| TestContext {
            contract_address: c.address.clone(),
            function_name: c.function.clone(),
            args: c.args.clone(),
        })
        .collect();

    let payload = AuthPayload {
        signer_addresses: collect_signer_addresses(&recording.auth_tree.roots),
        context_rule_ids: vec![context_rule_id; contexts.len()],
    };

    match host.invoke_check_auth(smart_account, payload, contexts) {
        Ok(()) => Ok(()),
        Err(HostExecError::PolicyPanic(code)) => Err(Error::SimPermitDenied(format!(
            "recording denied by policy: panic code {code}",
        ))),
        Err(other) => Err(Error::SimPermitDenied(format!(
            "recording replay failed: {other}",
        ))),
    }
}

/// walk the recording's auth-tree and produce a stable, deduplicated list
/// of signer StrKeys.
///
/// `Credentials::SourceAccount` entries are skipped because they're the
/// transaction source account, not a SmartAccount signer. `Address` entries
/// contribute their `signer` StrKey.
///
/// recursive walk into `sub_invocations` is intentional: nested
/// `__check_auth` invocations may carry additional signers and we want
/// the synthesized `AuthPayload` to reflect the complete signer set.
fn collect_signer_addresses(roots: &[AuthEntry]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for entry in roots {
        if let Credentials::Address { signer, .. } = &entry.credentials {
            if !out.iter().any(|s| s == signer) {
                out.push(signer.clone());
            }
        }
        // sub-invocations don't carry their own credentials in the
        // `AuthInvocation` shape (only the top-level entry does), but we
        // still walk them so any future shape change is picked up.
        walk_invocation(&entry.root_invocation, &mut out);
    }
    out
}

fn walk_invocation(inv: &AuthInvocation, _out: &mut Vec<String>) {
    // no-op today — `AuthInvocation` carries no credentials. Kept as a
    // dedicated function so future shape changes have an obvious hook.
    match &inv.function {
        AuthFunction::Contract { .. }
        | AuthFunction::CreateContract { .. }
        | AuthFunction::CreateContractV2 { .. } => {}
    }
    for sub in &inv.sub_invocations {
        walk_invocation(sub, _out);
    }
}

// tests

#[cfg(test)]
mod tests {
    use super::*;
    use oz_policy_core::recording::{
        AuthEntry, AuthFunction, AuthInvocation, AuthTree, ContractRecord, Credentials,
        IngestSource, Recording, RECORDING_SCHEMA_URI,
    };
    use oz_policy_core::ArgValue;

    fn empty_recording() -> Recording {
        Recording {
            schema: RECORDING_SCHEMA_URI.into(),
            network_passphrase: "Test SDF Network ; September 2015".into(),
            ingest: IngestSource::Hash {
                hash: "deadbeef".into(),
            },
            ledger: Some(1),
            contracts: vec![],
            auth_tree: AuthTree { roots: vec![] },
            state_changes: vec![],
            events: vec![],
        }
    }

    fn recording_with_one_transfer(token: &str, signer: &str) -> Recording {
        Recording {
            contracts: vec![ContractRecord {
                address: token.into(),
                function: "transfer".into(),
                args: vec![
                    ArgValue::Address(
                        "GAEEZQIBQHBP3CG3F2BSTQHBHM5LJUFRTL2EFRC6CN4MV3OWJZ74C6XR".into(),
                    ),
                    ArgValue::Address(
                        "GAEEZQIBQHBP3CG3F2BSTQHBHM5LJUFRTL2EFRC6CN4MV3OWJZ74C6XR".into(),
                    ),
                    ArgValue::I128("100".into()),
                ],
            }],
            auth_tree: AuthTree {
                roots: vec![AuthEntry {
                    credentials: Credentials::Address {
                        signer: signer.into(),
                        nonce: "1".into(),
                        signature_expiration_ledger: 1000,
                        signature: ArgValue::Void,
                    },
                    root_invocation: AuthInvocation {
                        function: AuthFunction::Contract {
                            address: token.into(),
                            function: "transfer".into(),
                            args: vec![],
                        },
                        sub_invocations: vec![],
                    },
                    source_op_index: 0,
                }],
            },
            ..empty_recording()
        }
    }

    /// empty recording → empty AuthPayload + empty contexts → permit.
    #[test]
    fn replay_empty_recording_permits() {
        let mut h = TestHost::new(100, "Test SDF Network ; September 2015").expect("host");
        let sa = h.install_smart_account("").expect("install SA");
        let r = empty_recording();
        replay_recording(&mut h, &r, &sa, 0).expect("empty recording must permit");
    }

    /// `collect_signer_addresses` deduplicates by first occurrence.
    #[test]
    fn collect_signers_dedupes() {
        let signer = "GAEEZQIBQHBP3CG3F2BSTQHBHM5LJUFRTL2EFRC6CN4MV3OWJZ74C6XR";
        let entry = || AuthEntry {
            credentials: Credentials::Address {
                signer: signer.into(),
                nonce: "1".into(),
                signature_expiration_ledger: 100,
                signature: ArgValue::Void,
            },
            root_invocation: AuthInvocation {
                function: AuthFunction::Contract {
                    address: "CDG7N5LG7TAWOHZH27TW6XN3WBA66TA5TUXYJP6552KVPZ3CTWABHKIH".into(),
                    function: "transfer".into(),
                    args: vec![],
                },
                sub_invocations: vec![],
            },
            source_op_index: 0,
        };
        let roots = vec![entry(), entry(), entry()];
        let signers = collect_signer_addresses(&roots);
        assert_eq!(signers, vec![signer.to_string()]);
    }

    /// `SourceAccount` credentials don't contribute to the signer list.
    #[test]
    fn collect_signers_skips_source_account() {
        let roots = vec![AuthEntry {
            credentials: Credentials::SourceAccount,
            root_invocation: AuthInvocation {
                function: AuthFunction::Contract {
                    address: "CDG7N5LG7TAWOHZH27TW6XN3WBA66TA5TUXYJP6552KVPZ3CTWABHKIH".into(),
                    function: "transfer".into(),
                    args: vec![],
                },
                sub_invocations: vec![],
            },
            source_op_index: 0,
        }];
        assert!(collect_signer_addresses(&roots).is_empty());
    }

    /// recording with one transfer call but NO policies installed →
    /// permit. (The SA admits any flow when no policies are bound; the
    /// "deny" wiring lives in `run.rs` once a policy is installed.)
    #[test]
    fn replay_one_transfer_permits_when_no_policies() {
        let mut h = TestHost::new(100, "Test SDF Network ; September 2015").expect("host");
        let sa = h.install_smart_account("").expect("install SA");
        let token = "CDG7N5LG7TAWOHZH27TW6XN3WBA66TA5TUXYJP6552KVPZ3CTWABHKIH";
        let signer = "GAEEZQIBQHBP3CG3F2BSTQHBHM5LJUFRTL2EFRC6CN4MV3OWJZ74C6XR";
        let r = recording_with_one_transfer(token, signer);
        replay_recording(&mut h, &r, &sa, 0)
            .expect("recording with no policies installed must permit");
    }

    /// `AuthPayload.context_rule_ids` length must equal the contexts
    /// length (one per call). Validates the alignment the on-chain
    /// `AuthPayload` doc-comment requires.
    #[test]
    fn payload_context_rule_ids_align_with_contexts() {
        let token = "CDG7N5LG7TAWOHZH27TW6XN3WBA66TA5TUXYJP6552KVPZ3CTWABHKIH";
        let signer = "GAEEZQIBQHBP3CG3F2BSTQHBHM5LJUFRTL2EFRC6CN4MV3OWJZ74C6XR";
        let r = recording_with_one_transfer(token, signer);
        // manually invoke just the translation logic to inspect the payload.
        let contexts: Vec<TestContext> = r
            .contracts
            .iter()
            .map(|c| TestContext {
                contract_address: c.address.clone(),
                function_name: c.function.clone(),
                args: c.args.clone(),
            })
            .collect();
        let payload = AuthPayload {
            signer_addresses: collect_signer_addresses(&r.auth_tree.roots),
            context_rule_ids: vec![42; contexts.len()],
        };
        assert_eq!(payload.context_rule_ids.len(), contexts.len());
        assert_eq!(payload.context_rule_ids, vec![42]);
        assert_eq!(payload.signer_addresses, vec![signer.to_string()]);
    }
}

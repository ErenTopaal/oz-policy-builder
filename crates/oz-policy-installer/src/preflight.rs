//! pure-logic preflight checks — no i/o. enforces:
//! - PR-#655 account-revision assertion (PrePr655 + Unknown both refused).
//! - on-chain limit constants (`MAX_POLICIES`/`MAX_SIGNERS`/`MAX_NAME_SIZE`).
//! - PR-#649 (`spending_limit` requires `CallContract`).
//! - strkey shape (`C…`/`G…`) with checksum via `stellar-strkey`.
//!
//! ledger-existence + registry-presence checks live in `crate::envelope`.

use oz_policy_core::spec::{ContextType, ExistingPrimitive, PolicySlot, PolicySpec};
use oz_policy_core::Error;

/// caller-asserted statement about the target smart-account contract's
/// release vintage. See module doc-comment for why a user-asserted flag is
/// the only feasible v1 strategy.
///
/// serialised as snake_case (`post_pr_655` / `pre_pr_655` / `unknown`).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AccountRevision {
    /// asserted post-PR-#655 — install proceeds.
    PostPr655,
    /// asserted pre-#655 — hard refusal (sponsor-substitution attack).
    PrePr655,
    /// unknown — hard refusal in v1 (wasm-hash whitelist not wired yet).
    Unknown,
}

/// mirror of on-chain MAX_POLICIES/MAX_SIGNERS/MAX_NAME_SIZE.
const MAX_POLICIES_USIZE: usize = oz_policy_core::spec::MAX_POLICIES as usize;
const MAX_SIGNERS_USIZE: usize = oz_policy_core::spec::MAX_SIGNERS as usize;
const MAX_NAME_SIZE_USIZE: usize = oz_policy_core::spec::MAX_NAME_SIZE as usize;

/// run preflight checks in fixed order. `network_passphrase`/`rpc_url` accepted
/// for surface stability (v1.1 will use them for wasm-hash whitelist).
pub fn check(
    spec: &PolicySpec,
    smart_account: &str,
    source_account: &str,
    network_passphrase: &str,
    rpc_url: &str,
    revision: AccountRevision,
) -> Result<(), Error> {
    // suppress unused-warning for forward-compat parameters; v1.1 will use them.
    let _ = (network_passphrase, rpc_url);

    // 1. account revision gate (PR-#655).
    match revision {
        AccountRevision::PostPr655 => {}
        AccountRevision::PrePr655 => {
            return Err(Error::InstallPreflightFailed(
                "smart account is pre-PR-#655; refusing install \
                 (per docs/oz-internal-shapes.md §8)"
                    .to_string(),
            ));
        }
        AccountRevision::Unknown => {
            return Err(Error::InstallPreflightFailed(
                "account revision unknown; pass --account-revision post-pr-655 to assert, \
                 or run WASM-hash check (TODO v1.1)"
                    .to_string(),
            ));
        }
    }

    // 2. on-chain SmartAccount hard limits.
    if spec.policies.len() > MAX_POLICIES_USIZE {
        return Err(Error::InstallPreflightFailed(format!(
            "MAX_POLICIES ({MAX_POLICIES_USIZE}) exceeded: spec has {} policies",
            spec.policies.len()
        )));
    }
    if spec.signers.len() > MAX_SIGNERS_USIZE {
        return Err(Error::InstallPreflightFailed(format!(
            "MAX_SIGNERS ({MAX_SIGNERS_USIZE}) exceeded: spec has {} signers",
            spec.signers.len()
        )));
    }
    // MAX_NAME_SIZE is a utf-8 byte count; String::len() returns bytes.
    if spec.context_rule.name.len() > MAX_NAME_SIZE_USIZE {
        return Err(Error::InstallPreflightFailed(format!(
            "MAX_NAME_SIZE ({MAX_NAME_SIZE_USIZE}) exceeded for context rule name: \
             {} bytes (UTF-8)",
            spec.context_rule.name.len()
        )));
    }

    // 3. PR-#649: spending_limit requires CallContract.
    if matches!(spec.context_rule.context_type, ContextType::Default) {
        let has_spending_limit = spec.policies.iter().any(|slot| {
            matches!(
                slot,
                PolicySlot::Existing {
                    primitive: ExistingPrimitive::SpendingLimit,
                    ..
                }
            )
        });
        if has_spending_limit {
            return Err(Error::InstallPreflightFailed(
                "PR-#649: spending_limit requires CallContract context_type, not Default"
                    .to_string(),
            ));
        }
    }

    // 4. StrKey shape — checksum-validated via `stellar-strkey`.
    validate_contract_strkey(smart_account).map_err(|reason| {
        Error::InstallPreflightFailed(format!(
            "smart_account is not a valid C-address StrKey: {reason}"
        ))
    })?;
    validate_account_strkey(source_account).map_err(|reason| {
        Error::InstallPreflightFailed(format!(
            "source_account is not a valid G-address StrKey: {reason}"
        ))
    })?;

    Ok(())
}

/// validate `C…` strkey — checksum delegated to stellar-strkey.
fn validate_contract_strkey(s: &str) -> Result<(), String> {
    if s.len() != 56 {
        return Err(format!("expected 56 chars, got {}", s.len()));
    }
    // strkey uses uppercase base32; lowercase 'c' is always wrong.
    if !s.starts_with('C') {
        return Err(format!(
            "expected leading 'C' (uppercase) for contract address, got '{}'",
            s.chars().next().unwrap_or(' ')
        ));
    }
    stellar_strkey::Contract::from_string(s).map_err(|e| format!("strkey decode: {e}"))?;
    Ok(())
}

/// validate `G…` ed25519 strkey.
fn validate_account_strkey(s: &str) -> Result<(), String> {
    if s.len() != 56 {
        return Err(format!("expected 56 chars, got {}", s.len()));
    }
    if !s.starts_with('G') {
        return Err(format!(
            "expected leading 'G' (uppercase) for ed25519 account, got '{}'",
            s.chars().next().unwrap_or(' ')
        ));
    }
    stellar_strkey::ed25519::PublicKey::from_string(s)
        .map_err(|e| format!("strkey decode: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use oz_policy_core::spec::{
        ContextRuleSpec, ContextType, ExistingPrimitive, ExistingPrimitiveParams, PolicySlot,
        PolicySpec, RecordingRef, SignerSpec, SynthesisMode, POLICY_SCHEMA_URI,
    };

    /// known-valid `C…` strkey (testnet USDC SAC).
    const VALID_C: &str = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";

    /// all-zero ed25519 public key with the correct crc16 checksum.
    const VALID_G: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";

    /// baseline-valid spec that preflight should accept.
    fn valid_spec() -> PolicySpec {
        PolicySpec {
            schema: POLICY_SCHEMA_URI.to_string(),
            synthesis_mode: SynthesisMode::Auto,
            context_rule: ContextRuleSpec {
                name: "blend_claim".to_string(),
                context_type: ContextType::Default,
                valid_until: None,
            },
            signers: vec![SignerSpec::ExternalEd25519 {
                public_key_hex: "00".repeat(32),
            }],
            policies: vec![PolicySlot::Existing {
                primitive: ExistingPrimitive::SimpleThreshold,
                params: ExistingPrimitiveParams::SimpleThreshold { threshold: 1 },
            }],
            lifetime_ledgers: None,
            recording_ref: RecordingRef {
                hash: None,
                schema: "oz-recording/v1".to_string(),
            },
        }
    }

    #[test]
    fn pre_pr_655_is_rejected() {
        let spec = valid_spec();
        let err = check(
            &spec,
            VALID_C,
            VALID_G,
            "Test SDF Network ; September 2015",
            "https://soroban-testnet.stellar.org",
            AccountRevision::PrePr655,
        )
        .expect_err("PrePr655 must be a hard refusal");
        assert_eq!(err.code(), "E_INSTALL_PREFLIGHT_FAILED");
        assert!(
            err.to_string().contains("pre-PR-#655"),
            "error message must mention pre-PR-#655: got {err}"
        );
    }

    #[test]
    fn unknown_revision_is_rejected() {
        let spec = valid_spec();
        let err = check(
            &spec,
            VALID_C,
            VALID_G,
            "Test SDF Network ; September 2015",
            "https://soroban-testnet.stellar.org",
            AccountRevision::Unknown,
        )
        .expect_err("Unknown revision must be a hard refusal in v1");
        assert_eq!(err.code(), "E_INSTALL_PREFLIGHT_FAILED");
        assert!(
            err.to_string().contains("account revision unknown"),
            "error message must mention unknown revision: got {err}"
        );
    }

    #[test]
    fn max_policies_exceeded_is_rejected() {
        let mut spec = valid_spec();
        // 6 policies > MAX_POLICIES (5).
        spec.policies = (0..6)
            .map(|_| PolicySlot::Existing {
                primitive: ExistingPrimitive::SimpleThreshold,
                params: ExistingPrimitiveParams::SimpleThreshold { threshold: 1 },
            })
            .collect();
        let err = check(
            &spec,
            VALID_C,
            VALID_G,
            "Test SDF Network ; September 2015",
            "https://soroban-testnet.stellar.org",
            AccountRevision::PostPr655,
        )
        .expect_err("> MAX_POLICIES must fail");
        assert_eq!(err.code(), "E_INSTALL_PREFLIGHT_FAILED");
        assert!(err.to_string().contains("MAX_POLICIES"));
    }

    #[test]
    fn max_signers_exceeded_is_rejected() {
        let mut spec = valid_spec();
        // 16 signers > MAX_SIGNERS (15).
        spec.signers = (0..16)
            .map(|i| SignerSpec::ExternalEd25519 {
                public_key_hex: format!("{:0>64}", i),
            })
            .collect();
        let err = check(
            &spec,
            VALID_C,
            VALID_G,
            "Test SDF Network ; September 2015",
            "https://soroban-testnet.stellar.org",
            AccountRevision::PostPr655,
        )
        .expect_err("> MAX_SIGNERS must fail");
        assert_eq!(err.code(), "E_INSTALL_PREFLIGHT_FAILED");
        assert!(err.to_string().contains("MAX_SIGNERS"));
    }

    #[test]
    fn name_size_overflow_is_rejected() {
        let mut spec = valid_spec();
        // 21 bytes > MAX_NAME_SIZE (20).
        spec.context_rule.name = "a".repeat(21);
        assert_eq!(spec.context_rule.name.len(), 21);
        let err = check(
            &spec,
            VALID_C,
            VALID_G,
            "Test SDF Network ; September 2015",
            "https://soroban-testnet.stellar.org",
            AccountRevision::PostPr655,
        )
        .expect_err("> MAX_NAME_SIZE must fail");
        assert_eq!(err.code(), "E_INSTALL_PREFLIGHT_FAILED");
        assert!(err.to_string().contains("MAX_NAME_SIZE"));
    }

    #[test]
    fn spending_limit_with_default_context_is_rejected() {
        let mut spec = valid_spec();
        spec.context_rule.context_type = ContextType::Default;
        spec.policies = vec![PolicySlot::Existing {
            primitive: ExistingPrimitive::SpendingLimit,
            params: ExistingPrimitiveParams::SpendingLimit {
                period_ledgers: 17_280,
                limit_stroops_string: "10000000".to_string(),
            },
        }];
        let err = check(
            &spec,
            VALID_C,
            VALID_G,
            "Test SDF Network ; September 2015",
            "https://soroban-testnet.stellar.org",
            AccountRevision::PostPr655,
        )
        .expect_err("spending_limit + Default must fail (PR-#649)");
        assert_eq!(err.code(), "E_INSTALL_PREFLIGHT_FAILED");
        assert!(err.to_string().contains("PR-#649"));
        assert!(err.to_string().contains("spending_limit"));
    }

    #[test]
    fn valid_post_pr_655_spec_passes() {
        let spec = valid_spec();
        check(
            &spec,
            VALID_C,
            VALID_G,
            "Test SDF Network ; September 2015",
            "https://soroban-testnet.stellar.org",
            AccountRevision::PostPr655,
        )
        .expect("baseline-valid spec must pass preflight");
    }

    #[test]
    fn lowercase_smart_account_is_rejected() {
        let spec = valid_spec();
        // lowercase 'c' must fail the case-check before strkey decode.
        let bogus_lowercase = format!("c{}", &VALID_C[1..]);
        assert_eq!(bogus_lowercase.len(), 56);
        let err = check(
            &spec,
            &bogus_lowercase,
            VALID_G,
            "Test SDF Network ; September 2015",
            "https://soroban-testnet.stellar.org",
            AccountRevision::PostPr655,
        )
        .expect_err("lowercase 'c' prefix on smart_account must fail");
        assert_eq!(err.code(), "E_INSTALL_PREFLIGHT_FAILED");
        assert!(
            err.to_string().contains("smart_account"),
            "error must blame smart_account: {err}"
        );
    }

    /// sanity: spending_limit + CallContract is allowed (PR-#649 not over-restrictive).
    #[test]
    fn spending_limit_with_call_contract_passes() {
        let mut spec = valid_spec();
        spec.context_rule.context_type = ContextType::CallContract {
            address: VALID_C.to_string(),
        };
        spec.policies = vec![PolicySlot::Existing {
            primitive: ExistingPrimitive::SpendingLimit,
            params: ExistingPrimitiveParams::SpendingLimit {
                period_ledgers: 17_280,
                limit_stroops_string: "10000000".to_string(),
            },
        }];
        check(
            &spec,
            VALID_C,
            VALID_G,
            "Test SDF Network ; September 2015",
            "https://soroban-testnet.stellar.org",
            AccountRevision::PostPr655,
        )
        .expect("spending_limit + CallContract is the documented happy path");
    }
}

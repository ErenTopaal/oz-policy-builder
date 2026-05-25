//! Pure-logic install-time preflight checks.
//!
//! Every check in this module operates on the typed `PolicySpec` IR and the
//! caller-provided StrKey addresses — it performs **no** I/O. Network-level
//! checks (passphrase mismatch, missing source account) live in
//! [`crate::envelope`] where the RPC handle is already in scope. The
//! division-of-labour invariant: anything the synthesizer can determine
//! without a network round-trip belongs here.
//!
//! ## What is enforced
//!
//! 1. **OZ PR-#655 account revision** — per `docs/oz-internal-shapes.md` §8,
//!    no on-chain marker distinguishes pre/post-#655 smart accounts. The
//!    v1 strategy (option 3 in §8) is caller-asserted: the operator passes
//!    `AccountRevision::PostPr655` to certify they have verified their
//!    smart-account WASM came from `stellar-contracts >= v0.7.0-rc.2`.
//!    `PrePr655` is a hard refusal; `Unknown` is also a refusal so a
//!    silent install onto a vulnerable account is impossible.
//! 2. **On-chain `SmartAccount` limit constants** — `MAX_POLICIES = 5`,
//!    `MAX_SIGNERS = 15`, `MAX_NAME_SIZE = 20` (bytes). Mirrored from
//!    `oz-policy-core::spec` (and from `docs/oz-internal-shapes.md` §7).
//!    Catching these here gives the caller an `E_INSTALL_PREFLIGHT_FAILED`
//!    *before* a wallet round-trip, which is strictly more useful than
//!    discovering the contract `panic_with_error!`s mid-install.
//! 3. **PR-#649 (`spending_limit` requires `CallContract`)** — the
//!    on-chain spending-limit `install` rejects `Default`-typed context
//!    rules with `OnlyCallContractAllowed (3227)`. Same reasoning as the
//!    limits: surface it locally with a richer message than a numeric
//!    error code 3227.
//! 4. **StrKey shape** — `smart_account` must be a `C…` contract address
//!    and `source_account` must be a `G…` ed25519 account address. We use
//!    `stellar-strkey` (already a transitive dep via `stellar-rpc-client`)
//!    so the checksum is validated, not just the prefix/length.
//!
//! ## What is **NOT** enforced here (intentionally)
//!
//! * Existence of the `smart_account` ledger entry. That requires an RPC
//!   `getLedgerEntries` call; surfaced in [`crate::envelope`].
//! * Existence of canonical primitive contract addresses. The registry
//!   returns `None` in v1; [`crate::envelope`] surfaces the
//!   `primitive_address_unknown` error.

use oz_policy_core::spec::{ContextType, ExistingPrimitive, PolicySlot, PolicySpec};
use oz_policy_core::Error;

/// Caller-asserted statement about the target smart-account contract's
/// release vintage. See module doc-comment for why a user-asserted flag is
/// the only feasible v1 strategy.
///
/// `Serialize` + `Deserialize` + `JsonSchema` are derived (Phase 5 Stream A)
/// so the MCP `export_policy` tool can accept this discriminator on its
/// JSON input wire and publish a structured schema for it. The wire form
/// is the snake_case variant name (`"post_pr_655"`, `"pre_pr_655"`,
/// `"unknown"`), matching the rest of the policy IR convention.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AccountRevision {
    /// Operator asserts the deployed smart-account WASM came from
    /// `stellar-contracts >= v0.7.0-rc.2` (the first tag containing the
    /// PR-#655 merge commit, per `docs/oz-internal-shapes.md` §8).
    PostPr655,
    /// Operator asserts the deployed smart-account WASM predates PR-#655.
    /// Always a hard refusal — installing onto a vulnerable account would
    /// expose the user to the sponsor-substitution attack PR-#655 fixed.
    PrePr655,
    /// Operator does not know. Also a hard refusal in v1, since the v1.1
    /// WASM-hash whitelist (option 1 in `docs/oz-internal-shapes.md` §8) is
    /// not yet wired up.
    Unknown,
}

/// Hard cap on the number of policies a single context rule may carry —
/// mirrored from `oz_policy_core::spec::MAX_POLICIES` and from
/// `packages/accounts/src/smart_account/mod.rs:524` (`MAX_POLICIES`).
const MAX_POLICIES_USIZE: usize = oz_policy_core::spec::MAX_POLICIES as usize;
const MAX_SIGNERS_USIZE: usize = oz_policy_core::spec::MAX_SIGNERS as usize;
const MAX_NAME_SIZE_USIZE: usize = oz_policy_core::spec::MAX_NAME_SIZE as usize;

/// Run every pure-logic precondition. Returns `Err(Error::InstallPreflightFailed(_))`
/// with a human-readable, machine-stable message on the first failure. The
/// order of checks is fixed (revision → policy/signer/name limits →
/// `spending_limit`/`Default` interaction → StrKey shapes) so deterministic
/// error reporting is easy to test against.
///
/// `network_passphrase` and `rpc_url` are accepted (and currently unused
/// internally) so the surface in `lib.rs` does not change when v1.1 adds
/// the WASM-hash whitelist check (which will reach for the network).
pub fn check(
    spec: &PolicySpec,
    smart_account: &str,
    source_account: &str,
    network_passphrase: &str,
    rpc_url: &str,
    revision: AccountRevision,
) -> Result<(), Error> {
    // Suppress unused-warning for forward-compat parameters; v1.1 will use them.
    let _ = (network_passphrase, rpc_url);

    // 1. Account revision gate (PR-#655).
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

    // 2. OZ SmartAccount hard limits (`packages/accounts/src/smart_account/mod.rs:524-530`).
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
    // Per `docs/oz-internal-shapes.md` §7, `MAX_NAME_SIZE` is a UTF-8 BYTE
    // count, not a character count. `String::len()` already returns bytes
    // in Rust, so this comparison is correct.
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

/// Validate a contract (`C…`) StrKey. Length and prefix are necessary but
/// not sufficient: the StrKey body is a `crc16-xmodem` checksum over the
/// payload, so we delegate to `stellar-strkey` which enforces it.
fn validate_contract_strkey(s: &str) -> Result<(), String> {
    if s.len() != 56 {
        return Err(format!("expected 56 chars, got {}", s.len()));
    }
    // Reject lowercase 'c' or any non-'C' prefix before the strkey decode
    // gives a less-helpful error. StrKey base32 uses uppercase alphabet
    // only — a lowercase prefix is always wrong.
    if !s.starts_with('C') {
        return Err(format!(
            "expected leading 'C' (uppercase) for contract address, got '{}'",
            s.chars().next().unwrap_or(' ')
        ));
    }
    stellar_strkey::Contract::from_string(s).map_err(|e| format!("strkey decode: {e}"))?;
    Ok(())
}

/// Validate an ed25519 account (`G…`) StrKey, same approach as
/// [`validate_contract_strkey`].
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

    /// A real testnet contract address — `CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC`
    /// (USDC SAC on testnet, published in the Stellar testnet asset list).
    /// Used as a known-valid C-address StrKey so the preflight passes its
    /// shape check in the positive test.
    const VALID_C: &str = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";

    /// A real G-address StrKey — the all-zero ed25519 public key with the
    /// correct CRC16 checksum (well-known Stellar test value).
    const VALID_G: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";

    /// Construct a baseline-valid `PolicySpec` that the preflight should accept.
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
        // Lowercase 'c' prefix — the same body as VALID_C but the leading
        // 'C' is replaced with 'c'. This must fail the explicit
        // case-check before strkey decode even runs.
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

    /// Sanity: the spending_limit + CallContract combination IS allowed,
    /// so the PR-#649 check is not over-restrictive.
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

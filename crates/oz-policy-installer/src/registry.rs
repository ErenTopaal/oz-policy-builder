//! network-keyed registry of policy contract addresses.
//! OZ primitives ship as library modules, not standalone contracts — every
//! project deploys their own. `primitive_address` returns None in v1.
//! `project_deployed_policy_address` carries our own track-B deployments.

use oz_policy_core::spec::{ExistingPrimitive, TemplateFamily};

/// stellar testnet passphrase. Mirrored from
/// `https://soroban-testnet.stellar.org`'s `getNetwork` reply.
pub const TESTNET_PASSPHRASE: &str = "Test SDF Network ; September 2015";

/// stellar public-network ("pubnet") passphrase. Mirrored from
/// `https://soroban-rpc.creit.tech`'s `getNetwork` reply.
pub const MAINNET_PASSPHRASE: &str = "Public Global Stellar Network ; September 2015";

/// canonical OZ primitive address by (primitive, network). always None in v1.
pub fn primitive_address(
    primitive: ExistingPrimitive,
    network_passphrase: &str,
) -> Option<&'static str> {
    // no fabricated addresses. Match all variants so that adding a new
    // `ExistingPrimitive` triggers a build error here and forces the
    // implementer to make an explicit "do we have a published address?"
    // decision rather than silently falling through.
    match (primitive, network_passphrase) {
        // testnet: no published canonical deployments. Tracked as v1.1 work.
        (ExistingPrimitive::SimpleThreshold, TESTNET_PASSPHRASE) => None,
        (ExistingPrimitive::WeightedThreshold, TESTNET_PASSPHRASE) => None,
        (ExistingPrimitive::SpendingLimit, TESTNET_PASSPHRASE) => None,
        // mainnet: no published canonical deployments. Tracked as v1.1 work.
        (ExistingPrimitive::SimpleThreshold, MAINNET_PASSPHRASE) => None,
        (ExistingPrimitive::WeightedThreshold, MAINNET_PASSPHRASE) => None,
        (ExistingPrimitive::SpendingLimit, MAINNET_PASSPHRASE) => None,
        // unknown network → unknown address. Same surface; the envelope
        // builder will surface `primitive_address_unknown`.
        _ => None,
    }
}

/// look up the **project-deployed** policy contract address for the
/// generated `template` family on `network_passphrase`. Returns `None`
/// when no deployment is registered; the caller surfaces a typed
/// `Error::InstallPreflightFailed("generated_policy_address_unknown ...")`
/// rather than fabricating an address.
///
/// provenance for every `Some(...)` entry is stated inline as a comment
/// above the match arm: deployer keypair, network, ISO-8601 capture date,
/// and the deploy transaction hash. Rotating an address requires explicit
/// replacement and a CHANGELOG entry (the address is consumed by the
/// phase 7 walkthrough corpus).
///
/// **Per-template scope.** A single deployed contract instance services
/// all `PolicySlot::Generated { template_family: <family>, .. }` slots
/// because the on-chain policy contract is keyed by `(smart_account,
/// context_rule_id)` for all its persistent state (see
/// `walkthroughs/phase3-codegen-fixture/expected/slot_0/source.rs`
/// security invariant §2). Two unrelated installations therefore never
/// collide; one deployed instance per template family per network is
/// sufficient.
pub fn project_deployed_policy_address(
    template: TemplateFamily,
    network_passphrase: &str,
) -> Option<&'static str> {
    // match every (family, network) explicitly so that adding a new
    // template family or a new network triggers a build error and forces
    // an explicit "have we deployed for this pair yet?" decision.
    match (template, network_passphrase) {
        // functionAllowlist on testnet.
        //
        // deployer        : sa-owner-p7r2 (G-key
        //                   GCM2CB7P7ZL4QCCI62WIOCLFW2LT5AP7HPUQY7J6JQQUQT4XXZZNWHLJ)
        // network         : Test SDF Network ; September 2015
        // captured        : 2026-05-16 (Phase 7 Round 2)
        // WASM source     : walkthroughs/phase3-codegen-fixture/expected/
        //                   slot_0/policy.wasm
        // WASM SHA-256    : cb2a8736040711ff831346b20912fc1fe54a9bc096f9dab288014940d72b6fd4
        // upload tx hash  : c4b25d3db81d024f5903e19532a719b0d4367c6a844c6ce4f4bbb26f086b4f97
        // deploy  tx hash : 89ebf13d40ee25c071afb9505fec21042fedee61fbd6ef2280f94e1535991e59
        (TemplateFamily::FunctionAllowlist, TESTNET_PASSPHRASE) => {
            Some("CDBE67MNNVIOAD5RSKO6IECOGIVK45L3NRP4PS2DMCI3GPDYOLY7CWAR")
        }
        // no mainnet deployments — see Phase 10 (mainnet canary).
        (TemplateFamily::FunctionAllowlist, MAINNET_PASSPHRASE) => None,

        // other template families have no deployment yet; tracked as
        // phase 8 walkthrough work.
        (TemplateFamily::ArgumentPattern, _) => None,
        (TemplateFamily::AmountRange, _) => None,
        (TemplateFamily::AssetAllowlist, _) => None,
        (TemplateFamily::TimeWindow, _) => None,
        (TemplateFamily::CallFrequency, _) => None,
        (TemplateFamily::SequenceOrdering, _) => None,
        (TemplateFamily::FunctionAllowlist, _) => None,
    }
}

/// project-deployed smart-account C-address for the Phase 7 Round 2
/// testnet end-to-end integration test. Held here (rather than only in the
/// walkthrough corpus JSON) so Rust call-sites can reference it without
/// reading the file at test time. Provenance comment mirrors the policy
/// registry above.
///
/// deployer        : sa-owner-p7r2
///                   (G GCM2CB7P7ZL4QCCI62WIOCLFW2LT5AP7HPUQY7J6JQQUQT4XXZZNWHLJ)
/// network         : Test SDF Network ; September 2015
/// captured        : 2026-05-16
/// WASM            : crates/oz-policy-simhost/vendor/oz-minimal-smart-account-v0.7.1.wasm
/// WASM SHA-256    : 4b855eb5d4be538753d6b99fe570b5b25b8e064123229dc899edf050788d4a7a
/// upload tx hash  : 942cfa84ccbcc902ad6d999d419dd8e535416e1561eefcfa352ed9daa817cebb
/// deploy  tx hash : 2838989b1ef52a69cb553bd9a7599d22bbce8a8cbff5501c66e364235c6f325a
pub const TESTNET_PHASE7_SMART_ACCOUNT: &str =
    "CAQGYWVEZIE6ZZBVDIVUYTH4BBC5UVQMUOPAKYKDU2POXISSNFKCBN3A";

/// G-address of the keypair that owns [`TESTNET_PHASE7_SMART_ACCOUNT`] —
/// registered as a `Delegated` signer in the SA's bootstrap context rule
/// (ID 0, name "rule"). Held here so the integration test does not have to
/// parse the corpus JSON to find the source / fee-payer account.
///
/// SECURITY: the *secret* seed is held only by the test harness and the
/// walkthrough README (`walkthroughs/phase7-testnet-install/README.md`).
/// never paste it into source.
pub const TESTNET_PHASE7_SA_OWNER_G: &str =
    "GCM2CB7P7ZL4QCCI62WIOCLFW2LT5AP7HPUQY7J6JQQUQT4XXZZNWHLJ";

/// bootstrap-rule context_rule_id assigned by the SA's `init()` call.
/// `init()` always creates exactly one Default rule with id `0`; the
/// phase 7 integration test references it when building the *outer*
/// `add_context_rule` auth tree (the call is authorised under rule 0's
/// `Delegated` signer).
pub const TESTNET_PHASE7_BOOTSTRAP_RULE_ID: u32 = 0;

#[cfg(test)]
mod tests {
    use super::*;

    /// until v1.1 every OZ-primitive lookup returns `None`. This test
    /// exists so a future contributor who pastes a canonical address but
    /// forgets to remove the `None` placeholder is caught immediately —
    /// and so the documented "no fabricated addresses" invariant has a
    /// binary check.
    #[test]
    fn no_published_addresses_in_v1() {
        for primitive in [
            ExistingPrimitive::SimpleThreshold,
            ExistingPrimitive::WeightedThreshold,
            ExistingPrimitive::SpendingLimit,
        ] {
            for net in [TESTNET_PASSPHRASE, MAINNET_PASSPHRASE] {
                assert_eq!(
                    primitive_address(primitive.clone(), net),
                    None,
                    "registry must return None for {primitive:?} on {net}; \
                     paste a verifiable published source in the doc-comment \
                     before adding a Some(...) entry"
                );
            }
        }
    }

    #[test]
    fn unknown_network_returns_none() {
        assert_eq!(
            primitive_address(ExistingPrimitive::SimpleThreshold, "futurenet-or-bogus"),
            None
        );
    }

    /// the FunctionAllowlist family has a real testnet deployment captured
    /// in 2026-05-16 (see provenance comment). This test pins the address
    /// so any accidental rotation surfaces as a test failure rather than a
    /// silent integration-test break.
    #[test]
    fn function_allowlist_testnet_address_is_pinned() {
        assert_eq!(
            project_deployed_policy_address(TemplateFamily::FunctionAllowlist, TESTNET_PASSPHRASE),
            Some("CDBE67MNNVIOAD5RSKO6IECOGIVK45L3NRP4PS2DMCI3GPDYOLY7CWAR"),
        );
    }

    /// no mainnet deployments exist yet — confirm the registry stays honest
    /// (i.e., a contributor cannot paste a mainnet address without also
    /// updating this test).
    #[test]
    fn no_mainnet_deployed_policies() {
        for family in [
            TemplateFamily::FunctionAllowlist,
            TemplateFamily::ArgumentPattern,
            TemplateFamily::AmountRange,
            TemplateFamily::AssetAllowlist,
            TemplateFamily::TimeWindow,
            TemplateFamily::CallFrequency,
            TemplateFamily::SequenceOrdering,
        ] {
            assert_eq!(
                project_deployed_policy_address(family.clone(), MAINNET_PASSPHRASE),
                None,
                "no mainnet deployment expected for {family:?}; \
                 see Phase 10 (mainnet canary) before adding one"
            );
        }
    }

    /// confirm that the smart-account / owner / bootstrap-rule constants
    /// have the expected StrKey prefixes — same defence-in-depth as the
    /// address-pinning test above. A typo in any of these would manifest
    /// at integration-test time as an opaque RPC error; this fails fast.
    #[test]
    fn phase7_constants_have_expected_strkey_prefixes() {
        assert!(
            TESTNET_PHASE7_SMART_ACCOUNT.starts_with('C')
                && TESTNET_PHASE7_SMART_ACCOUNT.len() == 56,
            "TESTNET_PHASE7_SMART_ACCOUNT must be a 56-char C-strkey"
        );
        assert!(
            TESTNET_PHASE7_SA_OWNER_G.starts_with('G') && TESTNET_PHASE7_SA_OWNER_G.len() == 56,
            "TESTNET_PHASE7_SA_OWNER_G must be a 56-char G-strkey"
        );
        assert_eq!(TESTNET_PHASE7_BOOTSTRAP_RULE_ID, 0);
    }
}

//! Network-keyed registry of OZ primitive policy contract addresses.
//!
//! ## Honest finding (v1)
//!
//! The OZ "primitives" (`simple_threshold`, `weighted_threshold`,
//! `spending_limit`) shipped in `stellar-accounts = 0.7.1` are **library
//! modules**, not standalone deployed contracts. They live under
//! `packages/accounts/src/policies/*` and are intended to be compiled into
//! the user's own policy-contract crate (the canonical reference is the
//! `examples/multisig-smart-account/` example). The crate exposes neither a
//! `pub const SIMPLE_THRESHOLD_TESTNET: &str = "C..."` nor any other
//! published deployment address, because no shared deployment exists by
//! design — every project deploys its own instance.
//!
//! Source-of-truth checks (2026-05-15):
//! * `grep -rE "C[A-Z0-9]{55}" stellar-accounts-0.7.1/` → zero matches.
//! * `stellar-accounts-0.7.1/README.md` documents the library/example
//!   pattern with no canonical addresses.
//! * `github.com/OpenZeppelin/stellar-contracts/releases/tag/v0.7.1` ships
//!   crate artifacts only; the example accounts under
//!   `examples/multisig-smart-account/` instruct the user to
//!   `stellar contract deploy` their own.
//!
//! ## Consequence for `build_install_envelope`
//!
//! Until v1.1, this registry returns `None` for every
//! `(primitive, network)` pair. The envelope builder surfaces
//! `Error::InstallPreflightFailed("primitive_address_unknown ...")`
//! rather than fabricating an address. The caller is expected to provide
//! the contract address out-of-band in a future revision of this crate
//! (e.g., a `--primitive-address simple_threshold=C...` CLI flag, or a
//! per-project `policy-builder.toml` mapping).
//!
//! When OZ or a community-curated source publishes canonical addresses for
//! a network (e.g., a shared testnet deployment used by the walkthroughs),
//! they are inserted here as `Some("C...")` keyed by network passphrase.
//! Do **not** insert addresses without a published, verifiable source.

use oz_policy_core::spec::ExistingPrimitive;

/// Stellar testnet passphrase. Mirrored from
/// `https://soroban-testnet.stellar.org`'s `getNetwork` reply.
pub const TESTNET_PASSPHRASE: &str = "Test SDF Network ; September 2015";

/// Stellar public-network ("pubnet") passphrase. Mirrored from
/// `https://soroban-rpc.creit.tech`'s `getNetwork` reply.
pub const MAINNET_PASSPHRASE: &str = "Public Global Stellar Network ; September 2015";

/// Look up the canonical OZ primitive contract address for `primitive` on
/// the network identified by `network_passphrase`. Returns `None` when no
/// canonical published address exists; the caller (typically
/// [`crate::envelope::build_install_envelope`]) is responsible for
/// surfacing a typed `Error::InstallPreflightFailed` in that case.
///
/// **Stability:** This function is intentionally `None` for every
/// (primitive, network) pair in v1 — see the module-level doc-comment.
/// Once a curated address corpus exists, entries land here keyed by
/// passphrase. Callers must not branch on the network passphrase
/// directly; they must go through this function so the address sourcing
/// stays auditable in one place.
pub fn primitive_address(
    primitive: ExistingPrimitive,
    network_passphrase: &str,
) -> Option<&'static str> {
    // No fabricated addresses. Match all variants so that adding a new
    // `ExistingPrimitive` triggers a build error here and forces the
    // implementer to make an explicit "do we have a published address?"
    // decision rather than silently falling through.
    match (primitive, network_passphrase) {
        // Testnet: no published canonical deployments. Tracked as v1.1 work.
        (ExistingPrimitive::SimpleThreshold, TESTNET_PASSPHRASE) => None,
        (ExistingPrimitive::WeightedThreshold, TESTNET_PASSPHRASE) => None,
        (ExistingPrimitive::SpendingLimit, TESTNET_PASSPHRASE) => None,
        // Mainnet: no published canonical deployments. Tracked as v1.1 work.
        (ExistingPrimitive::SimpleThreshold, MAINNET_PASSPHRASE) => None,
        (ExistingPrimitive::WeightedThreshold, MAINNET_PASSPHRASE) => None,
        (ExistingPrimitive::SpendingLimit, MAINNET_PASSPHRASE) => None,
        // Unknown network → unknown address. Same surface; the envelope
        // builder will surface `primitive_address_unknown`.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Until v1.1 every lookup returns `None`. This test exists so a
    /// future contributor who pastes a canonical address but forgets to
    /// remove the `None` placeholder is caught immediately — and so the
    /// documented "no fabricated addresses" invariant has a binary check.
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
}

//! Phase 1 placeholder. Real install-envelope construction lands in Phase 2.

/// Returns the canonical SmartAccount host-function names this crate will
/// invoke when constructing the install envelope. Both names are stable per
/// `docs/oz-internal-shapes.md` §SmartAccount.
pub fn smart_account_install_fns() -> [&'static str; 2] {
    ["add_context_rule", "add_policy"]
}

#[cfg(test)]
mod tests {
    use super::smart_account_install_fns;
    use oz_policy_core::Error;

    #[test]
    fn install_fns_are_stable_and_ordered() {
        assert_eq!(
            smart_account_install_fns(),
            ["add_context_rule", "add_policy"]
        );
    }

    #[test]
    fn preflight_error_maps_to_canonical_code() {
        let err = Error::InstallPreflightFailed("placeholder".into());
        assert_eq!(err.code(), "E_INSTALL_PREFLIGHT_FAILED");
    }
}

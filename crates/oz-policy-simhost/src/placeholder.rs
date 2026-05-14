//! Phase 1 placeholder. Real simulation harness lands in Phase 4.

/// Returns the canonical name of the `enforce` entrypoint that all generated
/// Soroban policy contracts expose. Used as a stable string anchor for
/// downstream phases.
pub fn enforce_entrypoint() -> &'static str {
    "enforce"
}

#[cfg(test)]
mod tests {
    use super::enforce_entrypoint;
    use oz_policy_core::Error;

    #[test]
    fn enforce_entrypoint_is_stable() {
        assert_eq!(enforce_entrypoint(), "enforce");
    }

    #[test]
    fn sim_error_variants_map_to_canonical_codes() {
        assert_eq!(
            Error::SimPermitDenied("placeholder".into()).code(),
            "E_SIM_PERMIT_DENIED"
        );
        assert_eq!(
            Error::SimDenyPassed("placeholder".into()).code(),
            "E_SIM_DENY_PASSED"
        );
        assert_eq!(
            Error::VerifyDrift("placeholder".into()).code(),
            "E_VERIFY_DRIFT"
        );
    }
}

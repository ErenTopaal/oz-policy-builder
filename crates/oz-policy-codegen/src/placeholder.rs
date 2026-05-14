//! Phase 1 placeholder. Track-B codegen implementation lands in Phase 3.

/// Symbolic name of the Track-B output crate template family. Returned
/// verbatim so we can wire the placeholder test to a real string.
pub fn template_family_root() -> &'static str {
    "oz-policy-template"
}

#[cfg(test)]
mod tests {
    use super::template_family_root;
    use oz_policy_core::Error;

    #[test]
    fn template_family_root_is_stable() {
        assert_eq!(template_family_root(), "oz-policy-template");
    }

    #[test]
    fn codegen_compile_failure_maps_to_canonical_code() {
        let err = Error::CodegenCompileFailed("placeholder".into());
        assert_eq!(err.code(), "E_CODEGEN_COMPILE_FAILED");
    }
}

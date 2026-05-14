//! Phase 1 placeholder. The recorder implementation lands in P1-T3.

/// Marker function used by the placeholder unit test to verify the crate is
/// wired into the workspace.
pub fn schema_uri() -> &'static str {
    "oz-policy-builder/recording/v1"
}

#[cfg(test)]
mod tests {
    use super::schema_uri;
    use oz_policy_core::Error;

    #[test]
    fn schema_uri_is_recording_v1() {
        assert_eq!(schema_uri(), "oz-policy-builder/recording/v1");
    }

    /// Confirms this crate can construct the canonical recorder error codes
    /// from `oz_policy_core`. P1-T3 will replace the placeholder bodies with
    /// real RPC plumbing that returns these variants on real failure paths.
    #[test]
    fn recorder_errors_round_trip_via_core() {
        let hash_err = Error::RecorderHashNotFound("placeholder".into());
        let sim_err = Error::RecorderSimFailed("placeholder".into());
        assert_eq!(hash_err.code(), "E_RECORDER_HASH_NOT_FOUND");
        assert_eq!(sim_err.code(), "E_RECORDER_SIM_FAILED");
    }
}

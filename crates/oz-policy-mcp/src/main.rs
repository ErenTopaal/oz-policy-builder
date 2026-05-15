//! Phase 1 placeholder entrypoint for the `oz-policy-mcp` binary.
//!
//! The full MCP server (rmcp 1.7.0, STDIO + Streamable HTTP transports,
//! five tools, resources, prompts) lands in Phase 5. See `plan.md` § "Phase 5
//! — Full MCP server surface".

fn main() {
    println!("oz-policy-mcp placeholder; see plan.md Phase 5 for the real entrypoint");
}

#[cfg(test)]
mod tests {
    use oz_policy_core::Error;

    /// Confirms the MCP binary actually links against `oz_policy_core` —
    /// Phase 5 will surface these `E_*` codes verbatim over the MCP wire,
    /// so the cross-crate boundary needs to be exercised from Phase 1 to
    /// match the pattern used by the other placeholder crates.
    #[test]
    fn mcp_can_round_trip_canonical_error_code() {
        let e = Error::WalletRejected("user clicked cancel".into());
        assert_eq!(e.code(), "E_WALLET_REJECTED");
    }
}

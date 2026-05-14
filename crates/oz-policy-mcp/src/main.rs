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
    #[test]
    fn arithmetic_still_works() {
        // Trivial smoke test gating Phase-1 binary scaffolding; replaced
        // with real MCP handshake tests in Phase 5.
        assert_eq!(2 + 2, 4);
    }
}

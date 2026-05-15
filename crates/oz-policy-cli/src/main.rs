//! Thin CLI mirror of the MCP surface. Phase-1 scope: a `record` subcommand
//! stub that prints a placeholder string. The real recorder wiring lands in
//! P1-T3 (see `plan.md` § "Phase 1 — Foundations").

use clap::{Parser, Subcommand};
use oz_policy_core::Error;

#[derive(Debug, Parser)]
#[command(
    name = "oz-policy-cli",
    about = "OZ Accounts Policy Builder CLI (phase 1 placeholder)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Record a Stellar transaction into a deterministic Recording document.
    /// Phase-1 scope: prints a placeholder; full wiring lands in P1-T3.
    Record,
}

/// Phase-1 placeholder handler. Returns `Result<_, Error>` so the
/// cross-crate boundary with `oz-policy-core` is wired and link-tested from
/// the start; P1-T3 will replace the `Ok` body with real recorder output and
/// the `Err` variants will be the canonical `E_RECORDER_*` codes.
fn record_placeholder() -> Result<&'static str, Error> {
    Ok("phase 1 placeholder")
}

fn run(cli: Cli) -> Result<&'static str, Error> {
    match cli.command {
        Command::Record => record_placeholder(),
    }
}

fn main() {
    let cli = Cli::parse();
    match run(cli) {
        Ok(msg) => println!("{}", msg),
        Err(e) => {
            eprintln!("{}: {}", e.code(), e);
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{run, Cli, Command};
    use clap::CommandFactory;

    #[test]
    fn clap_definition_is_valid() {
        // `debug_assert` runs the validity checks clap exposes; this catches
        // typos in argument names that would otherwise only surface at first
        // invocation.
        Cli::command().debug_assert();
    }

    #[test]
    fn record_subcommand_returns_placeholder_string() {
        let cli = Cli {
            command: Command::Record,
        };
        assert_eq!(run(cli).unwrap(), "phase 1 placeholder");
    }
}

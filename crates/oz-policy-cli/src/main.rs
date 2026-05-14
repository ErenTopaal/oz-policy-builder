//! Thin CLI mirror of the MCP surface. Phase-1 scope: a `record` subcommand
//! stub that prints a placeholder string. The real recorder wiring lands in
//! P1-T3 (see `plan.md` § "Phase 1 — Foundations").

use clap::{Parser, Subcommand};

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

fn run(cli: Cli) -> &'static str {
    match cli.command {
        Command::Record => "phase 1 placeholder",
    }
}

fn main() {
    let cli = Cli::parse();
    println!("{}", run(cli));
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
        assert_eq!(run(cli), "phase 1 placeholder");
    }
}

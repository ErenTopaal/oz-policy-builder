//! Thin CLI mirror of the MCP surface. P1-T3: the `record` subcommand calls
//! `oz-policy-recorder` and prints the `Recording` as pretty-printed JSON to
//! stdout, exiting with a distinct non-zero status per `E_*` error code on
//! failure.

use clap::{ArgGroup, Args, Parser, Subcommand};
use oz_policy_core::Error;
use oz_policy_recorder::Recording;

const DEFAULT_TESTNET_RPC: &str = "https://soroban-testnet.stellar.org";
const DEFAULT_TESTNET_NETWORK: &str = "Test SDF Network ; September 2015";

#[derive(Debug, Parser)]
#[command(
    name = "oz-policy-cli",
    about = "OZ Accounts Policy Builder CLI — phase 1: `record` subcommand."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Record a Stellar Soroban transaction (by on-chain hash or by simulating
    /// a base64 envelope XDR) into a deterministic `Recording` JSON document.
    Record(RecordArgs),
}

/// Mutually exclusive: exactly one of `--hash` / `--envelope-xdr` is required.
#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("source")
        .required(true)
        .args(["hash", "envelope_xdr"])
        .multiple(false)
))]
struct RecordArgs {
    /// On-chain transaction hash (hex). Fetched via `getTransaction`.
    #[arg(long)]
    hash: Option<String>,

    /// Base64-encoded `TransactionEnvelope` XDR. Sent to
    /// `simulateTransaction`; not submitted on chain.
    #[arg(long = "envelope-xdr")]
    envelope_xdr: Option<String>,

    /// Soroban RPC endpoint. Defaults to the public testnet RPC.
    #[arg(long, default_value = DEFAULT_TESTNET_RPC)]
    rpc: String,

    /// Stellar network passphrase. Defaults to testnet.
    #[arg(long, default_value = DEFAULT_TESTNET_NETWORK)]
    network: String,

    /// Soroban `simulateTransaction` resource budget leeway. Only consulted
    /// on the `--envelope-xdr` path. (No-op for the current stable RPC
    /// client API; preserved here so the CLI surface stays stable when the
    /// upstream `resourceConfig` arg lands.)
    #[arg(long = "instruction-leeway")]
    instruction_leeway: Option<u64>,
}

/// Decide the process exit code from an `Error`. Distinct codes per `E_*`
/// variant so CI / wrappers can branch on the failure mode.
///
/// | E_* code                         | exit code |
/// |----------------------------------|-----------|
/// | (success)                        | 0         |
/// | (clap validation error)          | 2         |
/// | E_RECORDER_HASH_NOT_FOUND        | 10        |
/// | E_RECORDER_SIM_FAILED            | 11        |
/// | E_RECORDER_XDR_DECODE_FAILED     | 12        |
/// | (any other E_*)                  | 20        |
fn exit_code_for(e: &Error) -> i32 {
    match e {
        Error::RecorderHashNotFound(_) => 10,
        Error::RecorderSimFailed(_) => 11,
        Error::RecorderXdrDecodeFailed(_) => 12,
        _ => 20,
    }
}

async fn run_record(args: RecordArgs) -> Result<Recording, Error> {
    if let Some(hash) = args.hash {
        oz_policy_recorder::record_by_hash(&args.rpc, &args.network, &hash).await
    } else if let Some(env) = args.envelope_xdr {
        oz_policy_recorder::record_by_simulation(
            &args.rpc,
            &args.network,
            &env,
            args.instruction_leeway,
        )
        .await
    } else {
        // Unreachable: clap's `ArgGroup` enforces exactly-one above.
        Err(Error::RecorderSimFailed(
            "--hash or --envelope-xdr is required (clap arg-group failed?)".into(),
        ))
    }
}

fn main() {
    // Initialise the global `tracing` subscriber so the recorder's
    // `tracing::{info,debug,warn}!` calls reach stderr. Filter is driven by
    // `RUST_LOG` (e.g. `RUST_LOG=oz_policy_recorder=debug`); defaults to
    // `info` when the env var is absent or malformed.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("E_CLI_RUNTIME_FAILED: {e}");
            std::process::exit(30);
        }
    };
    let result = rt.block_on(async {
        match cli.command {
            Command::Record(args) => run_record(args).await,
        }
    });
    match result {
        Ok(rec) => match serde_json::to_string_pretty(&rec) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("E_CLI_SERIALIZE_FAILED: {e}");
                std::process::exit(31);
            }
        },
        Err(e) => {
            eprintln!("{}: {}", e.code(), e);
            std::process::exit(exit_code_for(&e));
        }
    }
}

// -------------------------------------------------------------------------
// Tests — clap-only, no network calls.
// -------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn clap_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn record_subcommand_accepts_hash_form() {
        let cli = Cli::try_parse_from([
            "oz-policy-cli",
            "record",
            "--hash",
            "abc",
            "--rpc",
            "http://x",
            "--network",
            "Test",
        ])
        .expect("parse --hash form");
        match cli.command {
            Command::Record(args) => {
                assert_eq!(args.hash.as_deref(), Some("abc"));
                assert_eq!(args.envelope_xdr, None);
                assert_eq!(args.rpc, "http://x");
                assert_eq!(args.network, "Test");
            }
        }
    }

    #[test]
    fn record_subcommand_accepts_envelope_form() {
        let cli = Cli::try_parse_from([
            "oz-policy-cli",
            "record",
            "--envelope-xdr",
            "AAAA",
            "--rpc",
            "http://y",
            "--network",
            "Test",
            "--instruction-leeway",
            "1000",
        ])
        .expect("parse --envelope-xdr form");
        match cli.command {
            Command::Record(args) => {
                assert_eq!(args.envelope_xdr.as_deref(), Some("AAAA"));
                assert_eq!(args.hash, None);
                assert_eq!(args.instruction_leeway, Some(1000));
            }
        }
    }

    #[test]
    fn record_subcommand_defaults_rpc_and_network_to_testnet() {
        let cli = Cli::try_parse_from(["oz-policy-cli", "record", "--hash", "abc"])
            .expect("parse without --rpc/--network falls back to defaults");
        let Command::Record(args) = cli.command;
        assert_eq!(args.rpc, DEFAULT_TESTNET_RPC);
        assert_eq!(args.network, DEFAULT_TESTNET_NETWORK);
    }

    #[test]
    fn record_subcommand_rejects_both_hash_and_envelope() {
        let err = Cli::try_parse_from([
            "oz-policy-cli",
            "record",
            "--hash",
            "abc",
            "--envelope-xdr",
            "AAAA",
        ])
        .expect_err("clap must reject providing both --hash and --envelope-xdr");
        // ArgGroup conflict is reported as ArgumentConflict.
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::ArgumentConflict,
            "expected ArgumentConflict, got {:?}",
            err.kind()
        );
    }

    #[test]
    fn record_subcommand_rejects_neither_hash_nor_envelope() {
        let err = Cli::try_parse_from(["oz-policy-cli", "record"])
            .expect_err("clap must reject when neither --hash nor --envelope-xdr is provided");
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::MissingRequiredArgument,
            "expected MissingRequiredArgument, got {:?}",
            err.kind()
        );
    }

    #[test]
    fn exit_code_mapping_is_distinct_per_recorder_variant() {
        assert_eq!(exit_code_for(&Error::RecorderHashNotFound("h".into())), 10);
        assert_eq!(exit_code_for(&Error::RecorderSimFailed("s".into())), 11);
        assert_eq!(
            exit_code_for(&Error::RecorderXdrDecodeFailed("x".into())),
            12
        );
        assert_eq!(exit_code_for(&Error::VerifyDrift("d".into())), 20);
    }
}

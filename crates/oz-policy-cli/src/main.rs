//! Thin CLI mirror of the MCP surface.
//!
//! Subcommands:
//! * `record` — fetch a Soroban transaction (by on-chain hash or simulation
//!   envelope) and emit a deterministic [`Recording`] JSON document.
//! * `synthesize` — read a `Recording` JSON, run the Phase 2 decision tree,
//!   and emit a [`PolicySpec`] JSON.
//! * `prepare-install` — read a `PolicySpec` JSON, call
//!   `oz_policy_installer::build_install_envelope`, and emit the resulting
//!   [`EnvelopeArtifact`] (base64 XDR + diagnostics) JSON.
//! * `codegen` — read a `PolicySpec` JSON, run Phase 3 Track-B codegen for
//!   every `Generated` slot, and write `source.rs`, `policy.wasm`, and
//!   `wasm_hash.txt` per slot under `--out`.
//!
//! All subcommands print the result pretty-printed to stdout on success, or
//! `E_*` + detail to stderr (exit non-zero) on failure. The exit code maps
//! deterministically from the `Error` variant — see [`exit_code_for`].

use clap::{ArgGroup, Args, Parser, Subcommand, ValueEnum};
use oz_policy_core::decision_tree::SynthesisOptions;
use oz_policy_core::spec::{PolicySpec, SynthesisMode};
use oz_policy_core::{Error, Tightness};
use oz_policy_installer::{AccountRevision, EnvelopeArtifact};
use oz_policy_recorder::Recording;
use serde::Serialize;
use std::path::PathBuf;

const DEFAULT_TESTNET_RPC: &str = "https://soroban-testnet.stellar.org";
const DEFAULT_TESTNET_NETWORK: &str = "Test SDF Network ; September 2015";

#[derive(Debug, Parser)]
#[command(
    name = "oz-policy-cli",
    about = "OZ Accounts Policy Builder CLI — record / synthesize / prepare-install."
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
    /// Read a `Recording` JSON document from disk, run the Phase 2 decision
    /// tree, and emit the resulting `PolicySpec` JSON to stdout. Pure
    /// in-process — no network calls.
    Synthesize(SynthesizeArgs),
    /// Read a `PolicySpec` JSON document from disk, call
    /// `oz_policy_installer::build_install_envelope`, and emit the resulting
    /// `EnvelopeArtifact` JSON to stdout. Calls `simulateTransaction` and
    /// `getLedgerEntries` on the configured RPC; never auto-submits.
    PrepareInstall(PrepareInstallArgs),
    /// Read a `PolicySpec` JSON document from disk, run Phase 3 Track-B
    /// codegen for every `Generated` policy slot, and write the rendered
    /// source, optimized WASM, and lowercase-hex SHA-256 hash for each slot
    /// under `--out/slot_<i>/`. Existing (Track-A) slots are silently
    /// skipped.
    Codegen(CodegenArgs),
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

/// CLI mirror of `oz_policy_core::decision_tree::SynthesisMode`.
///
/// `clap` cannot derive `ValueEnum` on a type defined in another crate, so we
/// keep the enum local and convert in [`SynthesizeArgs::to_options`]. The
/// `value_enum` variant kebab-cases by default; we name explicitly so the CLI
/// surface matches the JSON `synthesis_mode` snake_case wire shape.
#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
enum ModeArg {
    Auto,
    #[value(name = "compose-only")]
    ComposeOnly,
    #[value(name = "codegen-only")]
    CodegenOnly,
}

impl ModeArg {
    fn into_mode(self) -> SynthesisMode {
        match self {
            ModeArg::Auto => SynthesisMode::Auto,
            ModeArg::ComposeOnly => SynthesisMode::ComposeOnly,
            ModeArg::CodegenOnly => SynthesisMode::CodegenOnly,
        }
    }
}

/// CLI mirror of `oz_policy_core::decision_tree::Tightness`.
#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
enum TightnessArg {
    Exact,
    #[value(name = "small-margin")]
    SmallMargin,
    Loose,
}

impl TightnessArg {
    fn into_tightness(self) -> Tightness {
        match self {
            TightnessArg::Exact => Tightness::Exact,
            TightnessArg::SmallMargin => Tightness::SmallMargin,
            TightnessArg::Loose => Tightness::Loose,
        }
    }
}

#[derive(Debug, Args)]
struct SynthesizeArgs {
    /// Path to a `Recording` JSON document on disk.
    #[arg(value_name = "RECORDING_FILE")]
    recording_file: PathBuf,

    /// Synthesis mode. `auto` permits both composition (Track A) and
    /// generated slots (Track B). `compose-only` requires every constraint
    /// to fit an existing OZ primitive. `codegen-only` forces every
    /// constraint into a `Generated` slot.
    #[arg(long, value_enum, default_value_t = ModeArg::Auto)]
    mode: ModeArg,

    /// Numeric tightness applied to observed `i128` constraints.
    #[arg(long, value_enum, default_value_t = TightnessArg::Exact)]
    tightness: TightnessArg,

    /// Lifetime (in ledgers) emitted as `PolicySpec.lifetime_ledgers` and
    /// (when applicable) `SpendingLimit.period_ledgers`. `None` → spec's
    /// `lifetime_ledgers` stays `None`; the `SpendingLimit` slot, if any,
    /// falls back to the decision tree's default.
    #[arg(long)]
    lifetime: Option<u32>,

    /// Optional StrKey `C…` address of a contract that takes over auth
    /// (delegated signer). When provided, the synthesizer emits exactly
    /// one `Delegated` signer instead of the per-recording observed signers.
    #[arg(long = "delegated-signer")]
    delegated_signer: Option<String>,

    /// Human-readable name for the emitted `ContextRuleSpec`. Must be
    /// ≤ `MAX_NAME_SIZE` (20) UTF-8 bytes per the on-chain `SmartAccount`.
    #[arg(long = "rule-name", default_value = "rule")]
    rule_name: String,
}

impl SynthesizeArgs {
    fn to_options(&self) -> SynthesisOptions {
        SynthesisOptions {
            mode: self.mode.into_mode(),
            tightness: self.tightness.into_tightness(),
            lifetime_ledgers: self.lifetime,
            delegated_signer: self.delegated_signer.clone(),
            context_rule_name: self.rule_name.clone(),
        }
    }
}

/// CLI mirror of `oz_policy_installer::AccountRevision`.
#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
enum AccountRevisionArg {
    #[value(name = "post-pr-655")]
    PostPr655,
    #[value(name = "pre-pr-655")]
    PrePr655,
    Unknown,
}

impl AccountRevisionArg {
    fn into_revision(self) -> AccountRevision {
        match self {
            AccountRevisionArg::PostPr655 => AccountRevision::PostPr655,
            AccountRevisionArg::PrePr655 => AccountRevision::PrePr655,
            AccountRevisionArg::Unknown => AccountRevision::Unknown,
        }
    }
}

#[derive(Debug, Args)]
struct PrepareInstallArgs {
    /// Path to a `PolicySpec` JSON document on disk.
    #[arg(value_name = "SPEC_FILE")]
    spec_file: PathBuf,

    /// StrKey `C…` address of the target smart-account contract.
    #[arg(long = "smart-account")]
    smart_account: String,

    /// StrKey `G…` address of the funding source account (pays fees, signs
    /// the envelope).
    #[arg(long)]
    source: String,

    /// Soroban RPC endpoint. Defaults to the public testnet RPC.
    #[arg(long, default_value = DEFAULT_TESTNET_RPC)]
    rpc: String,

    /// Stellar network passphrase. Defaults to testnet.
    #[arg(long, default_value = DEFAULT_TESTNET_NETWORK)]
    network: String,

    /// Caller-asserted smart-account release vintage. See
    /// `oz_policy_installer::AccountRevision` for the rationale —
    /// `unknown` / `pre-pr-655` are hard refusals in v1.
    #[arg(long = "account-revision", value_enum)]
    account_revision: AccountRevisionArg,
}

#[derive(Debug, Args)]
struct CodegenArgs {
    /// Path to a `PolicySpec` JSON document on disk.
    #[arg(value_name = "SPEC_FILE")]
    spec_file: PathBuf,

    /// Output directory. One subdirectory `slot_<i>/` is written per
    /// `Generated` policy slot, each containing `source.rs`, `policy.wasm`,
    /// and `wasm_hash.txt`. The directory is created if missing; existing
    /// files at those paths are overwritten.
    #[arg(long = "out", value_name = "DIR")]
    out: PathBuf,
}

/// JSON-serialisable mirror of [`EnvelopeArtifact`]. The installer's struct
/// is `Debug + Clone + PartialEq + Eq` but not `Serialize`, so we project
/// the three public fields into a small local type that derives
/// `serde::Serialize` for the CLI output.
#[derive(Debug, Serialize)]
struct EnvelopeArtifactJson {
    envelope_xdr_base64: String,
    min_resource_fee: i64,
    host_function_count: u32,
}

/// JSON-serialisable summary emitted by the `codegen` subcommand on success.
/// Mirrors a single generated-slot artifact's identifying metadata; the
/// actual `source.rs` / `policy.wasm` / `wasm_hash.txt` files are written
/// to disk under `--out/slot_<i>/`.
#[derive(Debug, Serialize)]
struct CodegenSlotSummary {
    /// Index into `PolicySpec.policies` for this generated slot.
    slot_index: usize,
    /// Lowercase hex SHA-256 of the optimized WASM bytes — the same value
    /// written into `slot_<i>/wasm_hash.txt`.
    wasm_sha256_hex: String,
    /// `true` iff the sandbox driver served this slot from its on-disk
    /// cache (no `cargo build` was re-run).
    cache_hit: bool,
    /// Size of the optimized WASM in bytes.
    wasm_bytes: usize,
}

/// Top-level JSON output of the `codegen` subcommand.
#[derive(Debug, Serialize)]
struct CodegenReport {
    out_dir: String,
    generated_slot_count: usize,
    slots: Vec<CodegenSlotSummary>,
}

impl From<&EnvelopeArtifact> for EnvelopeArtifactJson {
    fn from(a: &EnvelopeArtifact) -> Self {
        Self {
            envelope_xdr_base64: a.envelope_xdr_base64.clone(),
            min_resource_fee: a.min_resource_fee,
            host_function_count: a.host_function_count,
        }
    }
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
/// | E_SYNTH_NOT_EXPRESSIBLE          | 13        |
/// | E_INSTALL_PREFLIGHT_FAILED       | 14        |
/// | E_CODEGEN_COMPILE_FAILED         | 15        |
/// | (any other E_*)                  | 20        |
fn exit_code_for(e: &Error) -> i32 {
    match e {
        Error::RecorderHashNotFound(_) => 10,
        Error::RecorderSimFailed(_) => 11,
        Error::RecorderXdrDecodeFailed(_) => 12,
        Error::SynthNotExpressible(_) => 13,
        Error::InstallPreflightFailed(_) => 14,
        Error::CodegenCompileFailed(_) => 15,
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

/// Synthesize a `PolicySpec` from a recording on disk. Pure I/O + a single
/// call into `oz_policy_core::decision_tree::synthesize`. Surface all errors
/// verbatim — no silent masking.
fn run_synthesize(args: SynthesizeArgs) -> Result<PolicySpec, Error> {
    let raw = std::fs::read_to_string(&args.recording_file).map_err(|e| {
        // Recording read errors aren't an E_RECORDER_* code (those are
        // RPC-level); surface as RecorderXdrDecodeFailed to keep the
        // failure inside the existing error taxonomy until we add a
        // dedicated E_CLI_READ_FAILED.
        Error::RecorderXdrDecodeFailed(format!(
            "failed to read recording file {}: {e}",
            args.recording_file.display()
        ))
    })?;
    let recording: Recording = serde_json::from_str(&raw).map_err(|e| {
        Error::RecorderXdrDecodeFailed(format!(
            "failed to parse recording JSON from {}: {e}",
            args.recording_file.display()
        ))
    })?;
    let opts = args.to_options();
    oz_policy_core::decision_tree::synthesize(&recording, &opts)
}

/// Read a spec from disk, run Phase 3 Track-B codegen, and write the
/// per-slot artifacts under `args.out`. Returns a summary suitable for
/// pretty-printing to stdout.
///
/// Disk layout produced under `args.out`:
///
/// ```text
/// <out>/
///   slot_<i>/
///     source.rs       — rendered Rust source (artifact.source)
///     policy.wasm     — optimized WASM bytes (artifact.wasm)
///     wasm_hash.txt   — lowercase hex SHA-256 of policy.wasm
/// ```
///
/// where `<i>` is the original index in `PolicySpec.policies`. Track-A
/// `Existing` slots are silently skipped (no `slot_<i>/` directory is
/// emitted for them). A spec with zero generated slots produces an empty
/// summary, exit 0, and no files written beyond the top-level `out` dir
/// itself.
async fn run_codegen(args: CodegenArgs) -> Result<CodegenReport, Error> {
    let raw = std::fs::read_to_string(&args.spec_file).map_err(|e| {
        Error::CodegenCompileFailed(format!(
            "failed to read spec file {}: {e}",
            args.spec_file.display()
        ))
    })?;
    let spec: PolicySpec = serde_json::from_str(&raw).map_err(|e| {
        Error::CodegenCompileFailed(format!(
            "failed to parse spec JSON from {}: {e}",
            args.spec_file.display()
        ))
    })?;

    // Drive codegen end-to-end. `synthesize_track_b` already returns
    // artifacts in slot order, skipping Existing slots silently.
    let artifacts = oz_policy_codegen::synthesize_track_b(&spec).await?;

    std::fs::create_dir_all(&args.out).map_err(|e| {
        Error::CodegenCompileFailed(format!(
            "failed to create out dir {}: {e}",
            args.out.display()
        ))
    })?;

    // We need to re-map artifact-index → original-slot-index so the per-slot
    // directories carry the spec's original numbering. `synthesize_track_b`
    // collapses Existing slots; iterate the spec to recover the mapping.
    let mut summaries = Vec::new();
    let mut art_iter = artifacts.into_iter();
    for (slot_idx, slot) in spec.policies.iter().enumerate() {
        if !matches!(slot, oz_policy_core::spec::PolicySlot::Generated { .. }) {
            continue;
        }
        let artifact = art_iter
            .next()
            .expect("internal: synthesize_track_b returned fewer artifacts than Generated slots");

        let slot_dir = args.out.join(format!("slot_{slot_idx}"));
        std::fs::create_dir_all(&slot_dir).map_err(|e| {
            Error::CodegenCompileFailed(format!(
                "failed to create slot dir {}: {e}",
                slot_dir.display()
            ))
        })?;
        std::fs::write(slot_dir.join("source.rs"), artifact.source.as_bytes()).map_err(|e| {
            Error::CodegenCompileFailed(format!(
                "failed to write source.rs in {}: {e}",
                slot_dir.display()
            ))
        })?;
        std::fs::write(slot_dir.join("policy.wasm"), &artifact.wasm).map_err(|e| {
            Error::CodegenCompileFailed(format!(
                "failed to write policy.wasm in {}: {e}",
                slot_dir.display()
            ))
        })?;
        let hex = hex_lower(&artifact.wasm_hash);
        // Single trailing newline so `cat`/`diff` on the file look right
        // and the value can be substring-loaded cleanly by other tools.
        std::fs::write(slot_dir.join("wasm_hash.txt"), format!("{hex}\n")).map_err(|e| {
            Error::CodegenCompileFailed(format!(
                "failed to write wasm_hash.txt in {}: {e}",
                slot_dir.display()
            ))
        })?;

        summaries.push(CodegenSlotSummary {
            slot_index: slot_idx,
            wasm_sha256_hex: hex,
            cache_hit: artifact.cache_hit,
            wasm_bytes: artifact.wasm.len(),
        });
    }

    Ok(CodegenReport {
        out_dir: args.out.display().to_string(),
        generated_slot_count: summaries.len(),
        slots: summaries,
    })
}

/// Lowercase-hex encode a 32-byte digest. Hand-rolled to avoid adding a
/// dedicated hex dep to the CLI crate; this function is invoked at most
/// once per generated slot.
fn hex_lower(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Read a spec from disk and call into the installer. Surfaces real errors
/// (preflight, network, primitive_address_unknown) verbatim.
async fn run_prepare_install(args: PrepareInstallArgs) -> Result<EnvelopeArtifact, Error> {
    let raw = std::fs::read_to_string(&args.spec_file).map_err(|e| {
        Error::InstallPreflightFailed(format!(
            "failed to read spec file {}: {e}",
            args.spec_file.display()
        ))
    })?;
    let spec: PolicySpec = serde_json::from_str(&raw).map_err(|e| {
        Error::InstallPreflightFailed(format!(
            "failed to parse spec JSON from {}: {e}",
            args.spec_file.display()
        ))
    })?;
    oz_policy_installer::build_install_envelope(
        &spec,
        &args.smart_account,
        &args.source,
        &args.network,
        &args.rpc,
        args.account_revision.into_revision(),
    )
    .await
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
    match cli.command {
        Command::Record(args) => {
            let result = rt.block_on(run_record(args));
            print_or_exit(result);
        }
        Command::Synthesize(args) => {
            // Pure-logic — no async needed, but we run inside the runtime
            // for surface symmetry with the other branches.
            let result = run_synthesize(args);
            print_or_exit(result);
        }
        Command::PrepareInstall(args) => {
            let result = rt.block_on(run_prepare_install(args));
            // EnvelopeArtifact isn't directly `Serialize`; project to the
            // CLI's local JSON view.
            match result {
                Ok(art) => print_or_exit::<EnvelopeArtifactJson>(Ok((&art).into())),
                Err(e) => {
                    eprintln!("{}: {}", e.code(), e);
                    std::process::exit(exit_code_for(&e));
                }
            }
        }
        Command::Codegen(args) => {
            let result = rt.block_on(run_codegen(args));
            print_or_exit(result);
        }
    }
}

/// Pretty-print `result` (`Ok`) or surface the error to stderr (`Err`).
/// Generic over `T: serde::Serialize` so the same printer handles every
/// subcommand's success type without duplication.
fn print_or_exit<T: serde::Serialize>(result: Result<T, Error>) {
    match result {
        Ok(val) => match serde_json::to_string_pretty(&val) {
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
            other => panic!("expected Record, got {other:?}"),
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
            other => panic!("expected Record, got {other:?}"),
        }
    }

    #[test]
    fn record_subcommand_defaults_rpc_and_network_to_testnet() {
        let cli = Cli::try_parse_from(["oz-policy-cli", "record", "--hash", "abc"])
            .expect("parse without --rpc/--network falls back to defaults");
        let Command::Record(args) = cli.command else {
            panic!("expected Record subcommand")
        };
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
    fn exit_code_mapping_is_distinct_per_variant() {
        assert_eq!(exit_code_for(&Error::RecorderHashNotFound("h".into())), 10);
        assert_eq!(exit_code_for(&Error::RecorderSimFailed("s".into())), 11);
        assert_eq!(
            exit_code_for(&Error::RecorderXdrDecodeFailed("x".into())),
            12
        );
        assert_eq!(exit_code_for(&Error::SynthNotExpressible("n".into())), 13);
        assert_eq!(
            exit_code_for(&Error::InstallPreflightFailed("p".into())),
            14
        );
        assert_eq!(exit_code_for(&Error::VerifyDrift("d".into())), 20);
    }

    // -------------------------------------------------------------------
    // Synthesize subcommand
    // -------------------------------------------------------------------

    #[test]
    fn synthesize_subcommand_accepts_full_form() {
        let cli = Cli::try_parse_from([
            "oz-policy-cli",
            "synthesize",
            "rec.json",
            "--mode",
            "compose-only",
            "--tightness",
            "exact",
            "--lifetime",
            "432000",
            "--rule-name",
            "sep41-subscription",
        ])
        .expect("parse synthesize full form");
        match cli.command {
            Command::Synthesize(args) => {
                assert_eq!(args.recording_file, PathBuf::from("rec.json"));
                assert_eq!(args.mode, ModeArg::ComposeOnly);
                assert_eq!(args.tightness, TightnessArg::Exact);
                assert_eq!(args.lifetime, Some(432_000));
                assert_eq!(args.rule_name, "sep41-subscription");
                assert_eq!(args.delegated_signer, None);
            }
            other => panic!("expected Synthesize, got {other:?}"),
        }
    }

    #[test]
    fn synthesize_subcommand_defaults_are_auto_exact_named_rule() {
        let cli = Cli::try_parse_from(["oz-policy-cli", "synthesize", "rec.json"])
            .expect("parse synthesize with defaults");
        let Command::Synthesize(args) = cli.command else {
            panic!("expected Synthesize")
        };
        assert_eq!(args.mode, ModeArg::Auto);
        assert_eq!(args.tightness, TightnessArg::Exact);
        assert_eq!(args.lifetime, None);
        assert_eq!(args.rule_name, "rule");
    }

    /// `--mode` only accepts the three documented values. Any other input
    /// (typo / older naming) must be rejected by clap before the command
    /// runs.
    #[test]
    fn synthesize_subcommand_rejects_unknown_mode() {
        let err = Cli::try_parse_from([
            "oz-policy-cli",
            "synthesize",
            "rec.json",
            "--mode",
            "compose_only", // wrong: we use kebab-case
        ])
        .expect_err("unknown --mode value must be rejected by clap");
        assert_eq!(err.kind(), clap::error::ErrorKind::InvalidValue);
    }

    #[test]
    fn synthesize_subcommand_rejects_unknown_tightness() {
        let err = Cli::try_parse_from([
            "oz-policy-cli",
            "synthesize",
            "rec.json",
            "--tightness",
            "small_margin", // wrong: we use kebab-case
        ])
        .expect_err("unknown --tightness value must be rejected by clap");
        assert_eq!(err.kind(), clap::error::ErrorKind::InvalidValue);
    }

    #[test]
    fn synthesize_subcommand_requires_recording_file() {
        let err = Cli::try_parse_from(["oz-policy-cli", "synthesize"])
            .expect_err("missing positional must fail");
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn synthesize_to_options_round_trips_every_field() {
        let args = SynthesizeArgs {
            recording_file: PathBuf::from("rec.json"),
            mode: ModeArg::CodegenOnly,
            tightness: TightnessArg::Loose,
            lifetime: Some(100_000),
            delegated_signer: Some("CXYZ".to_string()),
            rule_name: "x".to_string(),
        };
        let opts = args.to_options();
        assert!(matches!(opts.mode, SynthesisMode::CodegenOnly));
        assert!(matches!(opts.tightness, Tightness::Loose));
        assert_eq!(opts.lifetime_ledgers, Some(100_000));
        assert_eq!(opts.delegated_signer.as_deref(), Some("CXYZ"));
        assert_eq!(opts.context_rule_name, "x");
    }

    // -------------------------------------------------------------------
    // PrepareInstall subcommand
    // -------------------------------------------------------------------

    #[test]
    fn prepare_install_accepts_full_form() {
        let cli = Cli::try_parse_from([
            "oz-policy-cli",
            "prepare-install",
            "spec.json",
            "--smart-account",
            "CSMART",
            "--source",
            "GSRC",
            "--rpc",
            "http://x",
            "--network",
            "Test",
            "--account-revision",
            "post-pr-655",
        ])
        .expect("parse prepare-install full form");
        match cli.command {
            Command::PrepareInstall(args) => {
                assert_eq!(args.spec_file, PathBuf::from("spec.json"));
                assert_eq!(args.smart_account, "CSMART");
                assert_eq!(args.source, "GSRC");
                assert_eq!(args.rpc, "http://x");
                assert_eq!(args.network, "Test");
                assert_eq!(args.account_revision, AccountRevisionArg::PostPr655);
            }
            other => panic!("expected PrepareInstall, got {other:?}"),
        }
    }

    #[test]
    fn prepare_install_requires_smart_account() {
        let err = Cli::try_parse_from([
            "oz-policy-cli",
            "prepare-install",
            "spec.json",
            "--source",
            "GSRC",
            "--account-revision",
            "post-pr-655",
        ])
        .expect_err("missing --smart-account must fail");
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn prepare_install_requires_source() {
        let err = Cli::try_parse_from([
            "oz-policy-cli",
            "prepare-install",
            "spec.json",
            "--smart-account",
            "CSMART",
            "--account-revision",
            "post-pr-655",
        ])
        .expect_err("missing --source must fail");
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn prepare_install_requires_account_revision() {
        let err = Cli::try_parse_from([
            "oz-policy-cli",
            "prepare-install",
            "spec.json",
            "--smart-account",
            "CSMART",
            "--source",
            "GSRC",
        ])
        .expect_err("missing --account-revision must fail");
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn prepare_install_rejects_unknown_account_revision() {
        let err = Cli::try_parse_from([
            "oz-policy-cli",
            "prepare-install",
            "spec.json",
            "--smart-account",
            "CSMART",
            "--source",
            "GSRC",
            "--account-revision",
            "post_pr_655", // wrong: kebab-case is required
        ])
        .expect_err("unknown --account-revision value must be rejected");
        assert_eq!(err.kind(), clap::error::ErrorKind::InvalidValue);
    }

    #[test]
    fn prepare_install_defaults_rpc_and_network_to_testnet() {
        let cli = Cli::try_parse_from([
            "oz-policy-cli",
            "prepare-install",
            "spec.json",
            "--smart-account",
            "CSMART",
            "--source",
            "GSRC",
            "--account-revision",
            "unknown",
        ])
        .expect("defaults must populate rpc/network");
        let Command::PrepareInstall(args) = cli.command else {
            panic!("expected PrepareInstall")
        };
        assert_eq!(args.rpc, DEFAULT_TESTNET_RPC);
        assert_eq!(args.network, DEFAULT_TESTNET_NETWORK);
        assert_eq!(args.account_revision, AccountRevisionArg::Unknown);
    }

    // -------------------------------------------------------------------
    // Codegen subcommand
    // -------------------------------------------------------------------

    /// The full-form invocation must parse with `--out` populated.
    #[test]
    fn codegen_subcommand_accepts_full_form() {
        let cli = Cli::try_parse_from([
            "oz-policy-cli",
            "codegen",
            "spec.json",
            "--out",
            "target/codegen-out",
        ])
        .expect("parse codegen full form");
        match cli.command {
            Command::Codegen(args) => {
                assert_eq!(args.spec_file, PathBuf::from("spec.json"));
                assert_eq!(args.out, PathBuf::from("target/codegen-out"));
            }
            other => panic!("expected Codegen, got {other:?}"),
        }
    }

    /// `--out` is required by clap (no default). Forgetting it must surface
    /// `MissingRequiredArgument`, not silently default into the cwd.
    #[test]
    fn codegen_subcommand_requires_out_flag() {
        let err = Cli::try_parse_from(["oz-policy-cli", "codegen", "spec.json"])
            .expect_err("missing --out must fail");
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    /// `codegen` must require the positional spec-file argument too.
    #[test]
    fn codegen_subcommand_requires_spec_file() {
        let err = Cli::try_parse_from(["oz-policy-cli", "codegen", "--out", "target/x"])
            .expect_err("missing spec file must fail");
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    /// `--help` must parse cleanly (clap turns this into `DisplayHelp`,
    /// which is *not* an error in the failure sense but appears as one
    /// from `try_parse_from`).
    #[test]
    fn codegen_subcommand_help_renders() {
        let err = Cli::try_parse_from(["oz-policy-cli", "codegen", "--help"])
            .expect_err("`--help` short-circuits parsing");
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
        let rendered = err.to_string();
        // Cross-check that the help text mentions both the required spec
        // file positional and the `--out` flag — those are the load-bearing
        // pieces of the subcommand surface.
        assert!(
            rendered.contains("--out"),
            "codegen --help must mention --out flag; got:\n{rendered}"
        );
        assert!(
            rendered.contains("SPEC_FILE"),
            "codegen --help must mention SPEC_FILE positional; got:\n{rendered}"
        );
    }

    #[test]
    fn exit_code_includes_codegen_compile_failed() {
        assert_eq!(exit_code_for(&Error::CodegenCompileFailed("x".into())), 15);
    }

    #[test]
    fn account_revision_arg_round_trips_to_installer_enum() {
        assert!(matches!(
            AccountRevisionArg::PostPr655.into_revision(),
            AccountRevision::PostPr655
        ));
        assert!(matches!(
            AccountRevisionArg::PrePr655.into_revision(),
            AccountRevision::PrePr655
        ));
        assert!(matches!(
            AccountRevisionArg::Unknown.into_revision(),
            AccountRevision::Unknown
        ));
    }
}

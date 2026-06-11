//! mcp server library surface. modules share state via [`McpStore`].

// `deny` (not `forbid`) so store.rs can scope a narrow `set_var` exemption.
#![deny(unsafe_code)]

pub mod prompts;
pub mod resources;
pub mod store;

pub mod error_mapping;
pub mod tools;
pub mod verify_chain;

pub mod auth;
pub mod server;

pub use auth::{bearer_layer, BearerAuth, BearerAuthLayer, BearerOutcome};
pub use error_mapping::{code_to_int, error_to_jsonrpc};
pub use prompts::Prompts;
pub use resources::Resources;
pub use server::PolicyServer;
pub use store::{ArtifactBundle, McpStore, StorePersistKind};
pub use tools::{
    check_forbidden, create_snapshot, export_policy, get_snapshot, record_transaction,
    simulate_custom_source, simulate_policy, spawn_gc as spawn_snapshot_gc, synthesize_policy,
    verify_install, CreateSnapshotInput, CreateSnapshotOutput, DriftItem, ExportFormat,
    ExportPolicyInput, ExportPolicyOutput, ForbiddenPattern, GetSnapshotInput, NetworkKind,
    RecordTransactionInput, RecordTransactionOutput, SimulateCustomSourceInput,
    SimulatePolicyInput, SnapshotRecord, SynthesizePolicyInput, SynthesizePolicyOutput,
    VerifyInstallInput, VerifyInstallOutput, CARGO_BUILD_FAILED_CODE, CARGO_BUILD_FAILED_NAME,
    PREFLIGHT_FORBIDDEN_PATTERN_CODE, PREFLIGHT_FORBIDDEN_PATTERN_NAME,
};

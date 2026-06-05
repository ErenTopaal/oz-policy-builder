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
    export_policy, record_transaction, simulate_policy, synthesize_policy, verify_install,
    DriftItem, ExportFormat, ExportPolicyInput, ExportPolicyOutput, NetworkKind,
    RecordTransactionInput, RecordTransactionOutput, SimulatePolicyInput, SynthesizePolicyInput,
    SynthesizePolicyOutput, VerifyInstallInput, VerifyInstallOutput,
};

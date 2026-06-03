//! Library surface for the OZ Accounts Policy Builder MCP server.
//!
//! Phase 5 module ownership:
//! * Stream A — `tools.rs`, `error_mapping.rs` (tool handlers + the
//!   `oz_policy_core::Error → MCP error` mapping table).
//! * Stream B — [`store`], [`resources`], [`prompts`] (in-memory cache,
//!   `resources/list` + `resources/read`, `prompts/list` + `prompts/get`).
//! * Stream C (this stream) — [`auth`] (HTTP bearer-token middleware),
//!   [`server`] (the `ServerHandler` implementation wiring tools / resources
//!   / prompts together), and the binary transport in `main.rs`.
//!
//! The three streams share state exclusively through [`McpStore`].

// `deny` rather than `forbid` so individual modules can `#[allow(unsafe_code)]`
// for narrowly-scoped, audited blocks. Rust 2024 reclassified `std::env::set_var`
// / `std::env::remove_var` as `unsafe` (mutating the process-wide env is racy
// across threads), so the disk-persistence + test scaffolding in `store.rs`
// (Stream B) needs a localised exemption rather than a crate-wide forbid.
// All other modules MUST keep the deny in effect.
#![deny(unsafe_code)]

// --- Stream B (owned) ------------------------------------------------------
pub mod prompts;
pub mod resources;
pub mod store;

// --- Stream A (owned) ------------------------------------------------------
pub mod error_mapping;
pub mod tools;
// --- RFP deliverable #5 (2026-05-18) — on-chain readback for verify_install
pub mod verify_chain;

// --- Stream C (this stream) ------------------------------------------------
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

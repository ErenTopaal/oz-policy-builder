//! Library surface for the OZ Accounts Policy Builder MCP server.
//!
//! Phase 5 module ownership (sibling streams add their own `pub mod`
//! declarations to this file as they land):
//!
//! * Stream A — `tools.rs`, `error_mapping.rs` (tool handlers + the
//!   `oz_policy_core::Error → MCP error` mapping table).
//! * Stream B (this stream) — [`store`], `resources`, `prompts`
//!   (in-memory cache, `resources/list` + `resources/read`,
//!   `prompts/list` + `prompts/get`).
//! * Stream C — `auth` (HTTP bearer-token middleware), `server` (the
//!   `ServerHandler` implementation), and the binary transport in
//!   `main.rs`.
//!
//! The three streams share state exclusively through [`McpStore`].

// `deny` rather than `forbid` so individual modules can `#[allow(unsafe_code)]`
// for narrowly-scoped, audited blocks (e.g. Rust 2024 reclassified some
// `std::env::*` mutators as `unsafe`; staying on 2021 keeps them safe today,
// but the lint stays at deny in case the workspace flips edition later).
#![deny(unsafe_code)]

// --- Stream B (owned) ------------------------------------------------------
pub mod store;

pub use store::{ArtifactBundle, McpStore, StorePersistKind};

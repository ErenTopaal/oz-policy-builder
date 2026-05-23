//! Simulation harness for compiled policy WASM artifacts.
//!
//! Phase 1 scope: skeleton only. The in-process `soroban-env-host` driver and
//! the proptest deny-vector generator land in Phase 4 (see `plan.md` § "Phase
//! 4 — Simulation harness").

#![forbid(unsafe_code)]

pub mod deny;
pub mod host;
pub mod permit;
pub mod placeholder;
pub mod run;

pub use host::{AuthPayload, HostExecError, TestContext, TestHost};
pub use permit::replay_recording;
pub use run::{run_full_suite, DenyResult, PermitResult, SimReport};

//! simulation harness for compiled policy wasm artifacts.

#![forbid(unsafe_code)]

pub mod deny;
pub mod host;
pub mod permit;
pub mod placeholder;
pub mod run;

pub use host::{AuthPayload, HostExecError, TestContext, TestHost};
pub use permit::replay_recording;
pub use run::{run_full_suite, DenyResult, PermitResult, SimReport};

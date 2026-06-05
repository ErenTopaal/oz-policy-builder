//! install-envelope builder. never auto-submits — returns unsigned XDR after
//! `simulateTransaction`. wallets handle signing; cli/mcp own submit.

#![forbid(unsafe_code)]

pub mod envelope;
pub mod preflight;
pub mod registry;

pub use envelope::{build_install_envelope, EnvelopeArtifact};
pub use preflight::AccountRevision;

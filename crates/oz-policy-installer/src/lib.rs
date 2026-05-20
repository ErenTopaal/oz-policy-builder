//! Install-envelope builder for OZ `SmartAccount::add_context_rule` /
//! `add_policy`.
//!
//! Public surface:
//! * [`build_install_envelope`] — async, returns wallet-ready base64 XDR.
//! * [`EnvelopeArtifact`] — the structured result.
//! * [`AccountRevision`] — caller-asserted statement about the target
//!   smart-account contract's release vintage (per `docs/oz-internal-shapes.md` §8).
//!
//! The implementation **never** auto-submits. The function builds an
//! unsigned `TransactionEnvelope` XDR, runs `simulateTransaction` to
//! collect resources / auth, and hands the envelope back to the caller.
//! Phase 7 wallet adapters layer signing on top; the CLI / MCP server
//! own submission.
//!
//! ### Module map
//! * [`preflight`] — pure-logic precondition checks (no I/O).
//! * [`envelope`] — RPC + XDR assembly.
//! * [`registry`] — published OZ primitive contract addresses, keyed by
//!   network passphrase. Returns `None` in v1 — see the module
//!   doc-comment for the honest finding.

#![forbid(unsafe_code)]

pub mod envelope;
pub mod preflight;
pub mod registry;

pub use envelope::{build_install_envelope, EnvelopeArtifact};
pub use preflight::AccountRevision;

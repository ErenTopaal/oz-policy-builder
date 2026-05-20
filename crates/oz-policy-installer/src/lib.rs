//! Install-envelope builder for OZ `SmartAccount::add_context_rule` /
//! `add_policy`.
//!
//! Phase 2 Stream B scaffold. Landed modules:
//! * [`preflight`] — pure-logic precondition checks (no I/O).
//! * [`registry`] — network-keyed primitive contract address table.
//!
//! [`envelope`] lands in the next commit.

#![forbid(unsafe_code)]

pub mod preflight;
pub mod registry;

pub use preflight::AccountRevision;

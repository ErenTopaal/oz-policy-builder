//! Core types for the OZ Accounts Policy Builder.
//!
//! Phase 1 scope: only the [`errors::Error`] enum and an inert [`spec`]
//! placeholder are populated. The real `PolicySpec` IR, schema, and decision
//! tree land in Phase 2 (see `plan.md`).

#![forbid(unsafe_code)]

pub mod errors;
pub mod spec;

pub use errors::Error;

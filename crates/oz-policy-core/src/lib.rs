//! Core types for the OZ Accounts Policy Builder.
//!
//! Phase 2 bootstrap (P2-T1 / P2-T2):
//! * [`errors::Error`] — canonical wire-stable error enum (Phase 1 scope).
//! * [`arg_value::ArgValue`] — fully-decoded `ScVal` mirror, relocated from
//!   `oz-policy-recorder::recording` so the policy IR can reference it
//!   without a `core -> recorder` cycle. The recorder still re-exports the
//!   type from its public surface.
//! * [`spec::PolicySpec`] — versioned policy IR (`oz-policy-builder/v1`)
//!   consumed by the Phase 2 decision tree and the Phase 2 installer.

#![forbid(unsafe_code)]

pub mod arg_value;
pub mod errors;
pub mod spec;

pub use arg_value::{ArgValue, MapEntry};
pub use errors::Error;

//! Core types for the OZ Accounts Policy Builder.
//!
//! Phase 2 (P2-T1 / P2-T2 / P2-Stream-A):
//! * [`errors::Error`] — canonical wire-stable error enum (Phase 1 scope).
//! * [`arg_value::ArgValue`] — fully-decoded `ScVal` mirror, relocated from
//!   `oz-policy-recorder::recording` so the policy IR can reference it
//!   without a `core -> recorder` cycle. The recorder still re-exports the
//!   type from its public surface.
//! * [`recording`] — Recording IR (`Recording`, `ContractRecord`, `AuthTree`,
//!   etc.). Physically moved here in Phase 2 Stream A so the policy IR and
//!   the decision tree can reference it without a `core -> recorder` cycle;
//!   the recorder re-exports the types unchanged.
//! * [`spec::PolicySpec`] — versioned policy IR (`oz-policy-builder/v1`)
//!   consumed by the Phase 2 decision tree and the Phase 2 installer.
//! * [`sep41`] — SEP-41 SAC detection helpers used by the decision tree to
//!   gate `spending_limit` composition.
//! * [`decision_tree::synthesize`] — Track-A composition + Track-B slot
//!   emission for a single `Recording` (Phase 2 Stream A).

#![forbid(unsafe_code)]

pub mod arg_value;
pub mod decision_tree;
pub mod errors;
pub mod recording;
pub mod sep41;
pub mod spec;

pub use arg_value::{ArgValue, MapEntry};
pub use decision_tree::{synthesize, SynthesisOptions, Tightness};
pub use errors::Error;
pub use sep41::is_sep41_transfer;

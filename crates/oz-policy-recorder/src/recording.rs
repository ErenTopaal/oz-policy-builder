//! Recording IR — re-export shim.
//!
//! Phase 2 (Stream A) physically relocated the Recording IR types
//! (`Recording`, `ContractRecord`, `AuthTree`, `AuthEntry`, `AuthInvocation`,
//! `AuthFunction`, `Credentials`, `StateDelta`, `TypedEvent`, `IngestSource`,
//! `RECORDING_SCHEMA_URI`) plus the previously-relocated `ArgValue` /
//! `MapEntry` into `oz_policy_core::recording` and `oz_policy_core::arg_value`
//! respectively. That move lets the policy IR ([`oz_policy_core::spec`]) and
//! the synthesizer entry point ([`oz_policy_core::decision_tree`]) reference
//! the Recording shapes without introducing a `core -> recorder` cycle.
//!
//! This module is preserved as a thin `pub use` shim so the recorder's
//! public surface — and every Recording JSON document already on disk —
//! is byte-equal to the Phase 1 layout. The wire schema URI is unchanged;
//! every serde tag/content attribute is unchanged; only the physical home
//! of the types moved.

pub use oz_policy_core::recording::{
    AuthEntry, AuthFunction, AuthInvocation, AuthTree, ContractRecord, Credentials, IngestSource,
    Recording, StateDelta, TypedEvent, RECORDING_SCHEMA_URI,
};
// `ArgValue` and `MapEntry` were relocated into `oz_policy_core::arg_value`
// in Phase 2 (P2-T1); re-export them here too so the recorder's existing
// `pub use recording::ArgValue` surface in `lib.rs` keeps resolving.
pub use oz_policy_core::{ArgValue, MapEntry};

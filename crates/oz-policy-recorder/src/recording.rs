//! re-export shim — recording IR lives in `oz_policy_core` to avoid a cycle.

pub use oz_policy_core::recording::{
    AuthEntry, AuthFunction, AuthInvocation, AuthTree, ContractRecord, Credentials, IngestSource,
    Recording, StateDelta, TypedEvent, RECORDING_SCHEMA_URI,
};
pub use oz_policy_core::{ArgValue, MapEntry};

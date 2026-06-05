//! stellar transaction recorder — ingest by hash or sim, emit a typed Recording.

#![forbid(unsafe_code)]

pub mod recorder;
pub mod recording;

pub use recorder::{decode_from_xdr_blobs, record_by_hash, record_by_simulation};
pub use recording::{
    ArgValue, AuthEntry, AuthFunction, AuthInvocation, AuthTree, ContractRecord, Credentials,
    IngestSource, MapEntry, Recording, StateDelta, TypedEvent, RECORDING_SCHEMA_URI,
};

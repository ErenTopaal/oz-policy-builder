//! Stellar transaction recorder.
//!
//! Ingests a Stellar Soroban transaction — either by its on-chain hash via
//! `getTransaction` or speculatively by simulating a base64 `TransactionEnvelope`
//! via `simulateTransaction` — and emits a deterministic [`Recording`] JSON
//! document. The recording is fully decoded: no opaque XDR blobs survive in the
//! output. All `ScVal`s are walked into a typed [`recording::ArgValue`] tree,
//! the `SorobanAuthorizationEntry[]` becomes a typed [`recording::AuthTree`],
//! `LedgerEntryChanges` become [`recording::StateDelta`]s keyed by their
//! decoded `ScVal`, and Soroban contract events become [`recording::TypedEvent`]s.
//!
//! See `plan.md` § "Phase 1 — Foundations" (P1-T3).

#![forbid(unsafe_code)]

pub mod recorder;
pub mod recording;

pub use recorder::{decode_from_xdr_blobs, record_by_hash, record_by_simulation};
pub use recording::{
    ArgValue, AuthEntry, AuthFunction, AuthInvocation, AuthTree, ContractRecord, Credentials,
    IngestSource, Recording, StateDelta, TypedEvent, RECORDING_SCHEMA_URI,
};

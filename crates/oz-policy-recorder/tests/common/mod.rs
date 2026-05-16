//! Shared helper for integration tests: thin alias for the `pub`-but-hidden
//! `decode_from_xdr_blobs` so tests can call it without re-declaring the
//! signature. Kept in its own file (rather than `mod helpers` inside each
//! test file) so the `cargo nextest` test runner doesn't double-count it as
//! a test target.

#![allow(dead_code)]

use oz_policy_core::Error;
use oz_policy_recorder::Recording;

pub fn decode(envelope_b64: &str, meta_b64: &str, network: &str) -> Result<Recording, Error> {
    oz_policy_recorder::decode_from_xdr_blobs(envelope_b64, meta_b64, network)
}

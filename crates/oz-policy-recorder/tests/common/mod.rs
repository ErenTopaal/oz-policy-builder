//! thin alias for the hidden `decode_from_xdr_blobs`.

#![allow(dead_code)]

use oz_policy_core::Error;
use oz_policy_recorder::Recording;

pub fn decode(envelope_b64: &str, meta_b64: &str, network: &str) -> Result<Recording, Error> {
    oz_policy_recorder::decode_from_xdr_blobs(envelope_b64, meta_b64, network)
}

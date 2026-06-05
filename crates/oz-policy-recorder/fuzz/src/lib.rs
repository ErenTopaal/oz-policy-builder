//! library half of the recorder fuzz harness. body in
//! `run_recording_decode_panic_free` so cargo test can exercise it too.
//! invariant: only `Recorder*` errors or `Ok` — anything else is a finding.

use arbitrary::Arbitrary;
use base64::Engine;
use oz_policy_core::Error;
use oz_policy_recorder::recorder::decode_from_xdr_blobs;

/// structured fuzz input.
#[derive(Debug, Arbitrary)]
pub struct DecoderInput {
    /// base64-encoded into envelope_b64. empty allowed.
    pub envelope: Vec<u8>,
    /// base64-encoded into result_meta_b64.
    pub meta: Vec<u8>,
    /// passphrase, written into the Recording verbatim.
    pub passphrase: String,
}

/// fuzz target body. accepted: Ok, Recorder* errors. anything else = finding.
pub fn run_recording_decode_panic_free(input: &DecoderInput) {
    let engine = base64::engine::general_purpose::STANDARD;
    let envelope_b64 = engine.encode(&input.envelope);
    let meta_b64 = engine.encode(&input.meta);
    // empty meta = skeleton mode; exercise that branch explicitly.
    let meta_b64_to_use = if input.meta.is_empty() {
        String::new()
    } else {
        meta_b64
    };
    let res = decode_from_xdr_blobs(&envelope_b64, &meta_b64_to_use, &input.passphrase);
    match res {
        Ok(_) => {}
        Err(err) => {
            assert!(
                matches!(
                    err,
                    Error::RecorderXdrDecodeFailed(_)
                        | Error::RecorderSimFailed(_)
                        | Error::RecorderHashNotFound(_)
                ),
                "decode_from_xdr_blobs returned non-recorder error variant: {err:?}"
            );
        }
    }
}

#[cfg(test)]
mod smoke {
    use super::*;
    use arbitrary::{Arbitrary, Unstructured};

    /// smoke: derive compiles, run doesn't panic on synthetic input.
    #[test]
    fn arbitrary_decoder_input_does_not_panic() {
        let inputs: [&[u8]; 2] = [
            &[0u8; 8],
            &[
                0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0xde, 0xad, 0xbe, 0xef, 0x12, 0x34,
                0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
            ],
        ];
        for bytes in inputs {
            let mut u = Unstructured::new(bytes);
            if let Ok(input) = DecoderInput::arbitrary(&mut u) {
                run_recording_decode_panic_free(&input);
            }
        }
    }

    /// empty everything must return a typed error, not panic.
    #[test]
    fn empty_input_returns_typed_error() {
        let input = DecoderInput {
            envelope: Vec::new(),
            meta: Vec::new(),
            passphrase: String::new(),
        };
        run_recording_decode_panic_free(&input);
    }
}

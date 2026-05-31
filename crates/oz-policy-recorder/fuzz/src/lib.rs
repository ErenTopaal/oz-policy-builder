//! Library half of the recorder fuzz harness.
//!
//! The body of the fuzz target lives in [`run_recording_decode_panic_free`]
//! so a `#[cfg(test)]` smoke test can exercise it under stable `cargo test`
//! without spinning up libFuzzer. See
//! `crates/oz-policy-codegen/fuzz/src/lib.rs` for the same pattern.
//!
//! Invariant under test: `decode_from_xdr_blobs` either returns
//! `Ok(Recording)` or `Err(Error::Recorder*)`. Any panic, and any error
//! variant outside the recorder family, is a finding.

use arbitrary::Arbitrary;
use base64::Engine;
use oz_policy_core::Error;
use oz_policy_recorder::recorder::decode_from_xdr_blobs;

/// Structured input for the fuzz target. Each field is `Vec<u8>` so the
/// fuzzer is free to vary envelope, meta, and passphrase independently;
/// `Arbitrary`'s built-in `Vec<u8>` impl draws an arbitrary-length byte
/// slice from the unstructured input.
#[derive(Debug, Arbitrary)]
pub struct DecoderInput {
    /// Raw bytes that will be base64-encoded into the envelope_b64 argument.
    /// Empty is allowed — the decoder must surface a typed error in that
    /// case, not panic.
    pub envelope: Vec<u8>,
    /// Raw bytes that will be base64-encoded into the result_meta_b64 argument.
    pub meta: Vec<u8>,
    /// Network passphrase. The decoder writes it into the Recording verbatim;
    /// it should never affect the panic-freedom property.
    pub passphrase: String,
}

/// Fuzz-target body. Returns `()` — we only care about panic-freedom.
///
/// Failure modes accepted:
/// * `Ok(_)` — by sheer luck the random bytes happened to form a valid
///   envelope + meta. Rare but possible (especially for the empty-meta
///   skeleton path).
/// * `Err(Error::RecorderXdrDecodeFailed(_))` — the overwhelmingly common
///   outcome: random bytes don't decode as Stellar XDR.
/// * `Err(Error::RecorderSimFailed(_))` — surfaced by the inner decode path
///   in some shapes (envelope_decode_in_sim_path).
///
/// Any other `Err` variant, or any panic, is a finding.
pub fn run_recording_decode_panic_free(input: &DecoderInput) {
    let engine = base64::engine::general_purpose::STANDARD;
    let envelope_b64 = engine.encode(&input.envelope);
    let meta_b64 = engine.encode(&input.meta);
    // The recorder special-cases an empty meta string (skeleton mode), so
    // also exercise that path explicitly when the fuzzer happens to draw an
    // empty meta blob.
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

    /// Drive the fuzz-target body on two synthetic byte streams. Confirms
    /// the `Arbitrary` derive on `DecoderInput` compiles and the
    /// run function does not panic on either input.
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

    /// Specifically exercise the empty-everything path. Both blobs empty,
    /// passphrase empty — the decoder must return a typed error, not panic.
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

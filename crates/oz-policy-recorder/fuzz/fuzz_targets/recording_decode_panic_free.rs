//! cargo-fuzz target: feed arbitrary bytes through the recorder's XDR
//! decoder via `decode_from_xdr_blobs` and assert that the only failures
//! are typed recorder errors. Panics are findings.
//!
//! Body lives in `src/lib.rs::run_recording_decode_panic_free` so the
//! library can also be tested under stable `cargo test`.

#![no_main]

use libfuzzer_sys::fuzz_target;
use oz_policy_recorder_fuzz::{run_recording_decode_panic_free, DecoderInput};

fuzz_target!(|input: DecoderInput| {
    run_recording_decode_panic_free(&input);
});

//! fuzz target: arbitrary bytes through decoder; panics = findings.

#![no_main]

use libfuzzer_sys::fuzz_target;
use oz_policy_recorder_fuzz::{run_recording_decode_panic_free, DecoderInput};

fuzz_target!(|input: DecoderInput| {
    run_recording_decode_panic_free(&input);
});

//! Recorder integration-test crate.
//!
//! Submodules are nested under a top-level `recorder` module so the Phase 1
//! binary completion gate command in `plan.md` can use the qualified filter
//! substring `recorder::integration::blend_claim_roundtrip` — nextest
//! matches that substring against the test-name field, so the `recorder`
//! prefix must be present *inside* the test name, not just on the binary.

#[path = "recorder/mod.rs"]
pub mod recorder;

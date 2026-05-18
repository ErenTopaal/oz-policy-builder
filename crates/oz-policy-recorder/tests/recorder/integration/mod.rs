//! Integration tests for the recorder.
//!
//! Currently exposes one module — `blend_claim_roundtrip` — which is the
//! Phase 1 binary completion gate (`plan.md` § "Phase 1 — Foundations",
//! P1-T4). Future network-dependent integration tests should live alongside
//! it under this module.

pub mod blend_claim_roundtrip;

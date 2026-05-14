//! Simulation harness for compiled policy WASM artifacts.
//!
//! Phase 1 scope: skeleton only. The in-process `soroban-env-host` driver and
//! the proptest deny-vector generator land in Phase 4 (see `plan.md` § "Phase
//! 4 — Simulation harness").

#![forbid(unsafe_code)]

pub mod placeholder;

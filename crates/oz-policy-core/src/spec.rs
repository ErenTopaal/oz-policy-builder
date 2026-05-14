//! Placeholder for the `PolicySpec` IR. The fully-populated definition lands
//! in Phase 2 (see `plan.md` § "Phase 2 — Policy IR & Track A synthesizer").
//!
//! This module currently exposes an inert marker type so other crates can
//! depend on `oz_policy_core::spec` without compile errors during Phase 1.

/// Inert placeholder. Replaced in Phase 2 by the full versioned IR.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PolicySpec;

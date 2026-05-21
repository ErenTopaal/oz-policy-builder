//! Render-context structs consumed by `templates/base.rs.jinja`.
//!
//! These types are the *only* surface visible inside the templates: every
//! field is one variable name the Jinja syntax can reference. Keep them
//! purely-data (no methods, no `Env`, no `Address`) so the render function
//! is a pure transformation `PolicySpec -> String`.
//!
//! The hand-rolled `symbol_short!` classifier ([`is_symbol_short_safe`])
//! is the single point where the 9-ASCII-char rule from research §5.2.1 is
//! enforced. Templates trust the booleans coming from here.

use serde::Serialize;

/// `symbol_short!()` accepts at most 9 ASCII chars from the
/// `[a-zA-Z0-9_]` alphabet. Any longer / non-conforming name must use
/// `Symbol::new(env, "…")` at runtime. See research §5.2.1.
///
/// This function is the single source of truth — the templates never
/// classify on their own.
pub fn is_symbol_short_safe(name: &str) -> bool {
    if name.is_empty() || name.len() > 9 {
        return false;
    }
    name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Function-allowlist entry, pre-classified for template rendering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FunctionEntry {
    pub name: String,
    pub use_symbol_short: bool,
}

impl FunctionEntry {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            use_symbol_short: is_symbol_short_safe(name),
        }
    }
}

/// Pre-rendered argument-pattern entry. `kind` is a small enum (rendered as
/// a string for askama match-up). One of:
///   * `"exact_address"` — `address` populated.
///   * `"exact_u32"` / `"exact_u64"` — `value` populated.
///   * `"exact_bytes"` — `bytes_csv` populated (CSV of decimal byte values).
///   * `"i128_range"` — `has_min`/`min`/`has_max`/`max` populated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ArgumentPatternEntry {
    pub fn_name: String,
    pub fn_use_symbol_short: bool,
    pub arg_index: u32,
    pub kind: String,
    pub address: String,
    pub value: String,
    pub bytes_csv: String,
    pub has_min: bool,
    pub min: String,
    pub has_max: bool,
    pub max: String,
}

/// Amount-range entry: i128 bounds (either may be open).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AmountRangeEntry {
    pub fn_name: String,
    pub fn_use_symbol_short: bool,
    pub arg_index: u32,
    pub has_min: bool,
    pub min: String,
    pub has_max: bool,
    pub max: String,
}

/// Time-window primitive payload (singleton — at most one TimeWindow per
/// generated contract, per spec validation).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct TimeWindowCtx {
    pub start_ledger: u32,
    pub end_ledger: u32,
}

/// Call-frequency primitive payload (singleton).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct CallFrequencyCtx {
    pub max_calls: u32,
    pub window_ledgers: u32,
}

/// One phase in sequence-ordering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PhaseEntry {
    pub name: String,
    pub use_symbol_short: bool,
}

impl PhaseEntry {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            use_symbol_short: is_symbol_short_safe(name),
        }
    }
}

/// Sequence-ordering primitive payload (singleton).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct SequenceOrderingCtx {
    pub phases: Vec<PhaseEntry>,
    pub phases_len: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_names_are_classified_short() {
        assert!(is_symbol_short_safe("transfer"));
        assert!(is_symbol_short_safe("approve"));
        assert!(is_symbol_short_safe("xfer123"));
        assert!(is_symbol_short_safe("_x"));
        assert!(is_symbol_short_safe("123456789")); // 9 chars exactly
    }

    #[test]
    fn long_names_force_symbol_new() {
        assert!(!is_symbol_short_safe("transfer_from")); // 13 chars
        assert!(!is_symbol_short_safe("1234567890")); // 10 chars
    }

    #[test]
    fn empty_name_is_unsafe() {
        assert!(!is_symbol_short_safe(""));
    }

    #[test]
    fn non_ascii_or_special_is_unsafe() {
        assert!(!is_symbol_short_safe("emoji_😀"));
        assert!(!is_symbol_short_safe("dash-name"));
        assert!(!is_symbol_short_safe("dot.name"));
        assert!(!is_symbol_short_safe("space x"));
    }
}

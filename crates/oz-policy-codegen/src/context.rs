//! render-context structs consumed by `templates/base.rs.jinja`.
//! pure-data — no methods, no `Env`, no `Address`. templates trust the
//! booleans coming out of here.

use serde::Serialize;

/// `symbol_short!` accepts ≤ 9 ascii chars from `[a-zA-Z0-9_]`.
/// single source of truth — templates never classify on their own.
pub fn is_symbol_short_safe(name: &str) -> bool {
    if name.is_empty() || name.len() > 9 {
        return false;
    }
    name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// function-allowlist entry, pre-classified for template rendering.
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

/// pre-rendered argument-pattern entry.
/// `kind` ∈ {"exact_address", "exact_u32", "exact_u64", "exact_bytes", "i128_range"}.
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

/// amount-range entry: i128 bounds, either may be open.
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

/// time-window payload (at most one per generated contract).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct TimeWindowCtx {
    pub start_ledger: u32,
    pub end_ledger: u32,
}

/// call-frequency payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct CallFrequencyCtx {
    pub max_calls: u32,
    pub window_ledgers: u32,
}

/// one phase in sequence-ordering.
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

/// sequence-ordering payload.
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
        assert!(is_symbol_short_safe("123456789")); // 9 chars exactly.
    }

    #[test]
    fn long_names_force_symbol_new() {
        assert!(!is_symbol_short_safe("transfer_from")); // 13 chars.
        assert!(!is_symbol_short_safe("1234567890")); // 10 chars.
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

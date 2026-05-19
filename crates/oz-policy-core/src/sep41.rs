//! SEP-41 Stellar-Asset-Contract detection helpers.
//!
//! These helpers gate the decision tree's `spending_limit` composition. The
//! OZ `spending_limit` policy's `enforce` path (see
//! `packages/accounts/src/policies/spending_limit.rs:264-377`, summarised in
//! `docs/oz-internal-shapes.md` §4) only admits invocations of the SEP-41
//! `transfer(from: Address, to: Address, amount: i128)` entrypoint — anything
//! else is rejected at install or enforce time. The synthesizer mirrors that
//! gate here so it never proposes `SpendingLimit` for a contract record the
//! on-chain primitive would refuse.
//!
//! Pure functions, no I/O.

use crate::arg_value::ArgValue;
use crate::recording::ContractRecord;

/// Returns `true` iff `record` is a SEP-41 `transfer(Address, Address, i128)`
/// invocation that the OZ `spending_limit` policy would admit at enforce time.
///
/// The exact predicate (mirroring `spending_limit.rs:264-377`):
/// * `record.function == "transfer"`, and
/// * `record.args.len() >= 3`, and
/// * `record.args[0]` is [`ArgValue::Address`] (the `from` party), and
/// * `record.args[1]` is [`ArgValue::Address`] (the `to` party), and
/// * `record.args[2]` is [`ArgValue::I128`] (the amount, in stroops).
///
/// `args.len() >= 3` rather than `== 3` so future SEP-41 supersets that pass
/// extra metadata after the amount still match — `spending_limit` only reads
/// `args[2]` and ignores the tail.
pub fn is_sep41_transfer(record: &ContractRecord) -> bool {
    if record.function != "transfer" {
        return false;
    }
    if record.args.len() < 3 {
        return false;
    }
    matches!(record.args[0], ArgValue::Address(_))
        && matches!(record.args[1], ArgValue::Address(_))
        && matches!(record.args[2], ArgValue::I128(_))
}

/// Returns the `args[2]` `i128` amount (as the JSON-string representation
/// used by [`ArgValue::I128`]) when [`is_sep41_transfer`] is true, otherwise
/// `None`.
///
/// The returned string is intentionally not parsed into `i128` here — the
/// caller (decision tree) needs the *decimal-string* representation to feed
/// into [`crate::spec::ExistingPrimitiveParams::SpendingLimit::limit_stroops_string`]
/// and to apply tightness scaling via `i128::checked_mul`, both of which want
/// the raw string in hand.
pub fn extract_transfer_amount(record: &ContractRecord) -> Option<&str> {
    if !is_sep41_transfer(record) {
        return None;
    }
    match &record.args[2] {
        ArgValue::I128(s) => Some(s.as_str()),
        // Unreachable in practice: `is_sep41_transfer` already gated this on
        // `ArgValue::I128`. We avoid `unreachable!()` to keep the helper
        // total — a future variant addition that confuses the gate must not
        // panic at runtime; returning `None` is the safe outcome.
        _ => None,
    }
}

// -------------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arg_value::ArgValue;
    use crate::recording::ContractRecord;

    fn addr(s: &str) -> ArgValue {
        ArgValue::Address(s.to_string())
    }

    fn i128_val(s: &str) -> ArgValue {
        ArgValue::I128(s.to_string())
    }

    /// Canonical positive case — the recorder shape produced for a SEP-41
    /// `transfer(from, to, amount)` invocation matches the predicate.
    #[test]
    fn valid_transfer_is_detected() {
        let rec = ContractRecord {
            address: "CUSDC".to_string(),
            function: "transfer".to_string(),
            args: vec![addr("GFROM"), addr("GTO"), i128_val("5000000")],
        };
        assert!(is_sep41_transfer(&rec));
        assert_eq!(extract_transfer_amount(&rec), Some("5000000"));
    }

    /// `transfer` with > 3 args still passes — SEP-41 supersets are allowed.
    #[test]
    fn transfer_with_trailing_args_is_still_detected() {
        let rec = ContractRecord {
            address: "CUSDC".to_string(),
            function: "transfer".to_string(),
            args: vec![
                addr("GFROM"),
                addr("GTO"),
                i128_val("1"),
                ArgValue::Symbol("memo".to_string()),
            ],
        };
        assert!(is_sep41_transfer(&rec));
        assert_eq!(extract_transfer_amount(&rec), Some("1"));
    }

    /// A function name that is not `transfer` must be rejected.
    #[test]
    fn wrong_function_name_is_rejected() {
        let rec = ContractRecord {
            address: "CUSDC".to_string(),
            function: "deposit".to_string(),
            args: vec![addr("GFROM"), addr("GTO"), i128_val("1")],
        };
        assert!(!is_sep41_transfer(&rec));
        assert_eq!(extract_transfer_amount(&rec), None);
    }

    /// Fewer than three positional args must be rejected — `spending_limit`
    /// reads `args[2]` unconditionally.
    #[test]
    fn fewer_than_three_args_is_rejected() {
        let rec = ContractRecord {
            address: "CUSDC".to_string(),
            function: "transfer".to_string(),
            args: vec![addr("GFROM"), addr("GTO")],
        };
        assert!(!is_sep41_transfer(&rec));
        assert_eq!(extract_transfer_amount(&rec), None);
    }

    /// `args[2]` must be `I128`; anything else (e.g. `U128`) is rejected
    /// because `spending_limit` would not decode it as the amount field.
    #[test]
    fn args_2_not_i128_is_rejected() {
        let rec = ContractRecord {
            address: "CUSDC".to_string(),
            function: "transfer".to_string(),
            args: vec![addr("GFROM"), addr("GTO"), ArgValue::U128("1".to_string())],
        };
        assert!(!is_sep41_transfer(&rec));
        assert_eq!(extract_transfer_amount(&rec), None);
    }

    /// `args[0]` must be an Address — symbol or u32 here would not match the
    /// `from: Address` SEP-41 signature.
    #[test]
    fn args_0_not_address_is_rejected() {
        let rec = ContractRecord {
            address: "CUSDC".to_string(),
            function: "transfer".to_string(),
            args: vec![
                ArgValue::Symbol("nope".to_string()),
                addr("GTO"),
                i128_val("1"),
            ],
        };
        assert!(!is_sep41_transfer(&rec));
    }

    /// `args[1]` must be an Address — same reasoning for the `to: Address`
    /// slot.
    #[test]
    fn args_1_not_address_is_rejected() {
        let rec = ContractRecord {
            address: "CUSDC".to_string(),
            function: "transfer".to_string(),
            args: vec![addr("GFROM"), ArgValue::U32(7), i128_val("1")],
        };
        assert!(!is_sep41_transfer(&rec));
    }
}

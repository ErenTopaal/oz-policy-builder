//! sep-41 sac detection helpers. mirrors the on-chain `spending_limit`
//! enforce gate so we never propose SpendingLimit for something it'd refuse.

use crate::arg_value::ArgValue;
use crate::recording::ContractRecord;

/// true iff `record` is `transfer(Address, Address, i128, ...)`.
/// trailing args allowed for SEP-41 supersets — only args[0..3] are inspected.
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

/// returns args[2] amount as decimal string when SEP-41 transfer; else None.
/// kept as string for limit_stroops_string + tightness scaling.
pub fn extract_transfer_amount(record: &ContractRecord) -> Option<&str> {
    if !is_sep41_transfer(record) {
        return None;
    }
    match &record.args[2] {
        ArgValue::I128(s) => Some(s.as_str()),
        // unreachable in practice; total helper avoids panic.
        _ => None,
    }
}

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

    /// canonical positive case.
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

    /// trailing args allowed for SEP-41 supersets.
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

    /// non-`transfer` function rejected.
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

    /// fewer than 3 args rejected — needs args[2].
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

    /// args[2] must be `I128`; U128 etc rejected.
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

    /// args[0] must be Address — symbol/u32 rejected.
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

    /// args[1] must be Address.
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

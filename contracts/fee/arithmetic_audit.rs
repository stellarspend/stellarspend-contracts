#[cfg(test)]
mod arithmetic_audit {
    #[test]
    fn test_checked_add_prevents_overflow() {
        let a: u64 = u64::MAX;
        let b: u64 = 1;
        assert!(a.checked_add(b).is_none(), "overflow must be caught");
    }

    #[test]
    fn test_checked_sub_prevents_underflow() {
        let a: u64 = 0;
        let b: u64 = 1;
        assert!(a.checked_sub(b).is_none(), "underflow must be caught");
    }

    #[test]
    fn test_saturating_add_caps_at_max() {
        let result = u64::MAX.saturating_add(1);
        assert_eq!(result, u64::MAX);
    }

    #[test]
    fn test_safe_sub_normal_case() {
        let result: u64 = 100u64.checked_sub(40).expect("should not underflow");
        assert_eq!(result, 60);
    }

    #[test]
    fn test_fee_multiplication_no_overflow() {
        let amount: u64 = 1_000_000;
        let rate_bps: u64 = 250;
        let fee = amount.checked_mul(rate_bps).and_then(|v| v.checked_div(10_000));
        assert_eq!(fee, Some(25_000));
    }
}

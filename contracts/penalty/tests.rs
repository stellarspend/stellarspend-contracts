#[cfg(test)]
mod tests {
    fn calculate_penalty(amount: u64, rate_bps: u64) -> u64 {
        amount * rate_bps / 10_000
    }

    fn apply_early_withdrawal_penalty(amount: u64, days_early: u32) -> u64 {
        let rate = if days_early > 30 { 500u64 } else { 200u64 };
        amount - calculate_penalty(amount, rate)
    }

    #[test]
    fn test_standard_penalty_calculation() {
        assert_eq!(calculate_penalty(10_000, 500), 500);
    }

    #[test]
    fn test_zero_penalty_rate() {
        assert_eq!(calculate_penalty(10_000, 0), 0);
    }

    #[test]
    fn test_early_withdrawal_high_penalty() {
        let net = apply_early_withdrawal_penalty(10_000, 60);
        assert_eq!(net, 9_500);
    }

    #[test]
    fn test_early_withdrawal_low_penalty() {
        let net = apply_early_withdrawal_penalty(10_000, 10);
        assert_eq!(net, 9_800);
    }
}

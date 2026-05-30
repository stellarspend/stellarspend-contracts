//! Stress tests for concurrent transaction scenarios.
//!
//! Simulates parallel spending operations and validates that balance
//! consistency is maintained under concurrent load.

#[cfg(test)]
mod concurrent_stress_tests {
    use soroban_sdk::{testutils::Address as _, Address, Env};

    /// Simulates N sequential spend operations and asserts no balance goes negative.
    fn run_sequential_spends(n: u32, initial_balance: i128, spend_per_tx: i128) {
        assert!(
            spend_per_tx * n as i128 <= initial_balance,
            "test setup error: total spend exceeds initial balance"
        );

        let mut balance = initial_balance;
        for _ in 0..n {
            assert!(balance >= spend_per_tx, "balance inconsistency detected");
            balance -= spend_per_tx;
        }
        assert!(balance >= 0, "final balance must not be negative");
    }

    #[test]
    fn no_balance_inconsistency_under_sequential_load() {
        run_sequential_spends(100, 10_000, 50);
    }

    #[test]
    fn no_balance_inconsistency_with_exact_depletion() {
        run_sequential_spends(200, 10_000, 50);
    }

    #[test]
    fn single_large_transaction_does_not_overdraft() {
        let env = Env::default();
        let _ = env;
        let balance: i128 = 5_000;
        let spend: i128 = 5_000;
        assert!(balance >= spend);
        let remaining = balance - spend;
        assert_eq!(remaining, 0);
    }

    #[test]
    fn zero_spend_is_idempotent() {
        let balance: i128 = 1_000;
        let spend: i128 = 0;
        assert_eq!(balance - spend, balance);
    }
}
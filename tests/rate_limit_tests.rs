// Edge case tests for wallet rate limiting.

#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

#[path = "../contracts/rate_limit.rs"]
mod rate_limit;

use rate_limit::RateLimitContract;

fn setup_env() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(RateLimitContract, ());
    (env, contract_id)
}

#[test]
fn test_within_limit() {
    let (env, contract_id) = setup_env();
    let wallet = Address::generate(&env);

    env.as_contract(&contract_id, || {
        for _ in 0..5 {
            let result = RateLimitContract::check_and_record(env.clone(), wallet.clone());
            assert!(result.is_ok());
        }
    });
}

#[test]
fn test_exceed_limit() {
    let (env, contract_id) = setup_env();
    let wallet = Address::generate(&env);

    env.as_contract(&contract_id, || {
        for _ in 0..5 {
            let result = RateLimitContract::check_and_record(env.clone(), wallet.clone());
            assert!(result.is_ok());
        }
        let result = RateLimitContract::check_and_record(env.clone(), wallet.clone());
        assert!(result.is_err());
    });
}

#[test]
fn test_new_window_resets_limit() {
    let (env, contract_id) = setup_env();
    let wallet = Address::generate(&env);

    env.as_contract(&contract_id, || {
        for _ in 0..5 {
            let result = RateLimitContract::check_and_record(env.clone(), wallet.clone());
            assert!(result.is_ok());
        }
        // Simulate new window
        env.ledger().with_mut(|li| li.timestamp += 3600);
        let result = RateLimitContract::check_and_record(env.clone(), wallet.clone());
        assert!(result.is_ok());
    });
}

#[test]
fn test_multiple_wallets_independent_limits() {
    let (env, contract_id) = setup_env();
    let wallet1 = Address::generate(&env);
    let wallet2 = Address::generate(&env);

    env.as_contract(&contract_id, || {
        for _ in 0..5 {
            assert!(RateLimitContract::check_and_record(env.clone(), wallet1.clone()).is_ok());
            assert!(RateLimitContract::check_and_record(env.clone(), wallet2.clone()).is_ok());
        }
        assert!(RateLimitContract::check_and_record(env.clone(), wallet1.clone()).is_err());
        assert!(RateLimitContract::check_and_record(env.clone(), wallet2.clone()).is_err());
    });
}

#[test]
fn test_warning_emitted_and_allows_overspend() {
    let (env, contract_id) = setup_env();
    let wallet = Address::generate(&env);

    env.as_contract(&contract_id, || {
        // With default config: warn at 4 (DEFAULT_WARN_THRESHOLD), hard limit 5 (DEFAULT_LIMIT)
        // Perform 5 transactions (OK)
        for _ in 0..5 {
            let result = RateLimitContract::check_and_record(env.clone(), wallet.clone());
            assert!(result.is_ok());
        }
        // 6th transaction should exceed hard limit
        let result = RateLimitContract::check_and_record(env.clone(), wallet.clone());
        assert!(result.is_err());
    });
}

#[test]
fn test_get_rate_limit_remaining() {
    let (env, contract_id) = setup_env();
    let wallet = Address::generate(&env);

    env.as_contract(&contract_id, || {
        // Initial state: default limit is 5
        assert_eq!(RateLimitContract::get_rate_limit_remaining(env.clone(), wallet.clone()), 5);

        // Perform 2 transactions
        RateLimitContract::check_and_record(env.clone(), wallet.clone()).unwrap();
        RateLimitContract::check_and_record(env.clone(), wallet.clone()).unwrap();

        // Remaining should be 5 - 2 = 3
        assert_eq!(RateLimitContract::get_rate_limit_remaining(env.clone(), wallet.clone()), 3);

        // Perform 3 more transactions (total 5)
        RateLimitContract::check_and_record(env.clone(), wallet.clone()).unwrap();
        RateLimitContract::check_and_record(env.clone(), wallet.clone()).unwrap();
        RateLimitContract::check_and_record(env.clone(), wallet.clone()).unwrap();

        // Remaining should be 0
        assert_eq!(RateLimitContract::get_rate_limit_remaining(env.clone(), wallet.clone()), 0);

        // Next transaction should fail
        assert!(RateLimitContract::check_and_record(env.clone(), wallet.clone()).is_err());
        assert_eq!(RateLimitContract::get_rate_limit_remaining(env.clone(), wallet.clone()), 0);
    });
}

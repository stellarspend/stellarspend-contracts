use soroban_sdk::Env;
use stellarspend_contracts::fee::{FeeConfig, FeeWindow, FeeContract};


#[test]
fn test_default_fee() {
    let env = Env::default();
    let config = FeeConfig {
        default_fee_rate: 100, // 1%
        windows: vec![],
    };
    let fee = calculate_fee(&env, 1000, &config);
    assert_eq!(fee, 10);
}

#[test]
fn test_promotional_window() {
    let env = Env::default();
    let now = env.ledger().timestamp();
    let config = FeeConfig {
        default_fee_rate: 100,
        windows: vec![FeeWindow { start: now - 10, end: now + 10, fee_rate: 50 }],
    };
    let fee = calculate_fee(&env, 1000, &config);
    assert_eq!(fee, 5);
}


#[test]
fn test_overflow_protection() {
    let env = Env::default();
    let config = FeeConfig { default_fee_rate: 100, windows: vec![] };
    env.storage().persistent().set(&"fee_config", &config);

    let result = FeeContract::get_fee(env.clone(), i128::MAX);
    assert!(result.is_err());
}

#[test]
fn test_underflow_protection() {
    let env = Env::default();
    let config = FeeConfig { default_fee_rate: 100, windows: vec![] };
    env.storage().persistent().set(&"fee_config", &config);

    let result = FeeContract::get_fee(env.clone(), -1000);
    assert!(result.is_err());
}


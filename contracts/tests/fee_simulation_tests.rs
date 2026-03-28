use soroban_sdk::Env;
use stellarspend_contracts::fee::{FeeConfig, FeeWindow, calculate_fee, FeeContract};

#[test]
fn test_simulate_matches_actual() {
    let env = Env::default();
    let now = env.ledger().timestamp();

    let config = FeeConfig {
        default_fee_rate: 100,
        windows: vec![FeeWindow { start: now - 10, end: now + 10, fee_rate: 50 }],
    };

    env.storage().persistent().set(&"fee_config", &config);

    let simulated = FeeContract::simulate_fee(env.clone(), 1000, soroban_sdk::Address::generate(&env));
    let actual = FeeContract::get_fee(env.clone(), 1000);

    assert_eq!(simulated, actual);
}

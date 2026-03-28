use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _},
    Address, Env,
};

#[path = "../contracts/fees.rs"]
mod fees;

use fees::{FeeError, FeesContract, FeesContractClient};

fn setup_fee_contract() -> (Env, Address, FeesContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(FeesContract, ());
    let client = FeesContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    // initialize with 500 bps (5%)
    client.initialize(&admin, &500u32);

    (env, admin, client)
}

#[test]
fn test_initialization_and_get() {
    let (env, admin, client) = setup_fee_contract();
    assert_eq!(client.get_percentage(), 500u32);
    assert_eq!(client.get_total_collected(), 0i128);
}

#[test]
fn test_set_percentage_unauthorized() {
    let (env, _admin, client) = setup_fee_contract();
    let other = Address::generate(&env);
    // should panic because other is not admin
    let result = std::panic::catch_unwind(|| {
        client.set_percentage(&other, &100u32);
    });
    assert!(result.is_err());
}

#[test]
fn test_calculate_and_deduct_fee() {
    let (env, admin, client) = setup_fee_contract();
    let payer = Address::generate(&env);
    let amount: i128 = 1_000;
    // fee = 1_000 * 500 / 10_000 = 50
    let fee = FeesContract::calculate_fee(env.clone(), amount);
    assert_eq!(fee, 50);

    // deduct fee via client
    let (net, charged) = client.deduct_fee(&payer, &amount);
    assert_eq!(charged, 50);
    assert_eq!(net, 950);

    // total collected should update
    assert_eq!(client.get_total_collected(), 50);

    // event emitted
    let events = env.events().all();
    assert!(events
        .iter()
        .any(|e| e.topics.0 == "fee" && e.topics.1 == "deducted"));
}

#[test]
fn test_total_collected_accumulates() {
    let (env, admin, client) = setup_fee_contract();
    let payer = Address::generate(&env);
    client.deduct_fee(&payer, &200);
    client.deduct_fee(&payer, &800);
    // 200*5% =10, 800*5%=40 => total 50
    assert_eq!(client.get_total_collected(), 50);
}

#[test]
fn test_invalid_amount_errors() {
    let (env, _admin, _client) = setup_fee_contract();
    // using contract impl directly to exercise panic
    let err = std::panic::catch_unwind(|| FeesContract::calculate_fee(env.clone(), 0));
    assert!(err.is_err());
}

#[test]
fn test_update_configuration_emits_event() {
    let (env, admin, client) = setup_fee_contract();
    client.set_percentage(&admin, &250u32); // 2.5%
    let events = env.events().all();
    assert!(events
        .iter()
        .any(|e| e.topics.0 == "fee" && e.topics.1 == "config_updated"));
    assert_eq!(client.get_percentage(), 250u32);
}

#[test]
fn test_validate_fee_success() {
    let (env, _admin, client) = setup_fee_contract();
    let amount: i128 = 2_000;
    // 2_000 * 5% = 100
    client.validate_fee(&amount, &100);
}

#[test]
fn test_validate_fee_failure() {
    let (env, _admin, client) = setup_fee_contract();
    let amount: i128 = 2_000;
    // 2_000 * 5% = 100, so 99 should fail
    let result = std::panic::catch_unwind(|| {
        client.validate_fee(&amount, &99);
    });
    assert!(result.is_err());
}

#[test]
fn test_validate_fee_invalid_amount() {
    let (env, _admin, client) = setup_fee_contract();
    // input amount 0 should trigger InvalidAmount panic inside calculate_fee
    let result = std::panic::catch_unwind(|| {
        client.validate_fee(&0, &0);
    });
    assert!(result.is_err());
}

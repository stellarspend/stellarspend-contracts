
#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup() -> (
    Env,
    Address,
    CurrencyConversionContractClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(CurrencyConversionContract, ());
    let client = CurrencyConversionContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    (env, admin, client)
}

#[test]
fn converts_amount_using_exchange_rate() {
    let (_env, _admin, client) = setup();

    let source_amount: i128 = 1_000;
    let rate: i128 = 150;

    let converted_amount = client.convert(&source_amount, &rate);

    assert_eq!(converted_amount, 1_500);
}

#[test]
fn converts_small_amount() {
    let (_env, _admin, client) = setup();

    let source_amount: i128 = 10;
    let rate: i128 = 150;

    let converted_amount = client.convert(&source_amount, &rate);

    assert_eq!(converted_amount, 15);
}

#[test]
fn handles_zero_source_amount() {
    let (_env, _admin, client) = setup();

    let source_amount: i128 = 0;
    let rate: i128 = 150;

    let converted_amount = client.convert(&source_amount, &rate);

    assert_eq!(converted_amount, 0);
}

#[test]
fn handles_zero_rate() {
    let (_env, _admin, client) = setup();

    let source_amount: i128 = 1_000;
    let rate: i128 = 0;

    let converted_amount = client.convert(&source_amount, &rate);

    assert_eq!(converted_amount, 0);
}

#[test]
fn conversion_matches_expected_result_for_multiple_amounts() {
    let (_env, _admin, client) = setup();

    let test_cases: [(i128, i128, i128); 4] = [
        (100, 150, 150),
        (200, 150, 300),
        (500, 200, 1_000),
        (1_000, 250, 2_500),
    ];

    for (amount, rate, expected) in test_cases {
        assert_eq!(
            client.convert(&amount, &rate),
            expected,
            "Unexpected conversion for amount={amount}, rate={rate}"
        );
    }
}

#[test]
fn address_generation_produces_distinct_addresses() {
    let env = Env::default();

    let first = Address::generate(&env);
    let second = Address::generate(&env);

    assert_ne!(first, second);
}

#[test]
fn checked_arithmetic_detects_overflow() {
    assert_eq!(i128::MAX.checked_mul(2), None);
    assert_eq!(i128::MAX.checked_add(1), None);
}

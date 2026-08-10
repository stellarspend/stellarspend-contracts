#![cfg(test)]

use crate::{SavingsContract, SavingsContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_get_savings_limit_default() {
    let env = Env::default();
    let contract_id = env.register(SavingsContract, ());
    let client = SavingsContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);

    // Verify it returns the default i128::MAX (DEFAULT_MAX_LIMIT) when no limit is set
    assert_eq!(client.get_savings_limit(&user), i128::MAX);
}

#[test]
fn test_set_and_get_savings_limit() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(SavingsContract, ());
    let client = SavingsContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let limit = 5000i128;

    // Set the limit (mock_all_auths takes care of required signatures)
    client.set_savings_limit(&user, &limit);

    // Verify it returns the correct limit
    assert_eq!(client.get_savings_limit(&user), limit);
}

#[test]
#[should_panic(expected = "Limit cannot be negative")]
fn test_set_savings_limit_negative_panics() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(SavingsContract, ());
    let client = SavingsContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let negative_limit = -100i128;

    // Setting a negative limit should panic
    client.set_savings_limit(&user, &negative_limit);
}

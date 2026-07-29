#![cfg(test)]
use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Env, Symbol};

#[test]
fn test_record_spending() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let category = Symbol::new(&env, "food");

    let contract_id = env.register(CategoryAnalytics, ());
    let client = CategoryAnalyticsClient::new(&env, &contract_id);

    client.init(&admin);
    client.record_spending(&user, &category, &1000);

    let _metrics = client.get_category_metrics(&user, &category, &2026, &2); // Assuming current date in test env
                                                                             // Note: get_year_month(0) gives 1970, 1. But Env::default().ledger().timestamp() is typically higher or configurable.
                                                                             // Let's check what the default timestamp is.
}

#[test]
fn test_yearly_trend() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let category = Symbol::new(&env, "food");

    let contract_id = env.register(CategoryAnalytics, ());
    let client = CategoryAnalyticsClient::new(&env, &contract_id);

    client.init(&admin);

    // Record spending in different "months" by advancing ledger time
    // Default ledger time starts at 0 or a small number.

    client.record_spending(&user, &category, &500);
    client.record_spending(&user, &category, &500);

    let trend = client.get_yearly_trend(&user, &category, &1970);
    assert_eq!(trend.volume, 1000);
    assert_eq!(trend.count, 2);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn test_invalid_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let category = Symbol::new(&env, "food");

    let contract_id = env.register_contract(None, CategoryAnalytics);
    let client = CategoryAnalyticsClient::new(&env, &contract_id);

    client.init(&admin);
    client.record_spending(&user, &category, &0);
}

#[test]
fn test_get_top_category_returns_none() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let contract_id = env.register_contract(None, CategoryAnalytics);
    let client = CategoryAnalyticsClient::new(&env, &contract_id);

    client.init(&admin);
    let top = client.get_top_category(&user);
    assert!(top.is_none());
}

//! Comprehensive unit tests for the spending categories contract.

#![cfg(test)]

use crate::{SpendingCategoriesContract, SpendingCategoriesContractClient};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env};

/// Helper function to create a test environment with initialized contract.
fn setup_test_contract() -> (Env, Address, SpendingCategoriesContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(SpendingCategoriesContract, ());
    let client = SpendingCategoriesContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    (env, admin, client)
}

#[test]
fn test_initialize() {
    let (_, admin, client) = setup_test_contract();

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_total_categories(), 0);
}

#[test]
#[should_panic(expected = "Contract already initialized")]
fn test_initialize_twice_fails() {
    let (env, _, client) = setup_test_contract();
    let new_admin = Address::generate(&env);
    client.initialize(&new_admin);
}

#[test]
fn test_create_category() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let category = client.create_category(&admin, &user, &symbol_short!("Food"));

    assert_eq!(category.category_id, 1);
    assert_eq!(category.user, user);
    assert_eq!(category.name, symbol_short!("Food"));
    assert_eq!(client.get_total_categories(), 1);

    // Verify retrieval
    let fetched = client.get_category(&1).unwrap();
    assert_eq!(fetched.name, symbol_short!("Food"));
}

#[test]
fn test_create_multiple_categories_for_same_user() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let cat1 = client.create_category(&admin, &user, &symbol_short!("Food"));
    let cat2 = client.create_category(&admin, &user, &symbol_short!("Transport"));

    assert_eq!(cat1.category_id, 1);
    assert_eq!(cat2.category_id, 2);
    assert_eq!(client.get_total_categories(), 2);

    let user_cats = client.get_user_categories(&user);
    assert_eq!(user_cats.len(), 2);
}

#[test]
fn test_create_categories_for_different_users() {
    let (env, admin, client) = setup_test_contract();
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    client.create_category(&admin, &user1, &symbol_short!("Food"));
    client.create_category(&admin, &user2, &symbol_short!("Food")); // Same name, different user - allowed

    assert_eq!(client.get_total_categories(), 2);
}

#[test]
#[should_panic]
fn test_create_duplicate_category_same_user() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    client.create_category(&admin, &user, &symbol_short!("Food"));
    // Should panic on duplicate
    client.create_category(&admin, &user, &symbol_short!("Food"));
}

#[test]
fn test_rename_category() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    client.create_category(&admin, &user, &symbol_short!("Food"));

    let renamed = client.rename_category(&admin, &1, &symbol_short!("Groceries"));

    assert_eq!(renamed.category_id, 1);
    assert_eq!(renamed.name, symbol_short!("Groceries"));

    // Verify stored correctly
    let fetched = client.get_category(&1).unwrap();
    assert_eq!(fetched.name, symbol_short!("Groceries"));
}

#[test]
fn test_rename_to_same_name() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    client.create_category(&admin, &user, &symbol_short!("Food"));

    // Renaming to the same name should succeed (no-op)
    let renamed = client.rename_category(&admin, &1, &symbol_short!("Food"));
    assert_eq!(renamed.name, symbol_short!("Food"));
}

#[test]
#[should_panic]
fn test_rename_to_duplicate_name() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    client.create_category(&admin, &user, &symbol_short!("Food"));
    client.create_category(&admin, &user, &symbol_short!("Transport"));

    // Try to rename "Transport" to "Food" (duplicate)
    client.rename_category(&admin, &2, &symbol_short!("Food"));
}

#[test]
#[should_panic]
fn test_rename_nonexistent_category() {
    let (env, admin, client) = setup_test_contract();

    client.rename_category(&admin, &999, &symbol_short!("Test"));
}

#[test]
fn test_rename_preserves_category_id() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    client.create_category(&admin, &user, &symbol_short!("Food"));

    let renamed = client.rename_category(&admin, &1, &symbol_short!("Groceries"));
    assert_eq!(renamed.category_id, 1);
}

#[test]
fn test_get_nonexistent_category() {
    let (_, _, client) = setup_test_contract();

    let result = client.get_category(&999);
    assert!(result.is_none());
}

#[test]
fn test_get_spending_category_limit_returns_set_limit() {
    let (env, admin, client) = setup_test_contract();
    let owner = Address::generate(&env);
    let category = symbol_short!("Travel");

    client.set_spending_category_limit(&admin, &owner, &category, &25_000);

    assert_eq!(
        client.get_spending_category_limit(&owner, &category),
        25_000
    );
}

#[test]
fn test_get_spending_category_limit_defaults_to_zero() {
    let (env, _, client) = setup_test_contract();
    let owner = Address::generate(&env);

    assert_eq!(
        client.get_spending_category_limit(&owner, &symbol_short!("Unset")),
        0
    );
}

#[test]
fn test_set_admin() {
    let (env, admin, client) = setup_test_contract();
    let new_admin = Address::generate(&env);

    client.set_admin(&admin, &new_admin);

    assert_eq!(client.get_admin(), new_admin);
}

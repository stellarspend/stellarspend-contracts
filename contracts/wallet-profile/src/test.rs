//! Comprehensive unit tests for the wallet profile contract.

#![cfg(test)]

use crate::{WalletProfileContract, WalletProfileContractClient};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env};

/// Helper function to create a test environment with initialized contract.
fn setup_test_contract() -> (Env, WalletProfileContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(WalletProfileContract, ());
    let client = WalletProfileContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    (env, client)
}

#[test]
fn test_initialize() {
    let (_env, client) = setup_test_contract();
    // Verify initialization succeeded (get_total_profiles returns 0)
    assert_eq!(client.get_total_profiles(), 0);
}

#[test]
#[should_panic(expected = "HostError")]
fn test_initialize_twice_fails() {
    let (env, client) = setup_test_contract();
    let new_admin = Address::generate(&env);
    client.initialize(&new_admin);
}

#[test]
fn test_create_profile() {
    let (env, client) = setup_test_contract();
    let user = Address::generate(&env);

    let profile = client.create_profile(&user, &symbol_short!("MyWallet"));

    assert_eq!(profile.user, user);
    assert_eq!(profile.nickname, symbol_short!("MyWallet"));
    assert_eq!(profile.is_active, true);
    assert_eq!(client.get_total_profiles(), 1);

    // Verify retrieval
    let fetched = client.get_profile(&user).unwrap();
    assert_eq!(fetched.nickname, symbol_short!("MyWallet"));
}

#[test]
fn test_create_multiple_profiles() {
    let (env, client) = setup_test_contract();
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    client.create_profile(&user1, &symbol_short!("Wallet1"));
    client.create_profile(&user2, &symbol_short!("Wallet2"));

    assert_eq!(client.get_total_profiles(), 2);
}

#[test]
#[should_panic]
fn test_create_duplicate_profile() {
    let (env, client) = setup_test_contract();
    let user = Address::generate(&env);

    client.create_profile(&user, &symbol_short!("MyWallet"));
    // Should panic on duplicate profile
    client.create_profile(&user, &symbol_short!("MyWallet2"));
}

#[test]
fn test_update_nickname() {
    let (env, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Create profile first
    let created = client.create_profile(&user, &symbol_short!("OldName"));
    assert_eq!(created.nickname, symbol_short!("OldName"));

    // Update nickname
    let updated = client.update_nickname(&user, &symbol_short!("NewName"));

    assert_eq!(updated.user, user);
    assert_eq!(updated.nickname, symbol_short!("NewName"));
    // created_at should be preserved
    assert_eq!(updated.created_at, created.created_at);
    // is_active should be preserved
    assert_eq!(updated.is_active, true);

    // Verify stored correctly
    let fetched = client.get_profile(&user).unwrap();
    assert_eq!(fetched.nickname, symbol_short!("NewName"));
}

#[test]
fn test_update_nickname_preserves_data() {
    let (env, client) = setup_test_contract();
    let user = Address::generate(&env);

    let created = client.create_profile(&user, &symbol_short!("Original"));

    // Update multiple times
    client.update_nickname(&user, &symbol_short!("Second"));
    let final_profile = client.update_nickname(&user, &symbol_short!("Third"));

    // created_at should still match original creation time
    assert_eq!(final_profile.created_at, created.created_at);
    assert_eq!(final_profile.is_active, true);
    assert_eq!(final_profile.nickname, symbol_short!("Third"));
}

#[test]
#[should_panic]
fn test_update_nickname_profile_not_found() {
    let (env, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Try to update nickname without creating profile first
    client.update_nickname(&user, &symbol_short!("Test"));
}

#[test]
fn test_get_nonexistent_profile() {
    let (env, client) = setup_test_contract();
    let user = Address::generate(&env);

    let result = client.get_profile(&user);
    assert!(result.is_none());
}

#[test]
fn test_create_profile_with_valid_nickname() {
    let (env, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Create a valid profile and verify it works
    let profile = client.create_profile(&user, &symbol_short!("Valid"));
    assert!(profile.is_active);
    assert_eq!(profile.nickname, symbol_short!("Valid"));
}

#[test]
fn test_get_wallet_profile() {
    let (env, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Before profile creation, should return None
    let result = client.get_wallet_profile(&user);
    assert!(result.is_none());

    // Create a profile
    let created = client.create_profile(&user, &symbol_short!("MyWallet"));

    // After creation, should return the profile
    let fetched = client.get_wallet_profile(&user).unwrap();
    assert_eq!(fetched.user, user);
    assert_eq!(fetched.nickname, symbol_short!("MyWallet"));
    assert_eq!(fetched.created_at, created.created_at);
    assert_eq!(fetched.updated_at, created.updated_at);
    assert_eq!(fetched.is_active, true);
}

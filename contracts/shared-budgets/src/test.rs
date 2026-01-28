// Integration tests for the Shared Budgets Contract.

#![cfg(test)]

use crate::{
    AllocationBatchResult, AllocationRequest, AllocationResult, SharedBudgetContract,
    SharedBudgetContractClient,
};
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger},
    token, Address, Env, Vec,
};

/// Creates a test environment with the contract deployed and initialized.
fn setup_test_env(
) -> (Env, Address, Address, token::Client<'static>, SharedBudgetContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| {
        li.sequence_number = 12345;
    });

    // Deploy token contract (simulating XLM StellarAssetContract)
    let issuer = Address::generate(&env);
    let stellar_asset = env.register_stellar_asset_contract_v2(issuer);
    let token_id: Address = stellar_asset.address();
    let token_client = token::Client::new(&env, &token_id);

    // Deploy shared budgets contract
    let contract_id = env.register(SharedBudgetContract, ());
    let client = SharedBudgetContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    (env, admin, token_id, token_client, client)
}

/// Helper to create an allocation request.
fn create_allocation_request(recipient: Address, amount: i128) -> AllocationRequest {
    AllocationRequest { recipient, amount }
}

// Initialization Tests

#[test]
fn test_initialize_contract() {
    let (_env, admin, _token, _token_client, client) = setup_test_env();

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_total_batches(), 0);
    assert_eq!(client.get_total_allocations_processed(), 0);
    assert_eq!(client.get_total_allocated_volume(), 0);
}

#[test]
#[should_panic(expected = "Contract already initialized")]
fn test_cannot_initialize_twice() {
    let (env, _admin, _token, _token_client, client) = setup_test_env();

    let new_admin = Address::generate(&env);
    client.initialize(&new_admin);
}

// Batch Allocation Tests

#[test]
fn test_allocate_shared_budget_single_recipient() {
    let (env, admin, token, _token_client, client) = setup_test_env();

    let recipient = Address::generate(&env);
    let amount: i128 = 10_000_000; // 1 XLM

    let mut allocations: Vec<AllocationRequest> = Vec::new(&env);
    allocations.push_back(create_allocation_request(recipient.clone(), amount));

    let result = client.allocate_shared_budget_batch(&admin, &token, &allocations);

    assert_eq!(result.total_requests, 1);
    assert_eq!(result.successful, 1);
    assert_eq!(result.failed, 0);
    assert_eq!(result.total_allocated, amount);
    assert_eq!(result.results.len(), 1);
}

#[test]
fn test_allocate_shared_budget_multiple_recipients() {
    let (env, admin, token, _token_client, client) = setup_test_env();

    let recipient1 = Address::generate(&env);
    let recipient2 = Address::generate(&env);
    let recipient3 = Address::generate(&env);

    let amount1: i128 = 10_000_000;
    let amount2: i128 = 20_000_000;
    let amount3: i128 = 30_000_000;

    let mut allocations: Vec<AllocationRequest> = Vec::new(&env);
    allocations.push_back(create_allocation_request(recipient1.clone(), amount1));
    allocations.push_back(create_allocation_request(recipient2.clone(), amount2));
    allocations.push_back(create_allocation_request(recipient3.clone(), amount3));

    let result = client.allocate_shared_budget_batch(&admin, &token, &allocations);

    assert_eq!(result.total_requests, 3);
    assert_eq!(result.successful, 3);
    assert_eq!(result.failed, 0);
    assert_eq!(result.total_allocated, amount1 + amount2 + amount3);
}

#[test]
fn test_allocate_with_invalid_amounts_partial_failures() {
    let (env, admin, token, _token_client, client) = setup_test_env();

    let recipient1 = Address::generate(&env);
    let recipient2 = Address::generate(&env);

    let mut allocations: Vec<AllocationRequest> = Vec::new(&env);
    allocations.push_back(create_allocation_request(recipient1.clone(), -100)); // Invalid
    allocations.push_back(create_allocation_request(recipient2.clone(), 10_000_000)); // Valid

    let result = client.allocate_shared_budget_batch(&admin, &token, &allocations);

    assert_eq!(result.total_requests, 2);
    assert_eq!(result.successful, 1);
    assert_eq!(result.failed, 1);
    assert_eq!(result.total_allocated, 10_000_000);
}

#[test]
fn test_allocate_with_insufficient_shared_budget_partial_failures() {
    let (env, admin, token, _token_client, client) = setup_test_env();

    let recipient1 = Address::generate(&env);
    let recipient2 = Address::generate(&env);

    let amount1: i128 = 10_000_000;
    let amount2: i128 = 1_000_000_000_001; // More than available

    let mut allocations: Vec<AllocationRequest> = Vec::new(&env);
    allocations.push_back(create_allocation_request(recipient1.clone(), amount1));
    allocations.push_back(create_allocation_request(recipient2.clone(), amount2));

    let result = client.allocate_shared_budget_batch(&admin, &token, &allocations);

    assert_eq!(result.total_requests, 2);
    assert_eq!(result.successful, 1);
    assert_eq!(result.failed, 1);
    assert_eq!(result.total_allocated, amount1);
}

#[test]
fn test_allocation_events_emitted() {
    let (env, admin, token, _token_client, client) = setup_test_env();

    let recipient1 = Address::generate(&env);
    let recipient2 = Address::generate(&env);

    let mut allocations: Vec<AllocationRequest> = Vec::new(&env);
    allocations.push_back(create_allocation_request(recipient1.clone(), 10_000_000));
    allocations.push_back(create_allocation_request(recipient2.clone(), -100)); // Invalid

    client.allocate_shared_budget_batch(&admin, &token, &allocations);

    let events = env.events().all();
    // Should have: batch_started, allocation_success (1), allocation_failure (1), batch_completed
    assert!(events.len() >= 4);
}

#[test]
fn test_allocation_stats_accumulate() {
    let (env, admin, token, _token_client, client) = setup_test_env();

    let recipient1 = Address::generate(&env);
    let recipient2 = Address::generate(&env);

    let mut batch1: Vec<AllocationRequest> = Vec::new(&env);
    batch1.push_back(create_allocation_request(recipient1.clone(), 10_000_000));

    let mut batch2: Vec<AllocationRequest> = Vec::new(&env);
    batch2.push_back(create_allocation_request(recipient2.clone(), 20_000_000));

    assert_eq!(client.get_total_batches(), 0);
    assert_eq!(client.get_total_allocations_processed(), 0);
    assert_eq!(client.get_total_allocated_volume(), 0);

    client.allocate_shared_budget_batch(&admin, &token, &batch1);
    assert_eq!(client.get_total_batches(), 1);
    assert_eq!(client.get_total_allocations_processed(), 1);
    assert_eq!(client.get_total_allocated_volume(), 10_000_000);

    client.allocate_shared_budget_batch(&admin, &token, &batch2);
    assert_eq!(client.get_total_batches(), 2);
    assert_eq!(client.get_total_allocations_processed(), 2);
    assert_eq!(client.get_total_allocated_volume(), 30_000_000);
}

// Admin and Error Tests

#[test]
#[should_panic]
fn test_allocate_empty_batch_rejected() {
    let (env, admin, token, _token_client, client) = setup_test_env();

    let allocations: Vec<AllocationRequest> = Vec::new(&env);
    client.allocate_shared_budget_batch(&admin, &token, &allocations);
}

#[test]
#[should_panic]
fn test_allocate_unauthorized_caller() {
    let (env, _admin, token, _token_client, client) = setup_test_env();

    let unauthorized = Address::generate(&env);
    let recipient = Address::generate(&env);

    let mut allocations: Vec<AllocationRequest> = Vec::new(&env);
    allocations.push_back(create_allocation_request(recipient, 10_000_000));

    client.allocate_shared_budget_batch(&unauthorized, &token, &allocations);
}

#[test]
fn test_set_admin() {
    let (env, admin, _token, _token_client, client) = setup_test_env();

    let new_admin = Address::generate(&env);
    client.set_admin(&admin, &new_admin);

    assert_eq!(client.get_admin(), new_admin);
}

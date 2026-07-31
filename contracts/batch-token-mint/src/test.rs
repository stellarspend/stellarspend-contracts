//! Comprehensive unit and integration tests for batch token minting.

#![cfg(test)]

use crate::{BatchTokenMintContract, BatchTokenMintContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

use crate::types::{ErrorCode, MintResult, TokenMintRequest};

/// Helper function to create a test environment with initialized contract.
fn setup_test_contract() -> (Env, Address, BatchTokenMintContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(BatchTokenMintContract, ());
    let client = BatchTokenMintContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    (env, admin, client)
}

/// Helper function to create a valid mint request.
fn create_valid_request(env: &Env, amount: i128) -> TokenMintRequest {
    TokenMintRequest {
        recipient: Address::generate(env),
        amount,
    }
}

#[test]
fn test_initialize() {
    let (_, admin, client) = setup_test_contract();

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_last_batch_id(), 0);
    assert_eq!(client.get_total_minted(), 0);
    assert_eq!(client.get_total_batches_processed(), 0);
}

#[test]
#[should_panic(expected = "Contract already initialized")]
fn test_initialize_twice_fails() {
    let (env, _admin, client) = setup_test_contract();
    let new_admin = Address::generate(&env);
    client.initialize(&new_admin);
}

#[test]
fn test_batch_mint_single_recipient() {
    let (env, admin, client) = setup_test_contract();
    let token = Address::generate(&env);

    let mut requests: Vec<TokenMintRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, 100_000_000));

    let result = client.batch_mint_tokens(&admin, &token, &requests);

    assert_eq!(result.successful, 1);
    assert_eq!(result.failed, 0);
    assert_eq!(result.total_requests, 1);
    assert_eq!(result.metrics.total_amount_minted, 100_000_000);
    assert_eq!(result.metrics.avg_mint_amount, 100_000_000);
}

#[test]
fn test_batch_mint_multiple_recipients() {
    let (env, admin, client) = setup_test_contract();
    let token = Address::generate(&env);

    let mut requests: Vec<TokenMintRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, 100_000_000));
    requests.push_back(create_valid_request(&env, 200_000_000));
    requests.push_back(create_valid_request(&env, 150_000_000));

    let result = client.batch_mint_tokens(&admin, &token, &requests);

    assert_eq!(result.successful, 3);
    assert_eq!(result.failed, 0);
    assert_eq!(result.total_requests, 3);
    assert_eq!(result.metrics.total_amount_minted, 450_000_000);
    assert_eq!(result.metrics.avg_mint_amount, 150_000_000);
}

#[test]
fn test_batch_mint_metrics() {
    let (env, admin, client) = setup_test_contract();
    let token = Address::generate(&env);

    let mut requests: Vec<TokenMintRequest> = Vec::new(&env);
    for _ in 0..5 {
        requests.push_back(create_valid_request(&env, 50_000_000));
    }

    let result = client.batch_mint_tokens(&admin, &token, &requests);

    assert_eq!(result.metrics.total_requests, 5);
    assert_eq!(result.metrics.successful_mints, 5);
    assert_eq!(result.metrics.failed_mints, 0);
    assert_eq!(result.metrics.total_amount_minted, 250_000_000);
    assert_eq!(result.metrics.avg_mint_amount, 50_000_000);
}

#[test]
fn test_batch_mint_invalid_amount_zero() {
    let (env, admin, client) = setup_test_contract();
    let token = Address::generate(&env);

    let mut requests: Vec<TokenMintRequest> = Vec::new(&env);
    let mut invalid_req = create_valid_request(&env, 100_000_000);
    invalid_req.amount = 0;
    requests.push_back(invalid_req);

    let result = client.batch_mint_tokens(&admin, &token, &requests);

    assert_eq!(result.successful, 0);
    assert_eq!(result.failed, 1);

    match &result.results.get(0).unwrap() {
        MintResult::Failure(_, code) => {
            assert_eq!(*code, ErrorCode::INVALID_AMOUNT);
        }
        _ => panic!("Expected failure"),
    }
}

#[test]
fn test_batch_mint_invalid_amount_negative() {
    let (env, admin, client) = setup_test_contract();
    let token = Address::generate(&env);

    let mut requests: Vec<TokenMintRequest> = Vec::new(&env);
    let mut invalid_req = create_valid_request(&env, 100_000_000);
    invalid_req.amount = -1000;
    requests.push_back(invalid_req);

    let result = client.batch_mint_tokens(&admin, &token, &requests);

    assert_eq!(result.successful, 0);
    assert_eq!(result.failed, 1);

    match &result.results.get(0).unwrap() {
        MintResult::Failure(_, code) => {
            assert_eq!(*code, ErrorCode::INVALID_AMOUNT);
        }
        _ => panic!("Expected failure"),
    }
}

#[test]
fn test_batch_mint_partial_failures() {
    let (env, admin, client) = setup_test_contract();
    let token = Address::generate(&env);

    let mut requests: Vec<TokenMintRequest> = Vec::new(&env);

    // Valid
    requests.push_back(create_valid_request(&env, 100_000_000));

    // Invalid - zero amount
    let mut invalid1 = create_valid_request(&env, 50_000_000);
    invalid1.amount = 0;
    requests.push_back(invalid1);

    // Valid
    requests.push_back(create_valid_request(&env, 200_000_000));

    // Invalid - negative amount
    let mut invalid2 = create_valid_request(&env, 75_000_000);
    invalid2.amount = -100;
    requests.push_back(invalid2);

    let result = client.batch_mint_tokens(&admin, &token, &requests);

    assert_eq!(result.total_requests, 4);
    assert_eq!(result.successful, 2);
    assert_eq!(result.failed, 2);
    assert_eq!(result.metrics.total_amount_minted, 300_000_000);
    assert_eq!(result.metrics.avg_mint_amount, 150_000_000);
}

#[test]
fn test_batch_mint_reverts_when_any_request_is_invalid() {
    let (env, admin, client) = setup_test_contract();
    let token = Address::generate(&env);

    let mut requests: Vec<TokenMintRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, 100_000_000));

    let mut invalid = create_valid_request(&env, 50_000_000);
    invalid.amount = 0;
    requests.push_back(invalid);

    let result = client.try_batch_mint_tokens(&admin, &token, &requests);

    assert!(result.is_err(), "invalid requests should revert the whole batch");
    assert_eq!(client.get_total_minted(), 0);
    assert_eq!(client.get_total_batches_processed(), 0);
    assert_eq!(client.get_last_batch_id(), 0);
}

#[test]
fn test_batch_mint_storage_updates() {
    let (env, admin, client) = setup_test_contract();
    let token = Address::generate(&env);

    let mut requests: Vec<TokenMintRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, 100_000_000));

    let result = client.batch_mint_tokens(&admin, &token, &requests);

    assert_eq!(client.get_total_minted(), 100_000_000);
    assert_eq!(client.get_total_batches_processed(), 1);
    assert_eq!(client.get_last_batch_id(), result.batch_id);
}

#[test]
fn test_batch_mint_multiple_batches() {
    let (env, admin, client) = setup_test_contract();
    let token = Address::generate(&env);

    // First batch
    let mut requests1: Vec<TokenMintRequest> = Vec::new(&env);
    requests1.push_back(create_valid_request(&env, 100_000_000));
    let result1 = client.batch_mint_tokens(&admin, &token, &requests1);

    // Second batch
    let mut requests2: Vec<TokenMintRequest> = Vec::new(&env);
    requests2.push_back(create_valid_request(&env, 200_000_000));
    let result2 = client.batch_mint_tokens(&admin, &token, &requests2);

    assert_eq!(client.get_total_minted(), 300_000_000);
    assert_eq!(client.get_total_batches_processed(), 2);
    assert_ne!(result1.batch_id, result2.batch_id);
}

#[test]
fn test_batch_mint_large_amount_event() {
    let (env, admin, client) = setup_test_contract();
    let token = Address::generate(&env);

    let mut requests: Vec<TokenMintRequest> = Vec::new(&env);
    // This should trigger the large_mint event (>= 1 billion stroops)
    requests.push_back(create_valid_request(&env, 1_000_000_000));

    let result = client.batch_mint_tokens(&admin, &token, &requests);

    assert_eq!(result.successful, 1);
    assert_eq!(result.metrics.total_amount_minted, 1_000_000_000);
}

#[test]
fn test_batch_mint_admin_update() {
    let (env, admin, client) = setup_test_contract();
    let new_admin = Address::generate(&env);

    // Update admin
    client.set_admin(&admin, &new_admin);
    assert_eq!(client.get_admin(), new_admin);
}

#[test]
fn test_batch_mint_set_admin_unauthorized() {
    let (env, admin, client) = setup_test_contract();
    let unauthorized = Address::generate(&env);
    let new_admin = Address::generate(&env);

    // Try to update admin as unauthorized user - should panic
    let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.set_admin(&unauthorized, &new_admin);
    }));

    assert!(panic_result.is_err());
}

#[test]
fn test_batch_mint_empty_batch() {
    let (env, admin, client) = setup_test_contract();
    let token = Address::generate(&env);

    let requests: Vec<TokenMintRequest> = Vec::new(&env);

    let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.batch_mint_tokens(&admin, &token, &requests);
    }));

    assert!(panic_result.is_err());
}

#[test]
fn test_batch_mint_too_large() {
    let (env, admin, client) = setup_test_contract();
    let token = Address::generate(&env);

    // Create batch exceeding MAX_BATCH_SIZE (100)
    let mut requests: Vec<TokenMintRequest> = Vec::new(&env);
    for _ in 0..=100 {
        requests.push_back(create_valid_request(&env, 100_000_000));
    }

    let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.batch_mint_tokens(&admin, &token, &requests);
    }));

    assert!(panic_result.is_err());
}

#[test]
fn test_batch_mint_batch_id_increment() {
    let (env, admin, client) = setup_test_contract();
    let token = Address::generate(&env);

    let mut requests: Vec<TokenMintRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, 100_000_000));

    let result1 = client.batch_mint_tokens(&admin, &token, &requests);
    assert_eq!(result1.batch_id, 1);

    let result2 = client.batch_mint_tokens(&admin, &token, &requests);
    assert_eq!(result2.batch_id, 2);

    let result3 = client.batch_mint_tokens(&admin, &token, &requests);
    assert_eq!(result3.batch_id, 3);
}

#[test]
fn test_batch_mint_all_valid_requests() {
    let (env, admin, client) = setup_test_contract();
    let token = Address::generate(&env);

    let mut requests: Vec<TokenMintRequest> = Vec::new(&env);
    for i in 1..=10 {
        requests.push_back(create_valid_request(&env, i as i128 * 10_000_000));
    }

    let result = client.batch_mint_tokens(&admin, &token, &requests);

    assert_eq!(result.successful, 10);
    assert_eq!(result.failed, 0);
    // Sum: 10+20+30+...+100 = 10*(11)*50 = 550 million
    assert_eq!(result.metrics.total_amount_minted, 550_000_000);
    assert_eq!(result.metrics.avg_mint_amount, 55_000_000);
}

#[test]
fn test_batch_mint_max_amount() {
    let (env, admin, client) = setup_test_contract();
    let token = Address::generate(&env);

    let mut requests: Vec<TokenMintRequest> = Vec::new(&env);
    // Use a valid large amount (not the absolute max to avoid overflow)
    requests.push_back(create_valid_request(&env, 100_000_000_000_000_000));

    let result = client.batch_mint_tokens(&admin, &token, &requests);

    assert_eq!(result.successful, 1);
    assert_eq!(result.failed, 0);
}

#[test]
fn test_batch_mint_unauthorized_caller() {
    let (env, _admin, client) = setup_test_contract();
    let token = Address::generate(&env);
    let unauthorized = Address::generate(&env);

    let mut requests: Vec<TokenMintRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, 100_000_000));

    let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.batch_mint_tokens(&unauthorized, &token, &requests);
    }));

    assert!(panic_result.is_err());
}

#[test]
fn test_batch_mint_result_structure() {
    let (env, admin, client) = setup_test_contract();
    let token = Address::generate(&env);

    let mut requests: Vec<TokenMintRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, 100_000_000));

    let result = client.batch_mint_tokens(&admin, &token, &requests);

    // Verify result structure
    assert_eq!(result.batch_id, 1);
    assert_eq!(result.token_address, token);
    assert_eq!(result.total_requests, 1);
    assert_eq!(result.successful, 1);
    assert_eq!(result.failed, 0);
    assert_eq!(result.results.len(), 1);
    assert_eq!(result.metrics.total_requests, 1);
    assert_eq!(result.metrics.successful_mints, 1);
    assert_eq!(result.metrics.failed_mints, 0);
}

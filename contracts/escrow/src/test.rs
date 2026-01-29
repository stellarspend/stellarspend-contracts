//! Integration tests for the Escrow Contract with Batch Reversal.

#![cfg(test)]

use crate::{
    EscrowContract, EscrowContractClient, EscrowStatus, ReversalRequest, ReversalResult,
};
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger},
    token, Address, Env, Vec,
};

/// Creates a test environment with the contract deployed and initialized.
fn setup_test_env() -> (
    Env,
    Address,
    Address,
    token::Client<'static>,
    token::StellarAssetClient<'static>,
    EscrowContractClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| {
        li.sequence_number = 12345;
    });

    // Deploy token contract
    let issuer = Address::generate(&env);
    let stellar_asset = env.register_stellar_asset_contract_v2(issuer.clone());
    let token_id: Address = stellar_asset.address();
    let token_client = token::Client::new(&env, &token_id);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_id);

    // Deploy escrow contract
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin, &token_id);

    (env, admin, token_id, token_client, token_admin_client, client)
}

/// Helper to create a reversal request.
fn create_reversal_request(escrow_id: u64) -> ReversalRequest {
    ReversalRequest { escrow_id }
}

/// Helper to create an escrow and return its ID.
fn create_test_escrow(
    env: &Env,
    client: &EscrowContractClient,
    token_admin: &token::StellarAssetClient,
    depositor: &Address,
    recipient: &Address,
    amount: i128,
    deadline: u64,
) -> u64 {
    // Mint tokens to depositor
    token_admin.mint(depositor, &amount);

    // Create escrow
    client.create_escrow(depositor, recipient, &amount, &deadline)
}

// ============================================
// Initialization Tests
// ============================================

#[test]
fn test_initialize_contract() {
    let (_env, admin, token, _token_client, _token_admin, client) = setup_test_env();

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_total_reversal_batches(), 0);
    assert_eq!(client.get_total_escrows_reversed(), 0);
    assert_eq!(client.get_total_amount_reversed(), 0);
    assert_eq!(client.get_escrow_counter(), 0);
}

#[test]
#[should_panic]
fn test_cannot_initialize_twice() {
    let (env, _admin, token, _token_client, _token_admin, client) = setup_test_env();

    let new_admin = Address::generate(&env);
    client.initialize(&new_admin, &token);
}

// ============================================
// Escrow Creation Tests
// ============================================

#[test]
fn test_create_escrow() {
    let (env, _admin, _token, token_client, token_admin, client) = setup_test_env();

    let depositor = Address::generate(&env);
    let recipient = Address::generate(&env);
    let amount: i128 = 10_000_000;
    let deadline: u64 = 20000;

    let escrow_id =
        create_test_escrow(&env, &client, &token_admin, &depositor, &recipient, amount, deadline);

    assert_eq!(escrow_id, 1);
    assert_eq!(client.get_escrow_counter(), 1);

    let escrow = client.get_escrow(&escrow_id).unwrap();
    assert_eq!(escrow.depositor, depositor);
    assert_eq!(escrow.recipient, recipient);
    assert_eq!(escrow.amount, amount);
    assert_eq!(escrow.status, EscrowStatus::Active);
    assert_eq!(escrow.deadline, deadline);

    // Check user escrows
    let user_escrows = client.get_user_escrows(&depositor);
    assert_eq!(user_escrows.len(), 1);
    assert_eq!(user_escrows.get(0).unwrap(), 1);
}

#[test]
fn test_create_multiple_escrows() {
    let (env, _admin, _token, _token_client, token_admin, client) = setup_test_env();

    let depositor = Address::generate(&env);
    let recipient1 = Address::generate(&env);
    let recipient2 = Address::generate(&env);

    let escrow_id1 = create_test_escrow(
        &env,
        &client,
        &token_admin,
        &depositor,
        &recipient1,
        10_000_000,
        20000,
    );
    let escrow_id2 = create_test_escrow(
        &env,
        &client,
        &token_admin,
        &depositor,
        &recipient2,
        20_000_000,
        25000,
    );

    assert_eq!(escrow_id1, 1);
    assert_eq!(escrow_id2, 2);
    assert_eq!(client.get_escrow_counter(), 2);

    let user_escrows = client.get_user_escrows(&depositor);
    assert_eq!(user_escrows.len(), 2);
}

#[test]
#[should_panic]
fn test_create_escrow_invalid_amount() {
    let (env, _admin, _token, _token_client, _token_admin, client) = setup_test_env();

    let depositor = Address::generate(&env);
    let recipient = Address::generate(&env);

    // Should panic due to invalid amount
    client.create_escrow(&depositor, &recipient, &0, &20000);
}

// ============================================
// Single Escrow Reversal Tests
// ============================================

#[test]
fn test_batch_reverse_single_escrow() {
    let (env, admin, _token, token_client, token_admin, client) = setup_test_env();

    let depositor = Address::generate(&env);
    let recipient = Address::generate(&env);
    let amount: i128 = 10_000_000;

    let escrow_id =
        create_test_escrow(&env, &client, &token_admin, &depositor, &recipient, amount, 20000);

    // Create reversal request
    let mut requests: Vec<ReversalRequest> = Vec::new(&env);
    requests.push_back(create_reversal_request(escrow_id));

    // Execute batch reversal
    let result = client.batch_reverse_escrows(&admin, &requests);

    assert_eq!(result.total_requests, 1);
    assert_eq!(result.successful, 1);
    assert_eq!(result.failed, 0);
    assert_eq!(result.total_reversed, amount);

    // Check escrow status is now Reversed
    let escrow = client.get_escrow(&escrow_id).unwrap();
    assert_eq!(escrow.status, EscrowStatus::Reversed);

    // Check result details
    match result.results.get(0).unwrap() {
        ReversalResult::Success(id, dep, amt) => {
            assert_eq!(id, escrow_id);
            assert_eq!(dep, depositor);
            assert_eq!(amt, amount);
        }
        _ => panic!("Expected success"),
    }
}

// ============================================
// Batch Reversal Tests
// ============================================

#[test]
fn test_batch_reverse_multiple_escrows() {
    let (env, admin, _token, _token_client, token_admin, client) = setup_test_env();

    let depositor1 = Address::generate(&env);
    let depositor2 = Address::generate(&env);
    let depositor3 = Address::generate(&env);
    let recipient = Address::generate(&env);

    let escrow_id1 = create_test_escrow(
        &env,
        &client,
        &token_admin,
        &depositor1,
        &recipient,
        10_000_000,
        20000,
    );
    let escrow_id2 = create_test_escrow(
        &env,
        &client,
        &token_admin,
        &depositor2,
        &recipient,
        20_000_000,
        20000,
    );
    let escrow_id3 = create_test_escrow(
        &env,
        &client,
        &token_admin,
        &depositor3,
        &recipient,
        30_000_000,
        20000,
    );

    // Create reversal requests
    let mut requests: Vec<ReversalRequest> = Vec::new(&env);
    requests.push_back(create_reversal_request(escrow_id1));
    requests.push_back(create_reversal_request(escrow_id2));
    requests.push_back(create_reversal_request(escrow_id3));

    // Execute batch reversal
    let result = client.batch_reverse_escrows(&admin, &requests);

    assert_eq!(result.total_requests, 3);
    assert_eq!(result.successful, 3);
    assert_eq!(result.failed, 0);
    assert_eq!(result.total_reversed, 60_000_000);

    // Verify all escrows are reversed
    assert_eq!(client.get_escrow(&escrow_id1).unwrap().status, EscrowStatus::Reversed);
    assert_eq!(client.get_escrow(&escrow_id2).unwrap().status, EscrowStatus::Reversed);
    assert_eq!(client.get_escrow(&escrow_id3).unwrap().status, EscrowStatus::Reversed);
}

#[test]
fn test_batch_reverse_large_batch() {
    let (env, admin, _token, _token_client, token_admin, client) = setup_test_env();

    let recipient = Address::generate(&env);
    let mut escrow_ids: Vec<u64> = Vec::new(&env);

    // Create 50 escrows
    for _i in 0..50 {
        let depositor = Address::generate(&env);
        let escrow_id = create_test_escrow(
            &env,
            &client,
            &token_admin,
            &depositor,
            &recipient,
            1_000_000,
            20000,
        );
        escrow_ids.push_back(escrow_id);
    }

    // Create reversal requests for all
    let mut requests: Vec<ReversalRequest> = Vec::new(&env);
    for id in escrow_ids.iter() {
        requests.push_back(create_reversal_request(id));
    }

    // Execute batch reversal
    let result = client.batch_reverse_escrows(&admin, &requests);

    assert_eq!(result.total_requests, 50);
    assert_eq!(result.successful, 50);
    assert_eq!(result.failed, 0);
    assert_eq!(result.total_reversed, 50_000_000);
}

// ============================================
// Failure Cases Tests
// ============================================

#[test]
fn test_batch_reverse_nonexistent_escrow() {
    let (env, admin, _token, _token_client, _token_admin, client) = setup_test_env();

    // Try to reverse non-existent escrow
    let mut requests: Vec<ReversalRequest> = Vec::new(&env);
    requests.push_back(create_reversal_request(999)); // Does not exist

    let result = client.batch_reverse_escrows(&admin, &requests);

    assert_eq!(result.total_requests, 1);
    assert_eq!(result.successful, 0);
    assert_eq!(result.failed, 1);
    assert_eq!(result.total_reversed, 0);

    // Check failure details
    match result.results.get(0).unwrap() {
        ReversalResult::Failure(id, error_code) => {
            assert_eq!(id, 999);
            assert_eq!(error_code, 0); // ESCROW_NOT_FOUND
        }
        _ => panic!("Expected failure"),
    }
}

#[test]
fn test_batch_reverse_already_released_escrow() {
    let (env, admin, _token, _token_client, token_admin, client) = setup_test_env();

    let depositor = Address::generate(&env);
    let recipient = Address::generate(&env);

    let escrow_id =
        create_test_escrow(&env, &client, &token_admin, &depositor, &recipient, 10_000_000, 20000);

    // Release the escrow first
    client.release_escrow(&admin, &escrow_id);

    // Try to reverse already released escrow
    let mut requests: Vec<ReversalRequest> = Vec::new(&env);
    requests.push_back(create_reversal_request(escrow_id));

    let result = client.batch_reverse_escrows(&admin, &requests);

    assert_eq!(result.total_requests, 1);
    assert_eq!(result.successful, 0);
    assert_eq!(result.failed, 1);

    // Check failure details
    match result.results.get(0).unwrap() {
        ReversalResult::Failure(id, error_code) => {
            assert_eq!(id, escrow_id);
            assert_eq!(error_code, 1); // ALREADY_RELEASED
        }
        _ => panic!("Expected failure"),
    }
}

#[test]
fn test_batch_reverse_already_reversed_escrow() {
    let (env, admin, _token, _token_client, token_admin, client) = setup_test_env();

    let depositor = Address::generate(&env);
    let recipient = Address::generate(&env);

    let escrow_id =
        create_test_escrow(&env, &client, &token_admin, &depositor, &recipient, 10_000_000, 20000);

    // Reverse the escrow first
    let mut requests: Vec<ReversalRequest> = Vec::new(&env);
    requests.push_back(create_reversal_request(escrow_id));
    client.batch_reverse_escrows(&admin, &requests);

    // Try to reverse again
    let result = client.batch_reverse_escrows(&admin, &requests);

    assert_eq!(result.total_requests, 1);
    assert_eq!(result.successful, 0);
    assert_eq!(result.failed, 1);

    // Check failure details
    match result.results.get(0).unwrap() {
        ReversalResult::Failure(id, error_code) => {
            assert_eq!(id, escrow_id);
            assert_eq!(error_code, 2); // ALREADY_REVERSED
        }
        _ => panic!("Expected failure"),
    }
}

// ============================================
// Partial Failure Tests
// ============================================

#[test]
fn test_batch_reverse_partial_failures_mixed() {
    let (env, admin, _token, _token_client, token_admin, client) = setup_test_env();

    let depositor1 = Address::generate(&env);
    let depositor2 = Address::generate(&env);
    let recipient = Address::generate(&env);

    // Create two valid escrows
    let escrow_id1 = create_test_escrow(
        &env,
        &client,
        &token_admin,
        &depositor1,
        &recipient,
        10_000_000,
        20000,
    );
    let escrow_id2 = create_test_escrow(
        &env,
        &client,
        &token_admin,
        &depositor2,
        &recipient,
        20_000_000,
        20000,
    );

    // Release one of them
    client.release_escrow(&admin, &escrow_id2);

    // Try to reverse: valid active, invalid released, non-existent
    let mut requests: Vec<ReversalRequest> = Vec::new(&env);
    requests.push_back(create_reversal_request(escrow_id1)); // Active - should succeed
    requests.push_back(create_reversal_request(escrow_id2)); // Released - should fail
    requests.push_back(create_reversal_request(999)); // Non-existent - should fail

    let result = client.batch_reverse_escrows(&admin, &requests);

    assert_eq!(result.total_requests, 3);
    assert_eq!(result.successful, 1);
    assert_eq!(result.failed, 2);
    assert_eq!(result.total_reversed, 10_000_000);

    // Verify results
    match result.results.get(0).unwrap() {
        ReversalResult::Success(id, _, amt) => {
            assert_eq!(id, escrow_id1);
            assert_eq!(amt, 10_000_000);
        }
        _ => panic!("Expected success for first escrow"),
    }

    match result.results.get(1).unwrap() {
        ReversalResult::Failure(id, error_code) => {
            assert_eq!(id, escrow_id2);
            assert_eq!(error_code, 1); // ALREADY_RELEASED
        }
        _ => panic!("Expected failure for released escrow"),
    }

    match result.results.get(2).unwrap() {
        ReversalResult::Failure(id, error_code) => {
            assert_eq!(id, 999);
            assert_eq!(error_code, 0); // ESCROW_NOT_FOUND
        }
        _ => panic!("Expected failure for non-existent escrow"),
    }
}

#[test]
fn test_batch_reverse_some_active_some_reversed() {
    let (env, admin, _token, _token_client, token_admin, client) = setup_test_env();

    let depositor1 = Address::generate(&env);
    let depositor2 = Address::generate(&env);
    let depositor3 = Address::generate(&env);
    let depositor4 = Address::generate(&env);
    let recipient = Address::generate(&env);

    let escrow_id1 = create_test_escrow(
        &env,
        &client,
        &token_admin,
        &depositor1,
        &recipient,
        10_000_000,
        20000,
    );
    let escrow_id2 = create_test_escrow(
        &env,
        &client,
        &token_admin,
        &depositor2,
        &recipient,
        20_000_000,
        20000,
    );
    let escrow_id3 = create_test_escrow(
        &env,
        &client,
        &token_admin,
        &depositor3,
        &recipient,
        30_000_000,
        20000,
    );
    let escrow_id4 = create_test_escrow(
        &env,
        &client,
        &token_admin,
        &depositor4,
        &recipient,
        40_000_000,
        20000,
    );

    // Reverse escrow 2 and 4 first
    let mut first_batch: Vec<ReversalRequest> = Vec::new(&env);
    first_batch.push_back(create_reversal_request(escrow_id2));
    first_batch.push_back(create_reversal_request(escrow_id4));
    client.batch_reverse_escrows(&admin, &first_batch);

    // Now try to reverse all four
    let mut requests: Vec<ReversalRequest> = Vec::new(&env);
    requests.push_back(create_reversal_request(escrow_id1)); // Active
    requests.push_back(create_reversal_request(escrow_id2)); // Already reversed
    requests.push_back(create_reversal_request(escrow_id3)); // Active
    requests.push_back(create_reversal_request(escrow_id4)); // Already reversed

    let result = client.batch_reverse_escrows(&admin, &requests);

    assert_eq!(result.total_requests, 4);
    assert_eq!(result.successful, 2);
    assert_eq!(result.failed, 2);
    assert_eq!(result.total_reversed, 40_000_000); // 10 + 30 million
}

// ============================================
// Validation Tests
// ============================================

#[test]
#[should_panic]
fn test_batch_reverse_empty_batch() {
    let (env, admin, _token, _token_client, _token_admin, client) = setup_test_env();

    let requests: Vec<ReversalRequest> = Vec::new(&env);
    client.batch_reverse_escrows(&admin, &requests);
}

#[test]
#[should_panic]
fn test_batch_reverse_unauthorized() {
    let (env, _admin, _token, _token_client, token_admin, client) = setup_test_env();

    let depositor = Address::generate(&env);
    let recipient = Address::generate(&env);
    let unauthorized = Address::generate(&env);

    let escrow_id =
        create_test_escrow(&env, &client, &token_admin, &depositor, &recipient, 10_000_000, 20000);

    let mut requests: Vec<ReversalRequest> = Vec::new(&env);
    requests.push_back(create_reversal_request(escrow_id));

    // Should panic due to unauthorized caller
    client.batch_reverse_escrows(&unauthorized, &requests);
}

// ============================================
// Event Emission Tests
// ============================================

#[test]
fn test_batch_reverse_events_emitted() {
    let (env, admin, _token, _token_client, token_admin, client) = setup_test_env();

    let depositor = Address::generate(&env);
    let recipient = Address::generate(&env);

    let escrow_id1 =
        create_test_escrow(&env, &client, &token_admin, &depositor, &recipient, 10_000_000, 20000);

    // Create another depositor for second escrow
    let depositor2 = Address::generate(&env);
    let escrow_id2 = create_test_escrow(
        &env,
        &client,
        &token_admin,
        &depositor2,
        &recipient,
        20_000_000,
        20000,
    );

    let mut requests: Vec<ReversalRequest> = Vec::new(&env);
    requests.push_back(create_reversal_request(escrow_id1));
    requests.push_back(create_reversal_request(999)); // Non-existent - will fail
    requests.push_back(create_reversal_request(escrow_id2));

    client.batch_reverse_escrows(&admin, &requests);

    let events = env.events().all();
    // Should have: escrow_created (2), batch_reversal_started, reversal_success (2),
    // reversal_failure (1), batch_reversal_completed
    // Plus token transfer events
    assert!(events.len() >= 6);
}

// ============================================
// State Tracking Tests
// ============================================

#[test]
fn test_batch_reverse_accumulates_stats() {
    let (env, admin, _token, _token_client, token_admin, client) = setup_test_env();

    let recipient = Address::generate(&env);

    assert_eq!(client.get_total_reversal_batches(), 0);
    assert_eq!(client.get_total_escrows_reversed(), 0);
    assert_eq!(client.get_total_amount_reversed(), 0);

    // First batch: create and reverse 2 escrows
    let depositor1 = Address::generate(&env);
    let depositor2 = Address::generate(&env);
    let escrow_id1 = create_test_escrow(
        &env,
        &client,
        &token_admin,
        &depositor1,
        &recipient,
        10_000_000,
        20000,
    );
    let escrow_id2 = create_test_escrow(
        &env,
        &client,
        &token_admin,
        &depositor2,
        &recipient,
        20_000_000,
        20000,
    );

    let mut batch1: Vec<ReversalRequest> = Vec::new(&env);
    batch1.push_back(create_reversal_request(escrow_id1));
    batch1.push_back(create_reversal_request(escrow_id2));
    client.batch_reverse_escrows(&admin, &batch1);

    assert_eq!(client.get_total_reversal_batches(), 1);
    assert_eq!(client.get_total_escrows_reversed(), 2);
    assert_eq!(client.get_total_amount_reversed(), 30_000_000);

    // Second batch: create and reverse 1 escrow
    let depositor3 = Address::generate(&env);
    let escrow_id3 = create_test_escrow(
        &env,
        &client,
        &token_admin,
        &depositor3,
        &recipient,
        15_000_000,
        20000,
    );

    let mut batch2: Vec<ReversalRequest> = Vec::new(&env);
    batch2.push_back(create_reversal_request(escrow_id3));
    client.batch_reverse_escrows(&admin, &batch2);

    assert_eq!(client.get_total_reversal_batches(), 2);
    assert_eq!(client.get_total_escrows_reversed(), 3);
    assert_eq!(client.get_total_amount_reversed(), 45_000_000);
}

#[test]
fn test_batch_id_increments() {
    let (env, admin, _token, _token_client, token_admin, client) = setup_test_env();

    let recipient = Address::generate(&env);

    // First batch
    let depositor1 = Address::generate(&env);
    let escrow_id1 = create_test_escrow(
        &env,
        &client,
        &token_admin,
        &depositor1,
        &recipient,
        10_000_000,
        20000,
    );
    let mut batch1: Vec<ReversalRequest> = Vec::new(&env);
    batch1.push_back(create_reversal_request(escrow_id1));
    let result1 = client.batch_reverse_escrows(&admin, &batch1);
    assert_eq!(result1.batch_id, 1);

    // Second batch
    let depositor2 = Address::generate(&env);
    let escrow_id2 = create_test_escrow(
        &env,
        &client,
        &token_admin,
        &depositor2,
        &recipient,
        20_000_000,
        20000,
    );
    let mut batch2: Vec<ReversalRequest> = Vec::new(&env);
    batch2.push_back(create_reversal_request(escrow_id2));
    let result2 = client.batch_reverse_escrows(&admin, &batch2);
    assert_eq!(result2.batch_id, 2);

    // Third batch
    let depositor3 = Address::generate(&env);
    let escrow_id3 = create_test_escrow(
        &env,
        &client,
        &token_admin,
        &depositor3,
        &recipient,
        30_000_000,
        20000,
    );
    let mut batch3: Vec<ReversalRequest> = Vec::new(&env);
    batch3.push_back(create_reversal_request(escrow_id3));
    let result3 = client.batch_reverse_escrows(&admin, &batch3);
    assert_eq!(result3.batch_id, 3);
}

// ============================================
// Release Escrow Tests
// ============================================

#[test]
fn test_release_escrow() {
    let (env, admin, _token, _token_client, token_admin, client) = setup_test_env();

    let depositor = Address::generate(&env);
    let recipient = Address::generate(&env);

    let escrow_id =
        create_test_escrow(&env, &client, &token_admin, &depositor, &recipient, 10_000_000, 20000);

    // Release the escrow
    client.release_escrow(&admin, &escrow_id);

    // Check escrow status
    let escrow = client.get_escrow(&escrow_id).unwrap();
    assert_eq!(escrow.status, EscrowStatus::Released);
}

#[test]
#[should_panic]
fn test_release_escrow_already_reversed() {
    let (env, admin, _token, _token_client, token_admin, client) = setup_test_env();

    let depositor = Address::generate(&env);
    let recipient = Address::generate(&env);

    let escrow_id =
        create_test_escrow(&env, &client, &token_admin, &depositor, &recipient, 10_000_000, 20000);

    // Reverse the escrow first
    let mut requests: Vec<ReversalRequest> = Vec::new(&env);
    requests.push_back(create_reversal_request(escrow_id));
    client.batch_reverse_escrows(&admin, &requests);

    // Try to release - should panic
    client.release_escrow(&admin, &escrow_id);
}

// ============================================
// Admin Tests
// ============================================

#[test]
fn test_set_admin() {
    let (env, admin, _token, _token_client, _token_admin, client) = setup_test_env();

    let new_admin = Address::generate(&env);
    client.set_admin(&admin, &new_admin);

    assert_eq!(client.get_admin(), new_admin);
}

#[test]
#[should_panic]
fn test_set_admin_unauthorized() {
    let (env, _admin, _token, _token_client, _token_admin, client) = setup_test_env();

    let unauthorized = Address::generate(&env);
    let new_admin = Address::generate(&env);

    // Should panic due to unauthorized caller
    client.set_admin(&unauthorized, &new_admin);
}

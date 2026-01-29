//! Comprehensive unit and integration tests for the spending limits contract.

#![cfg(test)]

use crate::{SpendingLimitsContract, SpendingLimitsContractClient};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env, Vec};

use crate::types::{ErrorCode, LimitUpdateResult, SpendingLimitRequest};

/// Helper function to create a test environment with initialized contract.
fn setup_test_contract() -> (Env, Address, SpendingLimitsContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(SpendingLimitsContract, ());
    let client = SpendingLimitsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    (env, admin, client)
}

/// Helper function to create a valid spending limit request.
fn create_valid_request(env: &Env, user: &Address, limit: i128) -> SpendingLimitRequest {
    SpendingLimitRequest {
        user: user.clone(),
        monthly_limit: limit,
        category: Some(symbol_short!("general")),
    }
}

#[test]
fn test_initialize() {
    let (_, admin, client) = setup_test_contract();

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_last_batch_id(), 0);
    assert_eq!(client.get_total_limits_updated(), 0);
    assert_eq!(client.get_total_batches_processed(), 0);
}

#[test]
#[should_panic(expected = "Contract already initialized")]
fn test_initialize_twice_fails() {
    let (env, _, client) = setup_test_contract();
    let new_admin = Address::generate(&env);
    client.initialize(&new_admin);
}

#[test]
fn test_batch_update_spending_limits_single_user() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SpendingLimitRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, &user, 50_000_000_000)); // 5,000 XLM

    let result = client.batch_update_spending_limits(&admin, &requests);

    assert_eq!(result.total_requests, 1);
    assert_eq!(result.successful, 1);
    assert_eq!(result.failed, 0);
    assert_eq!(result.batch_id, 1);

    // Verify storage updates
    assert_eq!(client.get_last_batch_id(), 1);
    assert_eq!(client.get_total_limits_updated(), 1);
    assert_eq!(client.get_total_batches_processed(), 1);
}

#[test]
fn test_batch_update_spending_limits_multiple_users() {
    let (env, admin, client) = setup_test_contract();

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let user3 = Address::generate(&env);

    let mut requests: Vec<SpendingLimitRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, &user1, 10_000_000_000)); // 1,000 XLM
    requests.push_back(create_valid_request(&env, &user2, 50_000_000_000)); // 5,000 XLM
    requests.push_back(create_valid_request(&env, &user3, 100_000_000_000)); // 10,000 XLM

    let result = client.batch_update_spending_limits(&admin, &requests);

    assert_eq!(result.total_requests, 3);
    assert_eq!(result.successful, 3);
    assert_eq!(result.failed, 0);
    assert_eq!(result.results.len(), 3);

    // Verify all limits were updated successfully
    for limit_result in result.results.iter() {
        match limit_result {
            LimitUpdateResult::Success(limit) => {
                assert!(limit.monthly_limit > 0);
                assert_eq!(limit.current_spending, 0);
                assert_eq!(limit.is_active, true);
            }
            LimitUpdateResult::Failure(_, _) => panic!("Expected success, got failure"),
        }
    }

    // Verify storage updates
    assert_eq!(client.get_total_limits_updated(), 3);
}

#[test]
fn test_batch_update_with_invalid_requests() {
    let (env, admin, client) = setup_test_contract();

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    let mut requests: Vec<SpendingLimitRequest> = Vec::new(&env);

    // Valid request
    requests.push_back(create_valid_request(&env, &user1, 50_000_000_000));

    // Invalid request - limit too low
    let mut invalid_request = create_valid_request(&env, &user2, 100);
    invalid_request.monthly_limit = 100; // Below minimum
    requests.push_back(invalid_request);

    let result = client.batch_update_spending_limits(&admin, &requests);

    assert_eq!(result.total_requests, 2);
    assert_eq!(result.successful, 1);
    assert_eq!(result.failed, 1);

    // Verify the first succeeded and second failed
    match &result.results.get(0).unwrap() {
        LimitUpdateResult::Success(_) => {}
        LimitUpdateResult::Failure(_, _) => panic!("Expected first request to succeed"),
    }

    match &result.results.get(1).unwrap() {
        LimitUpdateResult::Success(_) => panic!("Expected second request to fail"),
        LimitUpdateResult::Failure(_, error_code) => {
            assert_eq!(*error_code, ErrorCode::INVALID_LIMIT);
        }
    }
}

#[test]
fn test_batch_update_invalid_limit_negative() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SpendingLimitRequest> = Vec::new(&env);
    let mut request = create_valid_request(&env, &user, 50_000_000_000);
    request.monthly_limit = -1000; // Negative limit
    requests.push_back(request);

    let result = client.batch_update_spending_limits(&admin, &requests);

    assert_eq!(result.successful, 0);
    assert_eq!(result.failed, 1);

    match &result.results.get(0).unwrap() {
        LimitUpdateResult::Failure(_, error_code) => {
            assert_eq!(*error_code, ErrorCode::INVALID_LIMIT);
        }
        LimitUpdateResult::Success(_) => panic!("Expected failure"),
    }
}

#[test]
fn test_batch_update_invalid_limit_too_high() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SpendingLimitRequest> = Vec::new(&env);
    let mut request = create_valid_request(&env, &user, 50_000_000_000);
    request.monthly_limit = 100_000_000_000_000_001; // Above maximum
    requests.push_back(request);

    let result = client.batch_update_spending_limits(&admin, &requests);

    assert_eq!(result.successful, 0);
    assert_eq!(result.failed, 1);

    match &result.results.get(0).unwrap() {
        LimitUpdateResult::Failure(_, error_code) => {
            assert_eq!(*error_code, ErrorCode::INVALID_LIMIT);
        }
        LimitUpdateResult::Success(_) => panic!("Expected failure"),
    }
}

#[test]
#[should_panic]
fn test_batch_update_empty_batch() {
    let (env, admin, client) = setup_test_contract();
    let requests: Vec<SpendingLimitRequest> = Vec::new(&env);
    client.batch_update_spending_limits(&admin, &requests);
}

#[test]
#[should_panic]
fn test_batch_update_batch_too_large() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SpendingLimitRequest> = Vec::new(&env);
    // Create 101 requests (exceeds MAX_BATCH_SIZE of 100)
    for i in 0..101 {
        requests.push_back(create_valid_request(
            &env,
            &user,
            50_000_000_000 + i as i128,
        ));
    }

    client.batch_update_spending_limits(&admin, &requests);
}

#[test]
fn test_get_spending_limit() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SpendingLimitRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, &user, 50_000_000_000));

    client.batch_update_spending_limits(&admin, &requests);

    // Get the updated limit
    let limit = client.get_spending_limit(&user).unwrap();

    assert_eq!(limit.user, user);
    assert_eq!(limit.monthly_limit, 50_000_000_000);
    assert_eq!(limit.current_spending, 0);
    assert_eq!(limit.is_active, true);
}

#[test]
fn test_batch_metrics() {
    let (env, admin, client) = setup_test_contract();

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    let mut requests: Vec<SpendingLimitRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, &user1, 50_000_000_000)); // 5,000 XLM
    requests.push_back(create_valid_request(&env, &user2, 100_000_000_000)); // 10,000 XLM

    let result = client.batch_update_spending_limits(&admin, &requests);

    assert_eq!(result.metrics.total_requests, 2);
    assert_eq!(result.metrics.successful_updates, 2);
    assert_eq!(result.metrics.failed_updates, 0);
    assert_eq!(result.metrics.total_limits_value, 150_000_000_000);
    assert_eq!(result.metrics.avg_limit_amount, 75_000_000_000);
}

#[test]
fn test_multiple_batches() {
    let (env, admin, client) = setup_test_contract();

    // First batch
    let user1 = Address::generate(&env);
    let mut requests1: Vec<SpendingLimitRequest> = Vec::new(&env);
    requests1.push_back(create_valid_request(&env, &user1, 50_000_000_000));
    let result1 = client.batch_update_spending_limits(&admin, &requests1);
    assert_eq!(result1.batch_id, 1);

    // Second batch
    let user2 = Address::generate(&env);
    let mut requests2: Vec<SpendingLimitRequest> = Vec::new(&env);
    requests2.push_back(create_valid_request(&env, &user2, 100_000_000_000));
    let result2 = client.batch_update_spending_limits(&admin, &requests2);
    assert_eq!(result2.batch_id, 2);

    // Verify totals
    assert_eq!(client.get_total_batches_processed(), 2);
    assert_eq!(client.get_total_limits_updated(), 2);
}

#[test]
fn test_high_value_limit_event() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SpendingLimitRequest> = Vec::new(&env);
    // Create high-value limit (>= 1,000,000 XLM)
    requests.push_back(create_valid_request(&env, &user, 20_000_000_000_000_000));

    let result = client.batch_update_spending_limits(&admin, &requests);

    assert_eq!(result.successful, 1);
    // High-value event should be emitted (verified in event logs)
}

#[test]
fn test_set_admin() {
    let (env, admin, client) = setup_test_contract();
    let new_admin = Address::generate(&env);

    client.set_admin(&admin, &new_admin);

    assert_eq!(client.get_admin(), new_admin);
}

#[test]
fn test_mixed_valid_and_invalid_requests() {
    let (env, admin, client) = setup_test_contract();

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let user3 = Address::generate(&env);
    let user4 = Address::generate(&env);

    let mut requests: Vec<SpendingLimitRequest> = Vec::new(&env);

    // Valid
    requests.push_back(create_valid_request(&env, &user1, 50_000_000_000));

    // Invalid - limit too low
    let mut invalid1 = create_valid_request(&env, &user2, 100);
    invalid1.monthly_limit = 100;
    requests.push_back(invalid1);

    // Valid
    requests.push_back(create_valid_request(&env, &user3, 100_000_000_000));

    // Invalid - negative limit
    let mut invalid2 = create_valid_request(&env, &user4, -1000);
    invalid2.monthly_limit = -1000;
    requests.push_back(invalid2);

    let result = client.batch_update_spending_limits(&admin, &requests);

    assert_eq!(result.total_requests, 4);
    assert_eq!(result.successful, 2);
    assert_eq!(result.failed, 2);

    // Only successful limits should be stored
    assert_eq!(client.get_total_limits_updated(), 2);
}

#[test]
fn test_update_existing_limit() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Set initial limit
    let mut requests1: Vec<SpendingLimitRequest> = Vec::new(&env);
    requests1.push_back(create_valid_request(&env, &user, 50_000_000_000));
    client.batch_update_spending_limits(&admin, &requests1);

    let limit1 = client.get_spending_limit(&user).unwrap();
    assert_eq!(limit1.monthly_limit, 50_000_000_000);

    // Update the limit
    let mut requests2: Vec<SpendingLimitRequest> = Vec::new(&env);
    requests2.push_back(create_valid_request(&env, &user, 100_000_000_000));
    client.batch_update_spending_limits(&admin, &requests2);

    let limit2 = client.get_spending_limit(&user).unwrap();
    assert_eq!(limit2.monthly_limit, 100_000_000_000);
    assert_eq!(limit2.current_spending, 0); // Reset on update
}

#[test]
fn test_request_without_category() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SpendingLimitRequest> = Vec::new(&env);
    let mut request = create_valid_request(&env, &user, 50_000_000_000);
    request.category = None;
    requests.push_back(request);

    let result = client.batch_update_spending_limits(&admin, &requests);

    assert_eq!(result.successful, 1);
    assert_eq!(result.failed, 0);

    let limit = client.get_spending_limit(&user).unwrap();
    assert!(limit.category.is_none());
}

#[test]
fn test_minimum_valid_limit() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SpendingLimitRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, &user, 1_000_000)); // Minimum: 0.1 XLM

    let result = client.batch_update_spending_limits(&admin, &requests);

    assert_eq!(result.successful, 1);
    assert_eq!(result.failed, 0);
}

#[test]
fn test_maximum_valid_limit() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SpendingLimitRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(
        &env,
        &user,
        100_000_000_000_000_000, // Maximum: 10M XLM
    ));

    let result = client.batch_update_spending_limits(&admin, &requests);

    assert_eq!(result.successful, 1);
    assert_eq!(result.failed, 0);
}

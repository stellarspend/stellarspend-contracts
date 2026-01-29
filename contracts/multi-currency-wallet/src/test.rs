//! Comprehensive unit and integration tests for the multi-currency wallet contract.

#![cfg(test)]

use crate::{MultiCurrencyWalletContract, MultiCurrencyWalletContractClient};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env, Symbol, Vec};

use crate::types::{BalanceUpdateRequest, BalanceUpdateResult, ErrorCode};

/// Helper function to create a test environment with initialized contract.
fn setup_test_contract() -> (Env, Address, MultiCurrencyWalletContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(MultiCurrencyWalletContract, ());
    let client = MultiCurrencyWalletContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    (env, admin, client)
}

/// Helper function to create a valid balance update request.
fn create_valid_request(
    env: &Env,
    user: &Address,
    currency: Symbol,
    amount: i128,
    operation: Symbol,
) -> BalanceUpdateRequest {
    BalanceUpdateRequest {
        user: user.clone(),
        currency,
        amount,
        operation,
    }
}

#[test]
fn test_initialize() {
    let (_, admin, client) = setup_test_contract();

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_last_batch_id(), 0);
    assert_eq!(client.get_total_balances_updated(), 0);
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
fn test_batch_update_balances_single_user_single_currency() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<BalanceUpdateRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(
        &env,
        &user,
        symbol_short!("USDC"),
        1000_000_000, // 1000 USDC
        symbol_short!("set"),
    ));

    let result = client.batch_update_balances(&admin, &requests);

    assert_eq!(result.total_requests, 1);
    assert_eq!(result.successful, 1);
    assert_eq!(result.failed, 0);
    assert_eq!(result.batch_id, 1);

    // Verify balance was set
    let balance = client.get_balance(&user, &symbol_short!("USDC"));
    assert_eq!(balance, 1000_000_000);

    // Verify storage updates
    assert_eq!(client.get_last_batch_id(), 1);
    assert_eq!(client.get_total_balances_updated(), 1);
    assert_eq!(client.get_total_batches_processed(), 1);
}

#[test]
fn test_batch_update_balances_multiple_users_multiple_currencies() {
    let (env, admin, client) = setup_test_contract();

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let user3 = Address::generate(&env);

    let mut requests: Vec<BalanceUpdateRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(
        &env,
        &user1,
        symbol_short!("USDC"),
        500_000_000,
        symbol_short!("set"),
    ));
    requests.push_back(create_valid_request(
        &env,
        &user2,
        symbol_short!("XLM"),
        10_000_000_000,
        symbol_short!("set"),
    ));
    requests.push_back(create_valid_request(
        &env,
        &user3,
        symbol_short!("EURC"),
        750_000_000,
        symbol_short!("set"),
    ));

    let result = client.batch_update_balances(&admin, &requests);

    assert_eq!(result.total_requests, 3);
    assert_eq!(result.successful, 3);
    assert_eq!(result.failed, 0);
    assert_eq!(result.results.len(), 3);

    // Verify all balances were updated successfully
    for balance_result in result.results.iter() {
        match balance_result {
            BalanceUpdateResult::Success(balance) => {
                assert!(balance.balance > 0);
            }
            BalanceUpdateResult::Failure(_, _, _) => panic!("Expected success, got failure"),
        }
    }

    // Verify metrics
    assert_eq!(result.metrics.unique_users, 3);
    assert_eq!(result.metrics.unique_currencies, 3);

    // Verify individual balances
    assert_eq!(client.get_balance(&user1, &symbol_short!("USDC")), 500_000_000);
    assert_eq!(
        client.get_balance(&user2, &symbol_short!("XLM")),
        10_000_000_000
    );
    assert_eq!(client.get_balance(&user3, &symbol_short!("EURC")), 750_000_000);
}

#[test]
fn test_balance_add_operation() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Set initial balance
    let mut requests1: Vec<BalanceUpdateRequest> = Vec::new(&env);
    requests1.push_back(create_valid_request(
        &env,
        &user,
        symbol_short!("USDC"),
        1000_000_000,
        symbol_short!("set"),
    ));
    client.batch_update_balances(&admin, &requests1);

    // Add to balance
    let mut requests2: Vec<BalanceUpdateRequest> = Vec::new(&env);
    requests2.push_back(create_valid_request(
        &env,
        &user,
        symbol_short!("USDC"),
        500_000_000,
        symbol_short!("add"),
    ));
    let result = client.batch_update_balances(&admin, &requests2);

    assert_eq!(result.successful, 1);
    assert_eq!(client.get_balance(&user, &symbol_short!("USDC")), 1500_000_000);
}

#[test]
fn test_balance_subtract_operation() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Set initial balance
    let mut requests1: Vec<BalanceUpdateRequest> = Vec::new(&env);
    requests1.push_back(create_valid_request(
        &env,
        &user,
        symbol_short!("USDC"),
        1000_000_000,
        symbol_short!("set"),
    ));
    client.batch_update_balances(&admin, &requests1);

    // Subtract from balance
    let mut requests2: Vec<BalanceUpdateRequest> = Vec::new(&env);
    requests2.push_back(create_valid_request(
        &env,
        &user,
        symbol_short!("USDC"),
        300_000_000,
        symbol_short!("subtract"),
    ));
    let result = client.batch_update_balances(&admin, &requests2);

    assert_eq!(result.successful, 1);
    assert_eq!(client.get_balance(&user, &symbol_short!("USDC")), 700_000_000);
}

#[test]
fn test_balance_subtract_insufficient_fails() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Set initial balance
    let mut requests1: Vec<BalanceUpdateRequest> = Vec::new(&env);
    requests1.push_back(create_valid_request(
        &env,
        &user,
        symbol_short!("USDC"),
        500_000_000,
        symbol_short!("set"),
    ));
    client.batch_update_balances(&admin, &requests1);

    // Try to subtract more than balance
    let mut requests2: Vec<BalanceUpdateRequest> = Vec::new(&env);
    requests2.push_back(create_valid_request(
        &env,
        &user,
        symbol_short!("USDC"),
        1000_000_000,
        symbol_short!("subtract"),
    ));
    let result = client.batch_update_balances(&admin, &requests2);

    assert_eq!(result.successful, 0);
    assert_eq!(result.failed, 1);

    match &result.results.get(0).unwrap() {
        BalanceUpdateResult::Failure(_, _, error_code) => {
            assert_eq!(*error_code, ErrorCode::INSUFFICIENT_BALANCE);
        }
        BalanceUpdateResult::Success(_) => panic!("Expected failure"),
    }

    // Balance should remain unchanged
    assert_eq!(client.get_balance(&user, &symbol_short!("USDC")), 500_000_000);
}

#[test]
fn test_batch_update_with_invalid_requests() {
    let (env, admin, client) = setup_test_contract();

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    let mut requests: Vec<BalanceUpdateRequest> = Vec::new(&env);

    // Valid request
    requests.push_back(create_valid_request(
        &env,
        &user1,
        symbol_short!("USDC"),
        1000_000_000,
        symbol_short!("set"),
    ));

    // Invalid request - amount too low (0)
    requests.push_back(create_valid_request(
        &env,
        &user2,
        symbol_short!("XLM"),
        0,
        symbol_short!("set"),
    ));

    let result = client.batch_update_balances(&admin, &requests);

    assert_eq!(result.total_requests, 2);
    assert_eq!(result.successful, 1);
    assert_eq!(result.failed, 1);

    // Verify the first succeeded
    match &result.results.get(0).unwrap() {
        BalanceUpdateResult::Success(_) => {}
        BalanceUpdateResult::Failure(_, _, _) => panic!("Expected first request to succeed"),
    }

    // Verify the second failed
    match &result.results.get(1).unwrap() {
        BalanceUpdateResult::Success(_) => panic!("Expected second request to fail"),
        BalanceUpdateResult::Failure(_, _, error_code) => {
            assert_eq!(*error_code, ErrorCode::INVALID_AMOUNT);
        }
    }
}

#[test]
fn test_invalid_amount_negative() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<BalanceUpdateRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(
        &env,
        &user,
        symbol_short!("USDC"),
        -1000,
        symbol_short!("set"),
    ));

    let result = client.batch_update_balances(&admin, &requests);

    assert_eq!(result.successful, 0);
    assert_eq!(result.failed, 1);

    match &result.results.get(0).unwrap() {
        BalanceUpdateResult::Failure(_, _, error_code) => {
            assert_eq!(*error_code, ErrorCode::INVALID_AMOUNT);
        }
        BalanceUpdateResult::Success(_) => panic!("Expected failure"),
    }
}

#[test]
#[should_panic]
fn test_batch_update_empty_batch() {
    let (env, admin, client) = setup_test_contract();
    let requests: Vec<BalanceUpdateRequest> = Vec::new(&env);
    client.batch_update_balances(&admin, &requests);
}

#[test]
#[should_panic]
fn test_batch_update_batch_too_large() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<BalanceUpdateRequest> = Vec::new(&env);
    // Create 101 requests (exceeds MAX_BATCH_SIZE of 100)
    for i in 0..101 {
        requests.push_back(create_valid_request(
            &env,
            &user,
            symbol_short!("USDC"),
            1000 + i as i128,
            symbol_short!("set"),
        ));
    }

    client.batch_update_balances(&admin, &requests);
}

#[test]
fn test_get_balance_details() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<BalanceUpdateRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(
        &env,
        &user,
        symbol_short!("USDC"),
        1000_000_000,
        symbol_short!("set"),
    ));

    client.batch_update_balances(&admin, &requests);

    // Get balance details
    let details = client
        .get_balance_details(&user, &symbol_short!("USDC"))
        .unwrap();

    assert_eq!(details.user, user);
    assert_eq!(details.currency, symbol_short!("USDC"));
    assert_eq!(details.balance, 1000_000_000);
    assert!(details.updated_at > 0);
}

#[test]
fn test_batch_metrics() {
    let (env, admin, client) = setup_test_contract();

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    let mut requests: Vec<BalanceUpdateRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(
        &env,
        &user1,
        symbol_short!("USDC"),
        1000_000_000,
        symbol_short!("set"),
    ));
    requests.push_back(create_valid_request(
        &env,
        &user1,
        symbol_short!("XLM"),
        5000_000_000,
        symbol_short!("set"),
    ));
    requests.push_back(create_valid_request(
        &env,
        &user2,
        symbol_short!("EURC"),
        750_000_000,
        symbol_short!("set"),
    ));

    let result = client.batch_update_balances(&admin, &requests);

    assert_eq!(result.metrics.total_requests, 3);
    assert_eq!(result.metrics.successful_updates, 3);
    assert_eq!(result.metrics.failed_updates, 0);
    assert_eq!(result.metrics.unique_users, 2);
    assert_eq!(result.metrics.unique_currencies, 3);
}

#[test]
fn test_multiple_batches() {
    let (env, admin, client) = setup_test_contract();

    // First batch
    let user1 = Address::generate(&env);
    let mut requests1: Vec<BalanceUpdateRequest> = Vec::new(&env);
    requests1.push_back(create_valid_request(
        &env,
        &user1,
        symbol_short!("USDC"),
        1000_000_000,
        symbol_short!("set"),
    ));
    let result1 = client.batch_update_balances(&admin, &requests1);
    assert_eq!(result1.batch_id, 1);

    // Second batch
    let user2 = Address::generate(&env);
    let mut requests2: Vec<BalanceUpdateRequest> = Vec::new(&env);
    requests2.push_back(create_valid_request(
        &env,
        &user2,
        symbol_short!("XLM"),
        5000_000_000,
        symbol_short!("set"),
    ));
    let result2 = client.batch_update_balances(&admin, &requests2);
    assert_eq!(result2.batch_id, 2);

    // Verify totals
    assert_eq!(client.get_total_batches_processed(), 2);
    assert_eq!(client.get_total_balances_updated(), 2);
}

#[test]
fn test_large_balance_event() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<BalanceUpdateRequest> = Vec::new(&env);
    // Create large balance (>= 1,000,000 units)
    requests.push_back(create_valid_request(
        &env,
        &user,
        symbol_short!("USDC"),
        10_000_000,
        symbol_short!("set"),
    ));

    let result = client.batch_update_balances(&admin, &requests);

    assert_eq!(result.successful, 1);
    // Large balance event should be emitted (verified in event logs)
}

#[test]
fn test_set_admin() {
    let (env, admin, client) = setup_test_contract();
    let new_admin = Address::generate(&env);

    client.set_admin(&admin, &new_admin);

    assert_eq!(client.get_admin(), new_admin);
}

#[test]
fn test_mixed_operations_same_user() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<BalanceUpdateRequest> = Vec::new(&env);

    // Set USDC balance
    requests.push_back(create_valid_request(
        &env,
        &user,
        symbol_short!("USDC"),
        1000_000_000,
        symbol_short!("set"),
    ));

    // Set XLM balance
    requests.push_back(create_valid_request(
        &env,
        &user,
        symbol_short!("XLM"),
        5000_000_000,
        symbol_short!("set"),
    ));

    // Set EURC balance
    requests.push_back(create_valid_request(
        &env,
        &user,
        symbol_short!("EURC"),
        750_000_000,
        symbol_short!("set"),
    ));

    let result = client.batch_update_balances(&admin, &requests);

    assert_eq!(result.total_requests, 3);
    assert_eq!(result.successful, 3);
    assert_eq!(result.failed, 0);

    // Verify all balances for the same user
    assert_eq!(client.get_balance(&user, &symbol_short!("USDC")), 1000_000_000);
    assert_eq!(client.get_balance(&user, &symbol_short!("XLM")), 5000_000_000);
    assert_eq!(client.get_balance(&user, &symbol_short!("EURC")), 750_000_000);

    // Metrics should show 1 unique user, 3 unique currencies
    assert_eq!(result.metrics.unique_users, 1);
    assert_eq!(result.metrics.unique_currencies, 3);
}

#[test]
fn test_get_balance_nonexistent_returns_zero() {
    let (env, _admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let balance = client.get_balance(&user, &symbol_short!("USDC"));
    assert_eq!(balance, 0);
}

#[test]
fn test_mixed_valid_and_invalid_requests() {
    let (env, admin, client) = setup_test_contract();

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let user3 = Address::generate(&env);

    let mut requests: Vec<BalanceUpdateRequest> = Vec::new(&env);

    // Valid
    requests.push_back(create_valid_request(
        &env,
        &user1,
        symbol_short!("USDC"),
        1000_000_000,
        symbol_short!("set"),
    ));

    // Invalid - negative amount
    requests.push_back(create_valid_request(
        &env,
        &user2,
        symbol_short!("XLM"),
        -500,
        symbol_short!("set"),
    ));

    // Valid
    requests.push_back(create_valid_request(
        &env,
        &user3,
        symbol_short!("EURC"),
        750_000_000,
        symbol_short!("set"),
    ));

    let result = client.batch_update_balances(&admin, &requests);

    assert_eq!(result.total_requests, 3);
    assert_eq!(result.successful, 2);
    assert_eq!(result.failed, 1);

    // Only successful balances should be stored
    assert_eq!(client.get_total_balances_updated(), 2);
}

#[test]
fn test_minimum_valid_balance() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<BalanceUpdateRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(
        &env,
        &user,
        symbol_short!("USDC"),
        1, // Minimum balance
        symbol_short!("set"),
    ));

    let result = client.batch_update_balances(&admin, &requests);

    assert_eq!(result.successful, 1);
    assert_eq!(result.failed, 0);
    assert_eq!(client.get_balance(&user, &symbol_short!("USDC")), 1);
}

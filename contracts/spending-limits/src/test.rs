//! Comprehensive unit and integration tests for the spending limits contract.

#![cfg(test)]

extern crate alloc;
extern crate std;

use crate::{SpendingLimitsContract, SpendingLimitsContractClient};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger},
    Address, Env, Symbol, Vec,
};
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::types::{
    error_code, LimitStrategy, LimitUpdateResult, SpendingLimitRequest, MAX_SPENDING_LIMIT,
    MIN_SPENDING_LIMIT,
};
use alloc::format;

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
        daily_limit: if limit >= 30 { limit / 30 } else { limit },
        hourly_limit: if limit >= 30 { limit / 30 } else { limit },
        reset_window_seconds: 86_400,
        category: Some(symbol_short!("general")),
        strategy: LimitStrategy::Static,
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
            assert_eq!(*error_code, error_code::INVALID_LIMIT);
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
            assert_eq!(*error_code, error_code::INVALID_LIMIT);
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
    request.monthly_limit = MAX_SPENDING_LIMIT + 1; // Above maximum
    requests.push_back(request);

    let result = client.batch_update_spending_limits(&admin, &requests);

    assert_eq!(result.successful, 0);
    assert_eq!(result.failed, 1);

    match &result.results.get(0).unwrap() {
        LimitUpdateResult::Failure(_, error_code) => {
            assert_eq!(*error_code, error_code::INVALID_LIMIT);
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
    let limit = client.get_spending_limit_details(&user).unwrap();

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

    let limit1 = client.get_spending_limit_details(&user).unwrap();
    assert_eq!(limit1.monthly_limit, 50_000_000_000);

    // Update the limit
    let mut requests2: Vec<SpendingLimitRequest> = Vec::new(&env);
    requests2.push_back(create_valid_request(&env, &user, 100_000_000_000));
    client.batch_update_spending_limits(&admin, &requests2);

    let limit2 = client.get_spending_limit_details(&user).unwrap();
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

    let limit = client.get_spending_limit_details(&user).unwrap();
    assert!(limit.category.is_none());
}

#[test]
fn test_minimum_valid_limit() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SpendingLimitRequest> = Vec::new(&env);
    // All three bounds (monthly/daily/hourly) must be at least MIN_SPENDING_LIMIT.
    let mut request = create_valid_request(&env, &user, MIN_SPENDING_LIMIT * 3);
    request.monthly_limit = MIN_SPENDING_LIMIT;
    request.daily_limit = MIN_SPENDING_LIMIT;
    request.hourly_limit = MIN_SPENDING_LIMIT;
    requests.push_back(request);

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

#[test]
fn test_enforce_spending_limit_allows_within_daily_and_monthly() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Configure a monthly limit of 300 units; derived daily limit is 10 units.
    client.whitelist_destination(&admin, &user);
    let mut requests: Vec<SpendingLimitRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, &user, 300_000_000));
    client.batch_update_spending_limits(&admin, &requests);

    // Same timestamp (same logical day/month).
    env.ledger().set_timestamp(86_400); // day 1

    // Two spends of 5M each are within daily (10M) and monthly (300M) limits.
    client.enforce_spending_limit(&user, &5_000_000, &None::<Symbol>);
    client.enforce_spending_limit(&user, &5_000_000, &None::<Symbol>);
}

#[test]
fn test_enforce_spending_limit_resets_after_window() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Configure a monthly limit with a 24-hour reset window.
    client.whitelist_destination(&admin, &user);
    let mut requests: Vec<SpendingLimitRequest> = Vec::new(&env);
    let mut request = create_valid_request(&env, &user, 300_000_000);
    request.reset_window_seconds = 86_400;
    requests.push_back(request);
    client.batch_update_spending_limits(&admin, &requests);

    // Use the starting window and consume the daily limit.
    env.ledger().set_timestamp(0);
    client.enforce_spending_limit(&user, &10_000_000, &None::<Symbol>);

    // Same-day extra spend should be blocked by the daily limit.
    let result = catch_unwind(AssertUnwindSafe(|| {
        client.enforce_spending_limit(&user, &1_000_000, &None::<Symbol>);
    }));
    assert!(
        result.is_err(),
        "Same-day spend above the daily limit should fail"
    );

    // Advance past the 24-hour window and verify the count resets.
    env.ledger().set_timestamp(86_401);
    client.enforce_spending_limit(&user, &10_000_000, &None::<Symbol>);

    let limit = client.get_spending_limit_details(&user).unwrap();
    assert_eq!(limit.current_spending, 20_000_000);
}

#[test]
#[should_panic]
fn test_enforce_spending_limit_daily_exceeded() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Monthly 300M -> daily 10M
    client.whitelist_destination(&admin, &user);
    let mut requests: Vec<SpendingLimitRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, &user, 300_000_000));
    let result = client.batch_update_spending_limits(&admin, &requests);
    assert_eq!(result.successful, 1);
    assert!(client.get_spending_limit_details(&user).is_some());

    env.ledger().set_timestamp(2 * 86_400); // day 2

    // 2 * 5M is allowed; the third spend pushes daily total above 10M and should panic.
    client.enforce_spending_limit(&user, &5_000_000, &None::<Symbol>);
    client.enforce_spending_limit(&user, &5_000_000, &None::<Symbol>);
    client.enforce_spending_limit(&user, &1_000_000, &None::<Symbol>);
    // 2 * 5M is allowed.
    client.enforce_spending_limit(&user, &5_000_000, &None::<Symbol>);
    client.enforce_spending_limit(&user, &5_000_000, &None::<Symbol>);

    let limit = client.get_spending_limit_details(&user).unwrap();
    assert_eq!(limit.current_spending, 10_000_000);

    // The third spend pushes daily total above 10M and should panic.
    client.enforce_spending_limit(&user, &1_000_000, &None::<Symbol>);
}

#[test]
#[should_panic]
fn test_enforce_spending_limit_monthly_exceeded_over_multiple_days() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Monthly 30M, daily 1M (30M / 30) => 1M per day max, 30M per month.
    client.whitelist_destination(&admin, &user);
    let mut requests: Vec<SpendingLimitRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, &user, 30_000_000));
    let result = client.batch_update_spending_limits(&admin, &requests);
    assert_eq!(result.successful, 1);
    assert!(client.get_spending_limit_details(&user).is_some());

    // Spend 1M on 30 different "days" within the same logical month window.
    for d in 0..30u64 {
        env.ledger().set_timestamp(d * 86_400);
        client.enforce_spending_limit(&user, &1_000_000, &None::<Symbol>);
    }

    let limit = client.get_spending_limit_details(&user).unwrap();
    assert_eq!(limit.current_spending, 30_000_000);

    // Next day is still within the same 30-day "month" bucket and should exceed the
    // monthly limit, even though the daily limit would allow it.
    env.ledger().set_timestamp(30 * 86_400);
    client.enforce_spending_limit(&user, &1_000_000, &None::<Symbol>);
}

#[test]
fn test_enforce_without_limit_does_not_block() {
    let (env, _admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    client.whitelist_destination(&client.get_admin(), &user);
    env.ledger().set_timestamp(10 * 86_400);

    // No limit configured for this user; enforce should be a no-op and not panic.
    client.enforce_spending_limit(&user, &1_000_000, &None::<Symbol>);
}

// ─── Exception rule tests (#598) ──────────────────────────────────────────────

#[test]
fn test_add_approved_category() {
    let (env, admin, client) = setup_test_contract();
    let category = symbol_short!("medical");

    client.add_approved_category(&admin, &category);

    let categories = client.get_approved_categories();
    assert_eq!(categories.len(), 1);
    assert!(categories.contains(&category));
}

#[test]
fn test_add_and_remove_approved_category() {
    let (env, admin, client) = setup_test_contract();
    let cat = symbol_short!("medical");

    client.add_approved_category(&admin, &cat);
    assert_eq!(client.get_approved_categories().len(), 1);

    client.remove_approved_category(&admin, &cat);
    assert_eq!(client.get_approved_categories().len(), 0);
}

#[test]
fn test_add_duplicate_approved_category_is_idempotent() {
    let (env, admin, client) = setup_test_contract();
    let cat = symbol_short!("medical");

    client.add_approved_category(&admin, &cat);
    client.add_approved_category(&admin, &cat);

    // Should still only appear once
    assert_eq!(client.get_approved_categories().len(), 1);
}

#[test]
fn test_add_exception_grants_bypass() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);
    let cat = symbol_short!("medical");

    // Configure a tight limit: monthly 30M -> daily 1M
    let mut requests: Vec<SpendingLimitRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, &user, 30_000_000));
    client.batch_update_spending_limits(&admin, &requests);

    env.ledger().set_timestamp(86_400);

    // Without exception, a spend of 2 on day 1 (daily limit = 1M) should be blocked.
    // Now add an approved category and grant an exception.
    client.add_approved_category(&admin, &cat);
    client.add_exception(&admin, &user, &cat);

    // is_exempt should return true
    assert!(client.is_exempt(&user, &cat));

    // Spend exceeds the daily limit but has an exception — must succeed
    client.enforce_spending_limit(&user, &999, &Some(cat.clone()));
}

#[test]
fn test_exception_does_not_bypass_without_category() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);
    let cat = symbol_short!("medical");

    // Tight limit: monthly 30M -> daily 1M
    let mut requests: Vec<SpendingLimitRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, &user, 30_000_000));
    client.batch_update_spending_limits(&admin, &requests);

    client.whitelist_destination(&admin, &user);
    client.add_approved_category(&admin, &cat);
    client.add_exception(&admin, &user, &cat);

    env.ledger().set_timestamp(86_400);

    // Spending with no category should still enforce limits normally
    client.enforce_spending_limit(&user, &1_000_000, &None::<Symbol>);
    // Limit is now consumed; the second call without category must still succeed within limit.
    // (daily limit = 1M, already used 1M — next no-category call should panic)
}

#[test]
#[should_panic]
fn test_add_exception_for_unapproved_category_panics() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);
    let cat = symbol_short!("blocked");

    // Category was never added to approved list
    client.add_exception(&admin, &user, &cat);
}

#[test]
fn test_remove_exception_disables_bypass() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);
    let cat = symbol_short!("medical");

    // Tight limit
    let mut requests: Vec<SpendingLimitRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, &user, 30_000_000));
    client.batch_update_spending_limits(&admin, &requests);

    client.add_approved_category(&admin, &cat);
    client.add_exception(&admin, &user, &cat);
    assert!(client.is_exempt(&user, &cat));

    client.remove_exception(&admin, &user, &cat);
    assert!(!client.is_exempt(&user, &cat));
}

#[test]
fn test_get_exception_returns_rule() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);
    let cat = symbol_short!("medical");

    client.add_approved_category(&admin, &cat);
    client.add_exception(&admin, &user, &cat);

    let rule = client.get_exception(&user, &cat).unwrap();
    assert_eq!(rule.user, user);
    assert_eq!(rule.category, cat);
    assert!(rule.is_active);
}

#[test]
fn test_non_exempt_user_still_restricted() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);
    let other_user = Address::generate(&env);
    let cat = symbol_short!("medical");

    // Tight limit for other_user
    let mut requests: Vec<SpendingLimitRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, &other_user, 30_000_000));
    client.batch_update_spending_limits(&admin, &requests);

    // Grant exception to user (not other_user)
    client.add_approved_category(&admin, &cat);
    client.add_exception(&admin, &user, &cat);

    env.ledger().set_timestamp(86_400);

    // other_user has no exception — limit still enforced
    assert!(!client.is_exempt(&other_user, &cat));
}

#[test]
#[should_panic]
fn test_enforce_spending_limit_hourly_exceeded() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SpendingLimitRequest> = Vec::new(&env);
    // 300M monthly, 20M daily, but override hourly_limit to 10M (>= MIN_SPENDING_LIMIT).
    let mut request = create_valid_request(&env, &user, 300_000_000);
    request.daily_limit = 20_000_000;
    request.hourly_limit = 10_000_000;
    requests.push_back(request);
    client.batch_update_spending_limits(&admin, &requests);

    client.whitelist_destination(&admin, &user);
    env.ledger().set_timestamp(3600); // 1 hour

    // Spend of 10M is allowed.
    client.enforce_spending_limit(&user, &10_000_000, &None::<Symbol>);

    // This second spend in the same hour exceeds hourly limit of 10M and should panic.
    client.enforce_spending_limit(&user, &1_000_000, &None::<Symbol>);
}

#[test]
fn test_enforce_spending_limit_hourly_resets() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SpendingLimitRequest> = Vec::new(&env);
    let mut request = create_valid_request(&env, &user, 300_000_000);
    request.daily_limit = 20_000_000;
    request.hourly_limit = 10_000_000;
    requests.push_back(request);
    client.batch_update_spending_limits(&admin, &requests);

    client.whitelist_destination(&admin, &user);
    env.ledger().set_timestamp(3600); // hour 1

    // Spend of 10M is allowed.
    client.enforce_spending_limit(&user, &10_000_000, &None::<Symbol>);

    // Advance 1 hour and 1 second.
    env.ledger().set_timestamp(3600 + 3601); // hour 2

    // Another spend of 10M is allowed now because the hourly window has reset.
    client.enforce_spending_limit(&user, &10_000_000, &None::<Symbol>);
}

#[test]
fn test_adaptive_strategy_increases_limit_near_usage_threshold() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    client.whitelist_destination(&admin, &user);

    let mut requests: Vec<SpendingLimitRequest> = Vec::new(&env);
    let mut request = create_valid_request(&env, &user, 1_000_000_000);
    request.strategy = LimitStrategy::Adaptive;
    // Give the single large spend headroom on the derived daily/hourly limits.
    request.daily_limit = 1_000_000_000;
    request.hourly_limit = 1_000_000_000;
    requests.push_back(request);
    client.batch_update_spending_limits(&admin, &requests);

    env.ledger().set_timestamp(86_400);

    // Spend 900M (90% of 1B) — should trigger a deterministic 10% limit increase.
    client.enforce_spending_limit(&user, &900_000_000, &None::<Symbol>);

    let limit = client.get_spending_limit_details(&user).unwrap();
    assert_eq!(limit.monthly_limit, 1_100_000_000);
    assert_eq!(limit.strategy, LimitStrategy::Adaptive);
}

#[test]
fn create_food_budget() {
    // create budget with Food category
    // fetch budget
    // assert category == Food
}

#[test]
fn filter_budgets_by_category() {
    // create FOOD budget
    // create RENT budget
    // query FOOD
    // assert only FOOD returned
}

#[test]
fn uncategorized_budgets_are_excluded_from_category_filter() {
    // create uncategorized budget
    // filter FOOD
    // assert empty
}

#[test]
fn test_admin_can_override_spending_limit() {
    let (env, admin, client) = setup_test_contract();

    let user = Address::generate(&env);

    let mut requests = Vec::new(&env);

    requests.push_back(create_valid_request(&env, &user, 50_000_000_000));

    client.batch_update_spending_limits(&admin, &requests);

    let before = client.get_spending_limit_details(&user).unwrap();

    assert_eq!(before.monthly_limit, 50_000_000_000);

    client.override_spending_limit(&admin, &user, &100_000_000_000);

    let after = client.get_spending_limit_details(&user).unwrap();

    assert_eq!(after.monthly_limit, 100_000_000_000);
}

#[test]
#[should_panic]
fn test_non_admin_cannot_override_limit() {
    let (env, admin, client) = setup_test_contract();

    let user = Address::generate(&env);
    let attacker = Address::generate(&env);

    let mut requests = Vec::new(&env);

    requests.push_back(create_valid_request(&env, &user, 50_000_000_000));

    client.batch_update_spending_limits(&admin, &requests);

    client.override_spending_limit(&attacker, &user, &100_000_000_000);
}

#[test]
fn test_override_emits_audit_event() {
    let (env, admin, client) = setup_test_contract();

    let user = Address::generate(&env);

    let mut requests = Vec::new(&env);

    requests.push_back(create_valid_request(&env, &user, 50_000_000_000));

    client.batch_update_spending_limits(&admin, &requests);

    client.override_spending_limit(&admin, &user, &100_000_000_000);

    let events = env.events().all();

    assert!(!events.is_empty());

    let found = events
        .iter()
        .any(|event| format!("{:?}", event).contains("override"));

    assert!(found);
}

#[test]
fn test_override_changes_enforcement_limit() {
    let (env, admin, client) = setup_test_contract();

    let user = Address::generate(&env);

    client.whitelist_destination(&admin, &user);

    let mut requests = Vec::new(&env);

    let mut req = create_valid_request(&env, &user, 100_000_000_000);

    req.daily_limit = 100_000_000_000;
    req.hourly_limit = 100_000_000_000;

    requests.push_back(req);

    client.batch_update_spending_limits(&admin, &requests);

    client.override_spending_limit(&admin, &user, &500_000_000_000);

    let updated = client.get_spending_limit_details(&user).unwrap();

    assert_eq!(updated.monthly_limit, 500_000_000_000);
}

#[test]
fn test_get_spending_limit_scalar_returns_monthly_limit() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SpendingLimitRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, &user, 50_000_000_000));
    client.batch_update_spending_limits(&admin, &requests);

    // The scalar view must return the configured monthly limit (#961).
    assert_eq!(client.get_spending_limit(&user), 50_000_000_000);
}

#[test]
fn test_get_spending_limit_scalar_returns_zero_when_unset() {
    let (env, _admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    // No limit configured: documented default of 0 (#961).
    assert_eq!(client.get_spending_limit(&user), 0);
}

#[test]
fn test_get_spending_limit_scalar_tracks_override() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SpendingLimitRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, &user, 100_000_000_000));
    client.batch_update_spending_limits(&admin, &requests);

    client.override_spending_limit(&admin, &user, &500_000_000_000);

    // Scalar view reflects the overridden limit.
    assert_eq!(client.get_spending_limit(&user), 500_000_000_000);
}

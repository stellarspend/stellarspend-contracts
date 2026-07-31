//! Comprehensive unit and integration tests for the savings goals contract.

#![cfg(test)]

use crate::{SavingsGoalsContract, SavingsGoalsContractClient};
use penalty::{PenaltyContract, PenaltyContractClient};
use soroban_sdk::{testutils::Address as _, testutils::Ledger, Address, Bytes, Env, Symbol, Vec};

use crate::types::{
    ErrorCode, GoalResult, MilestoneAchievementRequest, MilestoneResult, SavingsGoalRequest,
};

/// Helper function to create a test environment with initialized contract.
fn setup_test_contract() -> (Env, Address, SavingsGoalsContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    // Register and configure penalty contract
    let penalty_id = env.register(PenaltyContract, ());
    let penalty_client = PenaltyContractClient::new(&env, &penalty_id);
    let penalty_admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    penalty_client.initialize(&penalty_admin, &10, &treasury);

    let contract_id = env.register(SavingsGoalsContract, ());
    let client = SavingsGoalsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    // Wire penalty contract into savings-goals
    client.set_penalty_contract(&admin, &penalty_id);

    (env, admin, client)
}

/// Helper function to create a valid savings goal request.
fn create_valid_request(
    env: &Env,
    user: &Address,
    goal_name: &str,
    amount: i128,
) -> SavingsGoalRequest {
    let current_ledger = env.ledger().sequence() as u64;
    SavingsGoalRequest {
        user: user.clone(),
        goal_name: Symbol::new(env, goal_name),
        target_amount: amount,
        deadline: current_ledger + 1000,
        initial_contribution: amount / 10, // 10% initial contribution
        priority: 1,
        lock_duration_seconds: 0,
        penalty_bps: 0,
        expiration_seconds: 0,
    }
}

fn idempotency_token(env: &Env, seed: u8) -> Bytes {
    let mut token = Bytes::new(env);
    token.push_back(seed);
    token
}

#[test]
fn test_initialize() {
    let (_, admin, client) = setup_test_contract();

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_last_batch_id(), 0);
    assert_eq!(client.get_last_goal_id(), 0);
    assert_eq!(client.get_total_goals_created(), 0);
    assert_eq!(client.get_total_batches_processed(), 0);
    assert_eq!(client.get_last_milestone_id(), 0);
    assert_eq!(client.get_total_milestones_achieved(), 0);
}

#[test]
#[should_panic(expected = "Contract already initialized")]
fn test_initialize_twice_fails() {
    let (env, _, client) = setup_test_contract();
    let new_admin = Address::generate(&env);
    client.initialize(&new_admin);
}

#[test]
fn test_auto_milestone_events() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);
    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    requests.push_back(SavingsGoalRequest {
        user: user.clone(),
        goal_name: Symbol::new(&env, "auto_milestone"),
        target_amount: 100_000_000,
        deadline: env.ledger().sequence() as u64 + 1000,
        initial_contribution: 25_000_000,
        priority: 1,
        lock_duration_seconds: 0,
        penalty_bps: 0,
        expiration_seconds: 0,
    });
    let result = client.batch_set_savings_goals(&admin, &requests);
    assert_eq!(result.successful, 1);

    client.contribute_to_goal(&user, &1, &25_000_000, &idempotency_token(&env, 1));
    client.contribute_to_goal(&user, &1, &25_000_000, &idempotency_token(&env, 2));
    client.contribute_to_goal(&user, &1, &25_000_000, &idempotency_token(&env, 3));

    let triggered = client.get_triggered_milestone_percents(&1);
    assert_eq!(triggered.len(), 4);
    assert!(triggered.contains(&25));
    assert!(triggered.contains(&50));
    assert!(triggered.contains(&75));
    assert!(triggered.contains(&100));

    client.check_and_emit_milestones(&1);
    let triggered_after = client.get_triggered_milestone_percents(&1);
    assert_eq!(triggered_after.len(), 4);
}

#[test]
fn test_batch_set_savings_goals_single_user() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, &user, "vacation", 100_000_000));

    let result = client.batch_set_savings_goals(&admin, &requests);

    assert_eq!(result.total_requests, 1);
    assert_eq!(result.successful, 1);
    assert_eq!(result.failed, 0);
    assert_eq!(result.batch_id, 1);

    // Verify storage updates
    assert_eq!(client.get_last_batch_id(), 1);
    assert_eq!(client.get_last_goal_id(), 1);
    assert_eq!(client.get_total_goals_created(), 1);
    assert_eq!(client.get_total_batches_processed(), 1);
}

#[test]
fn test_batch_set_savings_goals_multiple_users() {
    let (env, admin, client) = setup_test_contract();

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let user3 = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, &user1, "vacation", 100_000_000));
    requests.push_back(create_valid_request(&env, &user2, "house", 500_000_000));
    requests.push_back(create_valid_request(&env, &user3, "emergency", 200_000_000));

    let result = client.batch_set_savings_goals(&admin, &requests);

    assert_eq!(result.total_requests, 3);
    assert_eq!(result.successful, 3);
    assert_eq!(result.failed, 0);
    assert_eq!(result.results.len(), 3);

    // Verify all goals were created successfully
    for goal_result in result.results.iter() {
        match goal_result {
            GoalResult::Success(goal) => {
                assert!(goal.goal_id > 0);
                assert!(goal.target_amount > 0);
                assert_eq!(goal.is_active, true);
            }
            GoalResult::Failure(_, _) => panic!("Expected success, got failure"),
        }
    }

    // Verify storage updates
    assert_eq!(client.get_total_goals_created(), 3);
    assert_eq!(client.get_last_goal_id(), 3);
}

#[test]
fn test_batch_set_savings_goals_with_invalid_requests() {
    let (env, admin, client) = setup_test_contract();

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);

    // Valid request
    requests.push_back(create_valid_request(&env, &user1, "vacation", 100_000_000));

    // Invalid request - amount too low
    let mut invalid_request = create_valid_request(&env, &user2, "test", 1000);
    invalid_request.target_amount = 1000; // Below minimum
    requests.push_back(invalid_request);

    let result = client.batch_set_savings_goals(&admin, &requests);

    assert_eq!(result.total_requests, 2);
    assert_eq!(result.successful, 1);
    assert_eq!(result.failed, 1);

    // Verify the first succeeded and second failed
    match &result.results.get(0).unwrap() {
        GoalResult::Success(_) => {}
        GoalResult::Failure(_, _) => panic!("Expected first request to succeed"),
    }

    match &result.results.get(1).unwrap() {
        GoalResult::Success(_) => panic!("Expected second request to fail"),
        GoalResult::Failure(_, error_code) => {
            assert_eq!(*error_code, ErrorCode::INVALID_AMOUNT);
        }
    }
}

#[test]
fn test_batch_set_savings_goals_invalid_deadline() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    let mut request = create_valid_request(&env, &user, "vacation", 100_000_000);
    request.deadline = 0; // Past deadline
    requests.push_back(request);

    let result = client.batch_set_savings_goals(&admin, &requests);

    assert_eq!(result.successful, 0);
    assert_eq!(result.failed, 1);

    match &result.results.get(0).unwrap() {
        GoalResult::Failure(_, error_code) => {
            assert_eq!(*error_code, ErrorCode::INVALID_DEADLINE);
        }
        GoalResult::Success(_) => panic!("Expected failure"),
    }
}

#[test]
fn test_batch_set_savings_goals_invalid_initial_contribution() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    let mut request = create_valid_request(&env, &user, "vacation", 100_000_000);
    request.initial_contribution = -1000; // Negative contribution
    requests.push_back(request);

    let result = client.batch_set_savings_goals(&admin, &requests);

    assert_eq!(result.successful, 0);
    assert_eq!(result.failed, 1);

    match &result.results.get(0).unwrap() {
        GoalResult::Failure(_, error_code) => {
            assert_eq!(*error_code, ErrorCode::INVALID_INITIAL_CONTRIBUTION);
        }
        GoalResult::Success(_) => panic!("Expected failure"),
    }
}

#[test]
#[should_panic]
fn test_batch_set_savings_goals_empty_batch() {
    let (env, admin, client) = setup_test_contract();
    let requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    client.batch_set_savings_goals(&admin, &requests);
}

#[test]
#[should_panic]
fn test_batch_set_savings_goals_batch_too_large() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    // Create 101 requests (exceeds MAX_BATCH_SIZE of 100)
    for i in 0..101 {
        requests.push_back(create_valid_request(
            &env,
            &user,
            "goal",
            100_000_000 + i as i128,
        ));
    }

    client.batch_set_savings_goals(&admin, &requests);
}

#[test]
fn test_get_goal() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, &user, "vacation", 100_000_000));

    let _result = client.batch_set_savings_goals(&admin, &requests);

    // Get the created goal
    let goal = client.get_goal(&1).unwrap();

    assert_eq!(goal.goal_id, 1);
    assert_eq!(goal.user, user);
    assert_eq!(goal.target_amount, 100_000_000);
    assert_eq!(goal.current_amount, 10_000_000); // 10% initial
    assert_eq!(goal.is_active, true);
}

#[test]
fn test_get_goal_progress_and_completion() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, &user, "vacation", 100_000_000));

    client.batch_set_savings_goals(&admin, &requests);

    let progress = client.get_goal_progress(&1).unwrap();
    assert_eq!(progress.goal_id, 1);
    assert_eq!(progress.current_amount, 10_000_000);
    assert_eq!(progress.target_amount, 100_000_000);
    assert_eq!(progress.progress_percentage, 10);
    assert_eq!(progress.is_complete, false);

    client.test_set_goal_current_amount(&1, &100_000_000);

    let completed_progress = client.get_goal_progress(&1).unwrap();
    assert_eq!(completed_progress.goal_id, 1);
    assert_eq!(completed_progress.current_amount, 100_000_000);
    assert_eq!(completed_progress.target_amount, 100_000_000);
    assert_eq!(completed_progress.progress_percentage, 100);
    assert_eq!(completed_progress.is_complete, true);

    let completed_goal = client.get_goal(&1).unwrap();
    assert_eq!(completed_goal.is_complete, true);
}

#[test]
fn test_get_user_goals() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, &user, "vacation", 100_000_000));
    requests.push_back(create_valid_request(&env, &user, "house", 500_000_000));

    client.batch_set_savings_goals(&admin, &requests);

    let user_goals = client.get_user_goals(&user);
    assert_eq!(user_goals.len(), 2);
    assert_eq!(user_goals.get(0).unwrap(), 1);
    assert_eq!(user_goals.get(1).unwrap(), 2);
}

#[test]
fn test_batch_metrics() {
    let (env, admin, client) = setup_test_contract();

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, &user1, "vacation", 100_000_000));
    requests.push_back(create_valid_request(&env, &user2, "house", 200_000_000));

    let result = client.batch_set_savings_goals(&admin, &requests);

    assert_eq!(result.metrics.total_requests, 2);
    assert_eq!(result.metrics.successful_goals, 2);
    assert_eq!(result.metrics.failed_goals, 0);
    assert_eq!(result.metrics.total_target_amount, 300_000_000);
    assert_eq!(result.metrics.total_initial_contributions, 30_000_000);
    assert_eq!(result.metrics.avg_goal_amount, 150_000_000);
}

#[test]
fn test_multiple_batches() {
    let (env, admin, client) = setup_test_contract();

    // First batch
    let user1 = Address::generate(&env);
    let mut requests1: Vec<SavingsGoalRequest> = Vec::new(&env);
    requests1.push_back(create_valid_request(&env, &user1, "vacation", 100_000_000));
    let result1 = client.batch_set_savings_goals(&admin, &requests1);
    assert_eq!(result1.batch_id, 1);

    // Second batch
    let user2 = Address::generate(&env);
    let mut requests2: Vec<SavingsGoalRequest> = Vec::new(&env);
    requests2.push_back(create_valid_request(&env, &user2, "house", 500_000_000));
    let result2 = client.batch_set_savings_goals(&admin, &requests2);
    assert_eq!(result2.batch_id, 2);

    // Verify totals
    assert_eq!(client.get_total_batches_processed(), 2);
    assert_eq!(client.get_total_goals_created(), 2);
    assert_eq!(client.get_last_goal_id(), 2);
}

#[test]
fn test_high_value_goal_event() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    // Create high-value goal (>= 100,000 XLM)
    requests.push_back(create_valid_request(
        &env,
        &user,
        "mansion",
        1_000_000_000_000,
    ));

    let result = client.batch_set_savings_goals(&admin, &requests);

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

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);

    // Valid
    requests.push_back(create_valid_request(&env, &user1, "vacation", 100_000_000));

    // Invalid - amount too low
    let mut invalid1 = create_valid_request(&env, &user2, "test", 1000);
    invalid1.target_amount = 1000;
    requests.push_back(invalid1);

    // Valid
    requests.push_back(create_valid_request(&env, &user3, "house", 500_000_000));

    // Invalid - deadline in past
    let mut invalid2 = create_valid_request(&env, &user4, "test", 100_000_000);
    invalid2.deadline = 0;
    requests.push_back(invalid2);

    let result = client.batch_set_savings_goals(&admin, &requests);

    assert_eq!(result.total_requests, 4);
    assert_eq!(result.successful, 2);
    assert_eq!(result.failed, 2);

    // Only successful goals should be stored
    assert_eq!(client.get_total_goals_created(), 2);
}

#[test]
fn test_zero_initial_contribution() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    let mut request = create_valid_request(&env, &user, "vacation", 100_000_000);
    request.initial_contribution = 0; // Zero initial contribution is valid
    requests.push_back(request);

    let result = client.batch_set_savings_goals(&admin, &requests);

    assert_eq!(result.successful, 1);
    assert_eq!(result.failed, 0);

    let goal = client.get_goal(&1).unwrap();
    assert_eq!(goal.current_amount, 0);
}

#[test]
fn test_duplicate_goal_name_same_user() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, &user, "vacation", 100_000_000));
    requests.push_back(create_valid_request(&env, &user, "vacation", 200_000_000)); // Duplicate name

    let result = client.batch_set_savings_goals(&admin, &requests);

    assert_eq!(result.total_requests, 2);
    assert_eq!(result.successful, 1); // First one succeeds
    assert_eq!(result.failed, 1); // Second one fails (duplicate name)

    // Verify first succeeded
    match &result.results.get(0).unwrap() {
        GoalResult::Success(_) => {}
        GoalResult::Failure(_, _) => panic!("Expected first request to succeed"),
    }

    // Verify second failed with duplicate error
    match &result.results.get(1).unwrap() {
        GoalResult::Success(_) => panic!("Expected second request to fail"),
        GoalResult::Failure(_, error_code) => {
            assert_eq!(*error_code, ErrorCode::DUPLICATE_GOAL_NAME);
        }
    }

    assert_eq!(client.get_total_goals_created(), 1);
}

#[test]
fn test_same_goal_name_different_users() {
    let (env, admin, client) = setup_test_contract();
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, &user1, "vacation", 100_000_000));
    requests.push_back(create_valid_request(&env, &user2, "vacation", 200_000_000)); // Same name, different user

    let result = client.batch_set_savings_goals(&admin, &requests);

    assert_eq!(result.total_requests, 2);
    assert_eq!(result.successful, 2); // Both should succeed
    assert_eq!(result.failed, 0);
    assert_eq!(client.get_total_goals_created(), 2);
}

#[test]
fn test_duplicate_goal_name_across_batches() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    // First batch creates "vacation"
    let mut requests1: Vec<SavingsGoalRequest> = Vec::new(&env);
    requests1.push_back(create_valid_request(&env, &user, "vacation", 100_000_000));
    let result1 = client.batch_set_savings_goals(&admin, &requests1);
    assert_eq!(result1.successful, 1);

    // Second batch tries to create "vacation" again for same user
    let mut requests2: Vec<SavingsGoalRequest> = Vec::new(&env);
    requests2.push_back(create_valid_request(&env, &user, "vacation", 200_000_000));
    let result2 = client.batch_set_savings_goals(&admin, &requests2);
    assert_eq!(result2.successful, 0);
    assert_eq!(result2.failed, 1);

    match &result2.results.get(0).unwrap() {
        GoalResult::Failure(_, error_code) => {
            assert_eq!(*error_code, ErrorCode::DUPLICATE_GOAL_NAME);
        }
        GoalResult::Success(_) => panic!("Expected duplicate to fail"),
    }
}

#[test]
fn test_full_initial_contribution() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    let mut request = create_valid_request(&env, &user, "vacation", 100_000_000);
    request.initial_contribution = 100_000_000; // Full amount
    requests.push_back(request);

    let result = client.batch_set_savings_goals(&admin, &requests);

    assert_eq!(result.successful, 1);

    let goal = client.get_goal(&1).unwrap();
    assert_eq!(goal.current_amount, 100_000_000);
    assert_eq!(goal.target_amount, 100_000_000);
}

// ==================== Goal Auto-Closure Tests (#599) ====================

#[test]
fn test_contribute_to_goal_increases_current_amount() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    let mut req = create_valid_request(&env, &user, "house", 100_000_000);
    req.initial_contribution = 0;
    requests.push_back(req);
    client.batch_set_savings_goals(&admin, &requests);

    client.contribute_to_goal(&user, &1, &20_000_000_i128, &idempotency_token(&env, 1));
    let updated = client.get_goal(&1).unwrap();

    assert_eq!(updated.current_amount, 20_000_000);
    assert_eq!(updated.is_active, true);
}

#[test]
fn test_goal_auto_closes_when_target_reached() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    let mut req = create_valid_request(&env, &user, "car", 50_000_000);
    req.initial_contribution = 0;
    requests.push_back(req);
    client.batch_set_savings_goals(&admin, &requests);

    // Contribute exactly the target amount
    client.contribute_to_goal(&user, &1, &50_000_000_i128, &idempotency_token(&env, 1));
    let updated = client.get_goal(&1).unwrap();

    // Goal should be auto-closed
    assert_eq!(updated.is_active, false);
    assert_eq!(updated.current_amount, 50_000_000);

    // get_goal_closed_at should return a value
    let closed_at = client.get_goal_closed_at(&1);
    assert!(closed_at.is_some());
}

#[test]
fn test_goal_auto_closes_on_over_contribution() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    let mut req = create_valid_request(&env, &user, "fund", 50_000_000);
    req.initial_contribution = 0;
    requests.push_back(req);
    client.batch_set_savings_goals(&admin, &requests);

    // Contribute more than target — should be capped and goal auto-closed
    client.contribute_to_goal(&user, &1, &999_999_999_i128, &idempotency_token(&env, 1));
    let updated = client.get_goal(&1).unwrap();

    assert_eq!(updated.is_active, false);
    assert_eq!(updated.current_amount, 50_000_000); // capped at target
}

#[test]
#[should_panic]
fn test_closed_goal_rejects_further_contributions() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    let mut req = create_valid_request(&env, &user, "savings", 50_000_000);
    req.initial_contribution = 0;
    requests.push_back(req);
    client.batch_set_savings_goals(&admin, &requests);

    // Close the goal
    client.contribute_to_goal(&user, &1, &50_000_000_i128, &idempotency_token(&env, 1));

    // This contribution should panic because the goal is closed
    client.contribute_to_goal(&user, &1, &1_000_i128, &idempotency_token(&env, 2));
}

#[test]
#[should_panic]
fn test_contribute_with_wrong_caller_panics() {
    let (env, admin, client) = setup_test_contract();
    let owner = Address::generate(&env);
    let other = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    let mut req = create_valid_request(&env, &owner, "trip", 100_000_000);
    req.initial_contribution = 0;
    requests.push_back(req);
    client.batch_set_savings_goals(&admin, &requests);

    // other is not the goal owner — should panic
    client.contribute_to_goal(&other, &1, &10_000_000_i128, &idempotency_token(&env, 1));
}

#[test]
#[should_panic]
fn test_contribute_zero_amount_panics() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    let mut req = create_valid_request(&env, &user, "fund", 100_000_000);
    req.initial_contribution = 0;
    requests.push_back(req);
    client.batch_set_savings_goals(&admin, &requests);

    client.contribute_to_goal(&user, &1, &0_i128, &idempotency_token(&env, 1));
}

#[test]
fn test_get_goal_closed_at_none_for_open_goal() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, &user, "fund", 100_000_000));
    client.batch_set_savings_goals(&admin, &requests);

    assert!(client.get_goal_closed_at(&1).is_none());
}

#[test]
fn test_full_initial_contribution_auto_closes_goal() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    let mut req = create_valid_request(&env, &user, "vacation", 100_000_000);
    req.initial_contribution = 100_000_000; // Full amount at creation
    requests.push_back(req);

    client.batch_set_savings_goals(&admin, &requests);

    // Goal created with 100% initial contribution should be auto-closed
    let goal = client.get_goal(&1).unwrap();
    assert_eq!(goal.current_amount, 100_000_000);
    assert_eq!(goal.target_amount, 100_000_000);
    assert_eq!(goal.is_active, false);
    assert!(client.get_goal_closed_at(&1).is_some());
}

// ==================== Milestone Achievement Tests ====================

#[test]
fn test_batch_mark_single_milestone() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Create a goal first
    let mut goal_requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    goal_requests.push_back(create_valid_request(&env, &user, "savings", 100_000_000));
    client.batch_set_savings_goals(&admin, &goal_requests);
    // Update goal's current_amount to meet milestone
    let mut goal = client.get_goal(&1).unwrap();
    goal.current_amount = 25_000_000; // 25% of 100_000_000
    client.test_set_goal_current_amount(&1, &25_000_000);

    // Mark a milestone
    let mut milestone_requests: Vec<MilestoneAchievementRequest> = Vec::new(&env);
    milestone_requests.push_back(MilestoneAchievementRequest {
        goal_id: 1,
        user: user.clone(),
        milestone_percentage: 25,
        achieved_at: env.ledger().sequence() as u64,
    });

    let result = client.batch_mark_milestones(&user, &milestone_requests);

    assert_eq!(result.successful, 1);
    assert_eq!(result.failed, 0);
    assert_eq!(result.total_requests, 1);
    assert_eq!(result.metrics.total_percentage_points, 25);
    assert_eq!(result.metrics.avg_percentage, 25);
    assert_eq!(client.get_total_milestones_achieved(), 1);
}

#[test]
fn test_batch_mark_multiple_milestones() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Create a goal
    let mut goal_requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    goal_requests.push_back(create_valid_request(&env, &user, "savings", 100_000_000));
    client.batch_set_savings_goals(&admin, &goal_requests);
    // Update goal's current_amount to meet all milestones
    let mut goal = client.get_goal(&1).unwrap();
    goal.current_amount = 75_000_000; // 75% of 100_000_000
    client.test_set_goal_current_amount(&1, &75_000_000);

    // Mark multiple milestones in one batch
    let mut milestone_requests: Vec<MilestoneAchievementRequest> = Vec::new(&env);
    milestone_requests.push_back(MilestoneAchievementRequest {
        goal_id: 1,
        user: user.clone(),
        milestone_percentage: 25,
        achieved_at: env.ledger().sequence() as u64,
    });
    milestone_requests.push_back(MilestoneAchievementRequest {
        goal_id: 1,
        user: user.clone(),
        milestone_percentage: 50,
        achieved_at: env.ledger().sequence() as u64,
    });
    milestone_requests.push_back(MilestoneAchievementRequest {
        goal_id: 1,
        user: user.clone(),
        milestone_percentage: 75,
        achieved_at: env.ledger().sequence() as u64,
    });

    let result = client.batch_mark_milestones(&user, &milestone_requests);

    assert_eq!(result.successful, 3);
    assert_eq!(result.failed, 0);
    assert_eq!(result.total_requests, 3);
    assert_eq!(result.metrics.total_percentage_points, 150);
    assert_eq!(result.metrics.avg_percentage, 50);

    // Verify all milestones were created
    let goal_milestones = client.get_goal_milestones(&1);
    assert_eq!(goal_milestones.len(), 3);
}

#[test]
fn test_milestone_invalid_percentage_zero() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Create a goal
    let mut goal_requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    goal_requests.push_back(create_valid_request(&env, &user, "savings", 100_000_000));
    client.batch_set_savings_goals(&admin, &goal_requests);

    // Try to mark milestone with 0% (invalid)
    let mut milestone_requests: Vec<MilestoneAchievementRequest> = Vec::new(&env);
    milestone_requests.push_back(MilestoneAchievementRequest {
        goal_id: 1,
        user: user.clone(),
        milestone_percentage: 0,
        achieved_at: env.ledger().sequence() as u64,
    });

    let result = client.batch_mark_milestones(&user, &milestone_requests);

    assert_eq!(result.successful, 0);
    assert_eq!(result.failed, 1);

    match &result.results.get(0).unwrap() {
        MilestoneResult::Failure(_, code) => {
            assert_eq!(code, &ErrorCode::INVALID_MILESTONE_PERCENTAGE);
        }
        _ => panic!("Expected failure"),
    }
}

#[test]
fn test_milestone_invalid_percentage_over_100() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Create a goal
    let mut goal_requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    goal_requests.push_back(create_valid_request(&env, &user, "savings", 100_000_000));
    client.batch_set_savings_goals(&admin, &goal_requests);

    // Try to mark milestone with >100% (invalid)
    let mut milestone_requests: Vec<MilestoneAchievementRequest> = Vec::new(&env);
    milestone_requests.push_back(MilestoneAchievementRequest {
        goal_id: 1,
        user: user.clone(),
        milestone_percentage: 101,
        achieved_at: env.ledger().sequence() as u64,
    });

    let result = client.batch_mark_milestones(&user, &milestone_requests);

    assert_eq!(result.successful, 0);
    assert_eq!(result.failed, 1);

    match &result.results.get(0).unwrap() {
        MilestoneResult::Failure(_, code) => {
            assert_eq!(*code, ErrorCode::INVALID_MILESTONE_PERCENTAGE);
        }
        _ => panic!("Expected failure"),
    }
}

#[test]
fn test_milestone_goal_not_found() {
    let (env, _admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Try to mark milestone for non-existent goal
    let mut milestone_requests: Vec<MilestoneAchievementRequest> = Vec::new(&env);
    milestone_requests.push_back(MilestoneAchievementRequest {
        goal_id: 999,
        user: user.clone(),
        milestone_percentage: 50,
        achieved_at: env.ledger().sequence() as u64,
    });

    let result = client.batch_mark_milestones(&user, &milestone_requests);

    assert_eq!(result.successful, 0);
    assert_eq!(result.failed, 1);

    match &result.results.get(0).unwrap() {
        MilestoneResult::Failure(_, code) => {
            assert_eq!(code, &ErrorCode::GOAL_NOT_FOUND);
        }
        _ => panic!("Expected failure"),
    }
}

#[test]
fn test_milestone_unauthorized_user() {
    let (env, admin, client) = setup_test_contract();
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    // Create a goal for user1
    let mut goal_requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    goal_requests.push_back(create_valid_request(&env, &user1, "savings", 100_000_000));
    client.batch_set_savings_goals(&admin, &goal_requests);

    // Try to mark milestone as user2 (not the goal owner)
    let mut milestone_requests: Vec<MilestoneAchievementRequest> = Vec::new(&env);
    milestone_requests.push_back(MilestoneAchievementRequest {
        goal_id: 1,
        user: user1.clone(),
        milestone_percentage: 50,
        achieved_at: env.ledger().sequence() as u64,
    });

    let result = client.batch_mark_milestones(&user2, &milestone_requests);

    assert_eq!(result.successful, 0);
    assert_eq!(result.failed, 1);

    match &result.results.get(0).unwrap() {
        MilestoneResult::Failure(_, code) => {
            assert_eq!(code, &ErrorCode::UNAUTHORIZED_USER);
        }
        _ => panic!("Expected failure"),
    }
}

#[test]
fn test_milestone_duplicate_percentage() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Create a goal
    let mut goal_requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    goal_requests.push_back(create_valid_request(&env, &user, "savings", 100_000_000));
    client.batch_set_savings_goals(&admin, &goal_requests);

    // Update current amount so the 50% milestone is achievable.
    client.test_set_goal_current_amount(&1, &50_000_000);

    // Mark first milestone
    let mut milestone_requests: Vec<MilestoneAchievementRequest> = Vec::new(&env);
    milestone_requests.push_back(MilestoneAchievementRequest {
        goal_id: 1,
        user: user.clone(),
        milestone_percentage: 50,
        achieved_at: env.ledger().sequence() as u64,
    });
    client.batch_mark_milestones(&user, &milestone_requests);

    // Try to mark the same milestone again
    let mut duplicate_requests: Vec<MilestoneAchievementRequest> = Vec::new(&env);
    duplicate_requests.push_back(MilestoneAchievementRequest {
        goal_id: 1,
        user: user.clone(),
        milestone_percentage: 50,
        achieved_at: env.ledger().sequence() as u64,
    });

    let result = client.batch_mark_milestones(&user, &duplicate_requests);

    assert_eq!(result.successful, 0);
    assert_eq!(result.failed, 1);

    match &result.results.get(0).unwrap() {
        MilestoneResult::Failure(_, code) => {
            assert_eq!(code, &ErrorCode::MILESTONE_ALREADY_ACHIEVED);
        }
        _ => panic!("Expected failure"),
    }
}

#[test]
fn test_milestone_partial_failures() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Create a goal
    let mut goal_requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    goal_requests.push_back(create_valid_request(&env, &user, "savings", 100_000_000));
    client.batch_set_savings_goals(&admin, &goal_requests);
    // Update goal's current_amount to meet valid milestones
    let mut goal = client.get_goal(&1).unwrap();
    goal.current_amount = 75_000_000; // 75% of 100_000_000
    client.test_set_goal_current_amount(&1, &75_000_000);

    // Create a batch with mixed valid and invalid requests
    let mut milestone_requests: Vec<MilestoneAchievementRequest> = Vec::new(&env);

    // Valid
    milestone_requests.push_back(MilestoneAchievementRequest {
        goal_id: 1,
        user: user.clone(),
        milestone_percentage: 25,
        achieved_at: env.ledger().sequence() as u64,
    });

    // Invalid - percentage too high
    milestone_requests.push_back(MilestoneAchievementRequest {
        goal_id: 1,
        user: user.clone(),
        milestone_percentage: 101,
        achieved_at: env.ledger().sequence() as u64,
    });

    // Valid
    milestone_requests.push_back(MilestoneAchievementRequest {
        goal_id: 1,
        user: user.clone(),
        milestone_percentage: 75,
        achieved_at: env.ledger().sequence() as u64,
    });

    // Invalid - goal not found
    milestone_requests.push_back(MilestoneAchievementRequest {
        goal_id: 999,
        user: user.clone(),
        milestone_percentage: 50,
        achieved_at: env.ledger().sequence() as u64,
    });

    let result = client.batch_mark_milestones(&user, &milestone_requests);

    assert_eq!(result.total_requests, 4);
    assert_eq!(result.successful, 2);
    assert_eq!(result.failed, 2);
    assert_eq!(result.metrics.total_percentage_points, 100);
    assert_eq!(result.metrics.avg_percentage, 50);

    // Verify only successful milestones were created
    let goal_milestones = client.get_goal_milestones(&1);
    assert_eq!(goal_milestones.len(), 2);
}

#[test]
fn test_milestone_retrieve_milestone() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Create a goal
    let mut goal_requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    goal_requests.push_back(create_valid_request(&env, &user, "savings", 100_000_000));
    client.batch_set_savings_goals(&admin, &goal_requests);
    // Update goal's current_amount to meet milestone
    let mut goal = client.get_goal(&1).unwrap();
    goal.current_amount = 50_000_000; // 50% of 100_000_000
    client.test_set_goal_current_amount(&1, &50_000_000);

    // Mark a milestone
    let mut milestone_requests: Vec<MilestoneAchievementRequest> = Vec::new(&env);
    milestone_requests.push_back(MilestoneAchievementRequest {
        goal_id: 1,
        user: user.clone(),
        milestone_percentage: 50,
        achieved_at: env.ledger().sequence() as u64,
    });
    client.batch_mark_milestones(&user, &milestone_requests);

    // Retrieve and verify milestone
    let milestone = client.get_milestone(&1).unwrap();
    assert_eq!(milestone.milestone_id, 1);
    assert_eq!(milestone.goal_id, 1);
    assert_eq!(milestone.user, user);
    assert_eq!(milestone.milestone_percentage, 50);
}

#[test]
#[should_panic]
fn test_milestone_empty_batch() {
    let (env, _admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let milestone_requests: Vec<MilestoneAchievementRequest> = Vec::new(&env);

    client.batch_mark_milestones(&user, &milestone_requests);
}

#[test]
#[should_panic]
fn test_milestone_batch_too_large() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Create a goal
    let mut goal_requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    goal_requests.push_back(create_valid_request(&env, &user, "savings", 100_000_000));
    client.batch_set_savings_goals(&admin, &goal_requests);
    // Update goal's current_amount to meet milestone
    let mut goal = client.get_goal(&1).unwrap();
    goal.current_amount = 50_000_000; // 50% of 100_000_000
    client.test_set_goal_current_amount(&1, &50_000_000);

    // Create batch exceeding MAX_BATCH_SIZE
    let mut milestone_requests: Vec<MilestoneAchievementRequest> = Vec::new(&env);
    for i in 0..=100 {
        milestone_requests.push_back(MilestoneAchievementRequest {
            goal_id: 1,
            user: user.clone(),
            milestone_percentage: ((i % 100) + 1) as u32,
            achieved_at: env.ledger().sequence() as u64,
        });
    }

    client.batch_mark_milestones(&user, &milestone_requests);
}

// ==================== Lock Duration & Withdrawal Tests ====================

#[test]
fn test_locked_goal_rejects_withdrawal() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    requests.push_back(SavingsGoalRequest {
        user: user.clone(),
        goal_name: Symbol::new(&env, "locked"),
        target_amount: 100_000_000,
        deadline: env.ledger().sequence() as u64 + 1000,
        initial_contribution: 50_000_000,
        priority: 1,
        lock_duration_seconds: 86_400,
        penalty_bps: 0,
        expiration_seconds: 0,
    });
    client.batch_set_savings_goals(&admin, &requests);

    let goal = client.get_goal(&1).unwrap();
    assert!(goal.unlock_at > 0);
    assert!(client.is_goal_locked(&1));

    let result = client.try_withdraw_from_goal(&user, &1, &10_000_000);
    assert!(result.is_err());
}

#[test]
fn test_unlocked_goal_allows_withdrawal() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    requests.push_back(SavingsGoalRequest {
        user: user.clone(),
        goal_name: Symbol::new(&env, "unlocked"),
        target_amount: 100_000_000,
        deadline: env.ledger().sequence() as u64 + 1000,
        initial_contribution: 50_000_000,
        priority: 1,
        lock_duration_seconds: 0,
        penalty_bps: 0,
        expiration_seconds: 0,
    });
    client.batch_set_savings_goals(&admin, &requests);

    assert!(!client.is_goal_locked(&1));
    let remaining = client.withdraw_from_goal(&user, &1, &10_000_000);
    assert_eq!(remaining, 40_000_000);
}

#[test]
fn test_withdrawal_allowed_after_lock_expires() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    requests.push_back(SavingsGoalRequest {
        user: user.clone(),
        goal_name: Symbol::new(&env, "timed_lock"),
        target_amount: 100_000_000,
        deadline: env.ledger().sequence() as u64 + 1000,
        initial_contribution: 50_000_000,
        priority: 1,
        lock_duration_seconds: 3_600,
        penalty_bps: 0,
        expiration_seconds: 0,
    });
    client.batch_set_savings_goals(&admin, &requests);

    let goal = client.get_goal(&1).unwrap();
    env.ledger().set_timestamp(goal.unlock_at + 1);

    assert!(!client.is_goal_locked(&1));
    let remaining = client.withdraw_from_goal(&user, &1, &10_000_000);
    assert_eq!(remaining, 40_000_000);
}

#[test]
fn test_early_withdrawal_applies_configured_penalty() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    requests.push_back(SavingsGoalRequest {
        user: user.clone(),
        goal_name: Symbol::new(&env, "penalty_goal"),
        target_amount: 100_000_000,
        deadline: env.ledger().sequence() as u64 + 1000,
        initial_contribution: 50_000_000,
        priority: 1,
        lock_duration_seconds: 0,
        penalty_bps: 1_000,
        expiration_seconds: 0,
    });
    client.batch_set_savings_goals(&admin, &requests);

    let remaining = client.withdraw_from_goal(&user, &1, &10_000_000);
    assert_eq!(remaining, 39_000_000);
}

#[test]
fn test_withdrawal_has_no_penalty_when_goal_is_complete() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    requests.push_back(SavingsGoalRequest {
        user: user.clone(),
        goal_name: Symbol::new(&env, "complete_goal"),
        target_amount: 100_000_000,
        deadline: env.ledger().sequence() as u64 + 1000,
        initial_contribution: 100_000_000,
        priority: 1,
        lock_duration_seconds: 0,
        penalty_bps: 1_000,
        expiration_seconds: 0,
    });
    client.batch_set_savings_goals(&admin, &requests);

    let remaining = client.withdraw_from_goal(&user, &1, &10_000_000);
    assert_eq!(remaining, 90_000_000);
}

#[test]
fn test_contribute_emits_milestone_events() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    requests.push_back(SavingsGoalRequest {
        user: user.clone(),
        goal_name: Symbol::new(&env, "milestones"),
        target_amount: 100_000_000,
        deadline: env.ledger().sequence() as u64 + 1000,
        initial_contribution: 0,
        priority: 1,
        lock_duration_seconds: 0,
        penalty_bps: 0,
        expiration_seconds: 0,
    });
    client.batch_set_savings_goals(&admin, &requests);

    client.contribute_to_goal(&user, &1, &25_000_000, &idempotency_token(&env, 1));
    client.contribute_to_goal(&user, &1, &25_000_000, &idempotency_token(&env, 2));
    client.contribute_to_goal(&user, &1, &25_000_000, &idempotency_token(&env, 3));
    client.contribute_to_goal(&user, &1, &25_000_000, &idempotency_token(&env, 4));

    let triggered = client.get_triggered_milestone_percents(&1);
    assert_eq!(triggered.len(), 4);
    assert!(triggered.contains(&25));
    assert!(triggered.contains(&50));
    assert!(triggered.contains(&75));
    assert!(triggered.contains(&100));
}

// ==================== Snapshot Tests ====================

#[test]
fn test_record_and_get_goal_snapshots() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    requests.push_back(SavingsGoalRequest {
        user: user.clone(),
        goal_name: Symbol::new(&env, "snapshot_test"),
        target_amount: 100_000_000,
        deadline: env.ledger().sequence() as u64 + 1000,
        initial_contribution: 10_000_000,
        priority: 1,
        lock_duration_seconds: 0,
        penalty_bps: 0,
        expiration_seconds: 0,
    });
    client.batch_set_savings_goals(&admin, &requests);

    // Record first snapshot manually
    client.record_goal_snapshot(&user, &1);

    // Contribute
    client.contribute_to_goal(&user, &1, &20_000_000, &idempotency_token(&env, 1));

    // Record second snapshot
    env.ledger().set_timestamp(env.ledger().timestamp() + 100);
    client.record_goal_snapshot(&user, &1);

    let snapshots = client.get_goal_snapshots(&1);
    assert_eq!(snapshots.len(), 2);

    let snap1 = snapshots.get(0).unwrap();
    assert_eq!(snap1.goal_id, 1);
    assert_eq!(snap1.amount, 10_000_000);

    let snap2 = snapshots.get(1).unwrap();
    assert_eq!(snap2.goal_id, 1);
    assert_eq!(snap2.amount, 30_000_000);
}

#[test]
fn test_clone_savings_goal() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    requests.push_back(SavingsGoalRequest {
        user: user.clone(),
        goal_name: Symbol::new(&env, "original"),
        target_amount: 100_000_000,
        deadline: env.ledger().sequence() as u64 + 1000,
        initial_contribution: 50_000_000,
        priority: 1,
        lock_duration_seconds: 3600,
        penalty_bps: 0,
        expiration_seconds: 0,
    });
    client.batch_set_savings_goals(&admin, &requests);

    let cloned_name = Symbol::new(&env, "cloned");
    let cloned_id = client.clone_savings_goal(&user, &1, &cloned_name);

    assert_eq!(cloned_id, 2);

    let cloned_goal = client.get_goal(&cloned_id).unwrap();
    assert_eq!(cloned_goal.goal_name, cloned_name);
    assert_eq!(cloned_goal.target_amount, 100_000_000);
    assert_eq!(cloned_goal.current_amount, 0); // Balance reset
    assert_eq!(cloned_goal.user, user);
    assert_eq!(cloned_goal.is_complete, false);
    assert!(cloned_goal.unlock_at > cloned_goal.created_at); // Lock inherited
}

#[test]
#[should_panic]
fn test_record_goal_snapshot_unauthorized() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);
    let unauthorized = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    requests.push_back(SavingsGoalRequest {
        user: user.clone(),
        goal_name: Symbol::new(&env, "snapshot_test_auth"),
        target_amount: 100_000_000,
        deadline: env.ledger().sequence() as u64 + 1000,
        initial_contribution: 10_000_000,
        priority: 1,
        lock_duration_seconds: 0,
        penalty_bps: 0,
        expiration_seconds: 0,
    });
    client.batch_set_savings_goals(&admin, &requests);

    client.record_goal_snapshot(&unauthorized, &1);
}

#[test]
#[should_panic]
fn test_clone_savings_goal_unauthorized() {
    let (env, admin, client) = setup_test_contract();
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    requests.push_back(create_valid_request(&env, &user1, "original", 100_000_000));
    client.batch_set_savings_goals(&admin, &requests);

    let cloned_name = Symbol::new(&env, "cloned");
    client.clone_savings_goal(&user2, &1, &cloned_name); // user2 tries to clone user1's goal
}

#[test]
fn test_reverse_contribution_within_window() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Create a goal
    let mut goal_requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    goal_requests.push_back(create_valid_request(
        &env,
        &user,
        "reverse_goal",
        100_000_000,
    ));
    client.batch_set_savings_goals(&admin, &goal_requests);

    let goal_id: u64 = 1;
    let contribution_amount: i128 = 5_000_000;

    // Contribute and capture the contrib_id
    let contrib_id = client.contribute_to_goal(
        &user,
        &goal_id,
        &contribution_amount,
        &idempotency_token(&env, 1),
    );

    // Reverse immediately (still within the 24-hour window)
    let remaining = client.reverse_contribution(&user, &goal_id, &contrib_id);

    // Balance should be back to the initial contribution (10% = 10_000_000) only
    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(remaining, goal.current_amount);
    assert_eq!(goal.current_amount, 100_000_000 / 10); // initial_contribution only
}

#[test]
#[should_panic]
fn test_reverse_contribution_after_window_rejected() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Create a goal
    let mut goal_requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    goal_requests.push_back(create_valid_request(
        &env,
        &user,
        "expired_rev",
        100_000_000,
    ));
    client.batch_set_savings_goals(&admin, &goal_requests);

    let goal_id: u64 = 1;
    let contrib_id =
        client.contribute_to_goal(&user, &goal_id, &5_000_000i128, &idempotency_token(&env, 1));

    // Advance time past the 24-hour reversal window
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + 86_401);

    // Should panic with ReversalExpired
    client.reverse_contribution(&user, &goal_id, &contrib_id);
}

#[test]
fn test_duplicate_contribution_idempotency_rejected_and_original_preserved() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    let mut request = create_valid_request(&env, &user, "idempotent_goal", 100_000_000);
    request.initial_contribution = 0;
    requests.push_back(request);
    client.batch_set_savings_goals(&admin, &requests);

    let token = idempotency_token(&env, 42);
    let contrib_id = client.contribute_to_goal(&user, &1, &15_000_000, &token);
    let duplicate = client.try_contribute_to_goal(&user, &1, &15_000_000, &token);

    assert!(duplicate.is_err());

    let goal = client.get_goal(&1).unwrap();
    assert_eq!(goal.current_amount, 15_000_000);

    let record = client.get_contribution_record(&1, &contrib_id).unwrap();
    assert_eq!(record.amount, 15_000_000);
    assert_eq!(record.idempotency_token, token);
    assert_eq!(record.reversed, false);
}

#[test]
fn test_deadline_alerts_emit_once_per_threshold_before_deadline() {
    let (env, admin, client) = setup_test_contract();
    env.ledger().with_mut(|li| {
        li.sequence_number = 500;
    });

    let user = Address::generate(&env);
    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    let mut request = create_valid_request(&env, &user, "deadline_alerts", 100_000_000);
    request.initial_contribution = 0;
    request.deadline = 700;
    requests.push_back(request);
    client.batch_set_savings_goals(&admin, &requests);

    let mut thresholds: Vec<u64> = Vec::new(&env);
    thresholds.push_back(200);
    thresholds.push_back(50);
    client.set_goal_alert_thresholds(&user, &1, &thresholds);

    env.ledger().with_mut(|li| {
        li.sequence_number = 550;
    });
    assert_eq!(client.process_goal_alerts(&1), 1);
    assert_eq!(client.process_goal_alerts(&1), 0);

    env.ledger().with_mut(|li| {
        li.sequence_number = 660;
    });
    assert_eq!(client.process_goal_alerts(&1), 1);

    let emitted = client.get_goal_alerts_emitted(&1);
    assert_eq!(emitted.len(), 2);
    assert!(emitted.contains(&200));
    assert!(emitted.contains(&50));

    env.ledger().with_mut(|li| {
        li.sequence_number = 700;
    });
    assert_eq!(client.process_goal_alerts(&1), 0);
}

// ==================== Completion Detection Tests ====================

#[test]
fn test_goal_marked_complete_at_exact_target_threshold() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Create a goal with zero initial contribution
    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    let mut request = create_valid_request(&env, &user, "target_test", 100_000_000);
    request.initial_contribution = 0;
    requests.push_back(request);

    client.batch_set_savings_goals(&admin, &requests);

    // Verify goal starts as not complete
    let initial_progress = client.get_goal_progress(&1).unwrap();
    assert_eq!(initial_progress.is_complete, false);
    assert_eq!(initial_progress.progress_percentage, 0);

    let initial_goal = client.get_goal(&1).unwrap();
    assert_eq!(initial_goal.is_complete, false);
    assert_eq!(initial_goal.is_active, true);

    // Contribute exactly to the target amount
    client.contribute_to_goal(&user, &1, &100_000_000, &idempotency_token(&env, 1));

    // Verify goal is marked as complete at exact threshold
    let final_progress = client.get_goal_progress(&1).unwrap();
    assert_eq!(final_progress.is_complete, true);
    assert_eq!(final_progress.progress_percentage, 100);
    assert_eq!(final_progress.current_amount, 100_000_000);
    assert_eq!(final_progress.target_amount, 100_000_000);

    let final_goal = client.get_goal(&1).unwrap();
    assert_eq!(final_goal.is_complete, true);
    assert_eq!(final_goal.current_amount, 100_000_000);
    assert_eq!(final_goal.target_amount, 100_000_000);
}

#[test]
fn test_goal_not_marked_complete_below_target() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Create a goal with zero initial contribution
    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    let mut request = create_valid_request(&env, &user, "below_target", 100_000_000);
    request.initial_contribution = 0;
    requests.push_back(request);

    client.batch_set_savings_goals(&admin, &requests);

    // Test various amounts below target
    let test_amounts = [1, 50_000_000, 99_999_999]; // 1 stroop, 50%, just under target

    for amount in test_amounts.iter() {
        // Reset goal for each test
        client.test_set_goal_current_amount(&1, &0);

        // Contribute amount below target
        client.contribute_to_goal(
            &user,
            &1,
            amount,
            &idempotency_token(&env, (*amount % 251) as u8),
        );

        // Verify goal is not marked as complete
        let progress = client.get_goal_progress(&1).unwrap();
        assert_eq!(progress.is_complete, false);
        assert!(progress.progress_percentage < 100);
        assert_eq!(progress.current_amount, *amount);
        assert_eq!(progress.target_amount, 100_000_000);

        let goal = client.get_goal(&1).unwrap();
        assert_eq!(goal.is_complete, false);
        assert_eq!(goal.current_amount, *amount);
    }
}

#[test]
fn test_goal_marked_complete_when_contribution_exceeds_target() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Create a goal with zero initial contribution
    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    let mut request = create_valid_request(&env, &user, "exceed_target", 100_000_000);
    request.initial_contribution = 0;
    requests.push_back(request);

    client.batch_set_savings_goals(&admin, &requests);

    // Contribute more than the target amount
    client.contribute_to_goal(&user, &1, &150_000_000, &idempotency_token(&env, 1));

    // Verify goal is marked as complete and capped at target
    let progress = client.get_goal_progress(&1).unwrap();
    assert_eq!(progress.is_complete, true);
    assert_eq!(progress.progress_percentage, 100); // Capped at 100%
    assert_eq!(progress.current_amount, 100_000_000); // Capped at target
    assert_eq!(progress.target_amount, 100_000_000);

    let goal = client.get_goal(&1).unwrap();
    assert_eq!(goal.is_complete, true);
    assert_eq!(goal.current_amount, 100_000_000); // Capped at target
}

#[test]
fn test_goal_completion_with_initial_contribution() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Test case 1: Initial contribution reaches exact target
    let mut requests1: Vec<SavingsGoalRequest> = Vec::new(&env);
    let mut request1 = create_valid_request(&env, &user, "initial_complete", 100_000_000);
    request1.initial_contribution = 100_000_000; // Exactly target amount
    requests1.push_back(request1);

    client.batch_set_savings_goals(&admin, &requests1);

    let progress1 = client.get_goal_progress(&1).unwrap();
    assert_eq!(progress1.is_complete, true);
    assert_eq!(progress1.progress_percentage, 100);

    // Test case 2: Initial contribution below target
    let mut requests2: Vec<SavingsGoalRequest> = Vec::new(&env);
    let mut request2 = create_valid_request(&env, &user, "initial_partial", 100_000_000);
    request2.initial_contribution = 50_000_000; // 50% of target
    requests2.push_back(request2);

    client.batch_set_savings_goals(&admin, &requests2);

    let progress2 = client.get_goal_progress(&2).unwrap();
    assert_eq!(progress2.is_complete, false);
    assert_eq!(progress2.progress_percentage, 50);

    // Now contribute the remaining amount to reach target
    client.contribute_to_goal(&user, &2, &50_000_000, &idempotency_token(&env, 1));

    let final_progress = client.get_goal_progress(&2).unwrap();
    assert_eq!(final_progress.is_complete, true);
    assert_eq!(final_progress.progress_percentage, 100);
}

#[test]
fn test_incremental_contributions_toward_completion() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Create a goal with zero initial contribution
    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    let mut request = create_valid_request(&env, &user, "incremental", 100_000_000);
    request.initial_contribution = 0;
    requests.push_back(request);

    client.batch_set_savings_goals(&admin, &requests);

    // Make incremental contributions
    let contributions = [25_000_000, 25_000_000, 25_000_000, 25_000_000];
    let expected_totals = [25_000_000, 50_000_000, 75_000_000, 100_000_000];
    let expected_percentages = [25, 50, 75, 100];
    let expected_completion = [false, false, false, true];

    for (i, &contrib) in contributions.iter().enumerate() {
        client.contribute_to_goal(&user, &1, &contrib, &idempotency_token(&env, (i as u8) + 1));

        let progress = client.get_goal_progress(&1).unwrap();
        assert_eq!(progress.current_amount, expected_totals[i]);
        assert_eq!(progress.progress_percentage, expected_percentages[i]);
        assert_eq!(progress.is_complete, expected_completion[i]);

        let goal = client.get_goal(&1).unwrap();
        assert_eq!(goal.is_complete, expected_completion[i]);
    }
}

#[test]
fn test_completion_status_persists_across_queries() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Create and complete a goal
    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);
    let mut request = create_valid_request(&env, &user, "persistence", 100_000_000);
    request.initial_contribution = 100_000_000; // Complete immediately
    requests.push_back(request);

    client.batch_set_savings_goals(&admin, &requests);

    // Verify completion status persists across multiple queries
    for _ in 0..5 {
        let progress = client.get_goal_progress(&1).unwrap();
        assert_eq!(progress.is_complete, true);
        assert_eq!(progress.progress_percentage, 100);

        let goal = client.get_goal(&1).unwrap();
        assert_eq!(goal.is_complete, true);
    }
}

#[test]
fn test_progress_query_returns_accurate_completion_data() {
    let (env, admin, client) = setup_test_contract();
    let user = Address::generate(&env);

    // Test multiple goals with different completion states
    let mut requests: Vec<SavingsGoalRequest> = Vec::new(&env);

    // Goal 1: 0% complete
    let mut req1 = create_valid_request(&env, &user, "zero_percent", 100_000_000);
    req1.initial_contribution = 0;
    requests.push_back(req1);

    // Goal 2: 75% complete
    let mut req2 = create_valid_request(&env, &user, "seventy_five", 100_000_000);
    req2.initial_contribution = 75_000_000;
    requests.push_back(req2);

    // Goal 3: 100% complete
    let mut req3 = create_valid_request(&env, &user, "complete", 100_000_000);
    req3.initial_contribution = 100_000_000;
    requests.push_back(req3);

    client.batch_set_savings_goals(&admin, &requests);

    // Verify each goal's progress query returns accurate completion data
    let progress1 = client.get_goal_progress(&1).unwrap();
    assert_eq!(progress1.goal_id, 1);
    assert_eq!(progress1.current_amount, 0);
    assert_eq!(progress1.target_amount, 100_000_000);
    assert_eq!(progress1.progress_percentage, 0);
    assert_eq!(progress1.is_complete, false);

    let progress2 = client.get_goal_progress(&2).unwrap();
    assert_eq!(progress2.goal_id, 2);
    assert_eq!(progress2.current_amount, 75_000_000);
    assert_eq!(progress2.target_amount, 100_000_000);
    assert_eq!(progress2.progress_percentage, 75);
    assert_eq!(progress2.is_complete, false);

    let progress3 = client.get_goal_progress(&3).unwrap();
    assert_eq!(progress3.goal_id, 3);
    assert_eq!(progress3.current_amount, 100_000_000);
    assert_eq!(progress3.target_amount, 100_000_000);
    assert_eq!(progress3.progress_percentage, 100);
    assert_eq!(progress3.is_complete, true);
}

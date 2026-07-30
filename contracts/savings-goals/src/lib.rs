//! # Savings Goals Contract
//!
//! A Soroban smart contract for managing batch savings goal creation
//! and batch milestone achievement tracking for multiple users simultaneously.
//!
//! ## Features
//!
//! - **Batch Processing**: Efficiently create savings goals for multiple users in a single call
//! - **Batch Milestones**: Mark milestones achieved for multiple goals in a single call
//! - **Comprehensive Validation**: Validates goal amounts, deadlines, and milestone percentages
//! - **Event Emission**: Emits events for goal creation, milestone achievements, and batch processing
//! - **Error Handling**: Gracefully handles invalid inputs with detailed error codes
//! - **Optimized Storage**: Minimizes storage writes by batching operations
//! - **Partial Failure Support**: Batch operations continue even if some individual operations fail
//!
//! ## Optimization Strategies
//!
//! - Single-pass processing for O(n) complexity
//! - Minimized storage operations (batch writes at the end)
//! - Efficient data structures
//! - Batched event emissions

#![no_std]

mod types;
mod validation;

use penalty::PenaltyContractClient;
use soroban_sdk::{
    contract, contractimpl, panic_with_error, symbol_short, Address, Bytes, Env, Symbol, Vec,
};

pub use crate::types::{
    BatchGoalMetrics, BatchGoalResult, BatchMilestoneMetrics, BatchMilestoneResult,
    ContributionRecord, DataKey, ErrorCode, GoalCertificate, GoalEvents, GoalResult, GoalSnapshot,
    MilestoneAchievement, MilestoneAchievementRequest, MilestoneResult, SavingsGoal,
    SavingsGoalProgress, SavingsGoalRequest, MAX_BATCH_SIZE, REVERSAL_PERIOD_SECS,
};
use crate::validation::{validate_goal_name_unique, validate_goal_request};

const PERSISTENT_TTL_BUMP: u32 = 12_614_400;

/// Error codes for the savings goals contract.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum SavingsGoalError {
    /// Contract not initialized
    NotInitialized = 1,
    /// Caller is not authorized
    Unauthorized = 2,
    /// Invalid batch data
    InvalidBatch = 3,
    /// Batch is empty
    EmptyBatch = 4,
    /// Batch exceeds maximum size
    BatchTooLarge = 5,
    /// Goal is closed (target met) and no longer accepts contributions
    GoalClosed = 6,
    /// Insufficient balance for withdrawal
    InsufficientBalance = 7,
    /// Goal is not active
    GoalNotActive = 8,
    /// Invalid goal name
    InvalidGoalName = 9,
    /// Invalid contribution or withdrawal amount
    InvalidAmount = 10,
    /// Goal not found
    GoalNotFound = 11,
    /// Goal is locked against withdrawals
    GoalLocked = 12,
    /// Goal has expired
    GoalExpired = 13,
    /// One or more prerequisite goals are not complete
    DependencyNotMet = 14,
    /// Reversal window has elapsed
    ReversalExpired = 15,
    /// No contribution found with the given ID
    ContributionNotFound = 16,
    /// Deadline alert threshold configuration is invalid
    InvalidAlertThreshold = 17,
    /// Duplicate idempotency token for a contribution retry
    DuplicateContributionRequest = 18,
}

impl From<SavingsGoalError> for soroban_sdk::Error {
    fn from(e: SavingsGoalError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

#[contract]
pub struct SavingsGoalsContract;

#[contractimpl]
impl SavingsGoalsContract {
    /// Batch mark milestones for multiple goals and emit milestone events.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `caller` - The address calling this function (must be goal owner)
    /// * `requests` - Vector of milestone achievement requests
    ///
    /// # Returns
    /// * `BatchMilestoneResult` - Result containing milestone results and metrics
    pub fn batch_mark_milestones(
        env: Env,
        caller: Address,
        requests: Vec<MilestoneAchievementRequest>,
    ) -> BatchMilestoneResult {
        caller.require_auth();
        let mut results: Vec<MilestoneResult> = Vec::new(&env);
        let mut successful: u32 = 0;
        let mut failed: u32 = 0;
        let batch_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::LastBatchId)
            .unwrap_or(0)
            + 1;
        let mut total_percentage_points: u32 = 0;
        let processed_at = env.ledger().sequence() as u64;
        // Validate batch size
        let request_count = requests.len();
        if request_count == 0 {
            panic_with_error!(&env, SavingsGoalError::EmptyBatch);
        }
        if request_count > MAX_BATCH_SIZE {
            panic_with_error!(&env, SavingsGoalError::BatchTooLarge);
        }
        let mut last_milestone_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::LastMilestoneId)
            .unwrap_or(0);
        for req in requests.iter() {
            let goal: Option<SavingsGoal> =
                env.storage().persistent().get(&DataKey::Goal(req.goal_id));
            if let Some(goal) = goal {
                if goal.user != caller {
                    results.push_back(MilestoneResult::Failure(
                        req.goal_id,
                        ErrorCode::UNAUTHORIZED_USER,
                    ));
                    failed += 1;
                    continue;
                }
                let valid_percents = [25u32, 50, 75, 100];
                if !valid_percents.contains(&req.milestone_percentage) {
                    results.push_back(MilestoneResult::Failure(
                        req.goal_id,
                        ErrorCode::INVALID_MILESTONE_PERCENTAGE,
                    ));
                    failed += 1;
                    continue;
                }
                let mut triggered: Vec<u32> = env
                    .storage()
                    .persistent()
                    .get(&DataKey::GoalMilestonesPercent(req.goal_id))
                    .unwrap_or(Vec::new(&env));
                if triggered.contains(&req.milestone_percentage) {
                    results.push_back(MilestoneResult::Failure(
                        req.goal_id,
                        ErrorCode::MILESTONE_ALREADY_ACHIEVED,
                    ));
                    failed += 1;
                    continue;
                }
                let progress = if goal.target_amount > 0 {
                    (goal.current_amount * 100 / goal.target_amount) as u32
                } else {
                    0
                };
                if progress < req.milestone_percentage {
                    results.push_back(MilestoneResult::Failure(
                        req.goal_id,
                        ErrorCode::MILESTONE_NOT_YET_ACHIEVED,
                    ));
                    failed += 1;
                    continue;
                }
                triggered.push_back(req.milestone_percentage);
                env.storage()
                    .persistent()
                    .set(&DataKey::GoalMilestonesPercent(req.goal_id), &triggered);
                GoalEvents::milestone_achieved_percent(&env, req.goal_id, req.milestone_percentage);
                // Store MilestoneAchievement and update milestone IDs
                last_milestone_id += 1;
                let achievement = MilestoneAchievement {
                    milestone_id: last_milestone_id,
                    goal_id: req.goal_id,
                    user: caller.clone(),
                    milestone_percentage: req.milestone_percentage,
                    goal_amount_at_achievement: goal.current_amount,
                    achieved_at: req.achieved_at,
                };
                env.storage()
                    .persistent()
                    .set(&DataKey::Milestone(last_milestone_id), &achievement);
                // Update goal's milestone ID list
                let mut milestone_ids: Vec<u64> = env
                    .storage()
                    .persistent()
                    .get(&DataKey::GoalMilestones(req.goal_id))
                    .unwrap_or(Vec::new(&env));
                milestone_ids.push_back(last_milestone_id);
                env.storage()
                    .persistent()
                    .set(&DataKey::GoalMilestones(req.goal_id), &milestone_ids);
                // Update last milestone ID and total milestones achieved
                env.storage()
                    .instance()
                    .set(&DataKey::LastMilestoneId, &last_milestone_id);
                let total_achieved = env
                    .storage()
                    .instance()
                    .get(&DataKey::TotalMilestonesAchieved)
                    .unwrap_or(0u64)
                    + 1;
                env.storage()
                    .instance()
                    .set(&DataKey::TotalMilestonesAchieved, &total_achieved);
                results.push_back(MilestoneResult::Success(achievement));
                total_percentage_points += req.milestone_percentage;
                successful += 1;
            } else {
                results.push_back(MilestoneResult::Failure(
                    req.goal_id,
                    ErrorCode::GOAL_NOT_FOUND,
                ));
                failed += 1;
            }
        }
        let avg_percentage = if successful > 0 {
            total_percentage_points / successful
        } else {
            0
        };
        let metrics = BatchMilestoneMetrics {
            total_requests: requests.len(),
            successful_milestones: successful,
            failed_milestones: failed,
            total_percentage_points,
            avg_percentage,
            processed_at,
        };
        BatchMilestoneResult {
            batch_id,
            total_requests: requests.len(),
            successful,
            failed,
            results,
            metrics,
        }
    }
    /// Initializes the contract with an admin address.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - The admin address that can manage the contract
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Contract already initialized");
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::LastBatchId, &0u64);
        env.storage().instance().set(&DataKey::LastGoalId, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::TotalGoalsCreated, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::TotalBatchesProcessed, &0u64);
        env.storage().instance().set(
            &DataKey::DefaultDeadlineAlertThresholds,
            &Self::default_deadline_alert_thresholds(&env),
        );
    }

    /// Creates savings goals for multiple users in a batch.
    ///
    /// This is the main entry point for batch goal creation. It validates all requests,
    /// creates goals, emits events, and updates storage efficiently.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `caller` - The address calling this function (must be admin)
    /// * `requests` - Vector of savings goal requests
    ///
    /// # Returns
    /// * `BatchGoalResult` - Result containing created goals and metrics
    ///
    /// # Events Emitted
    /// * `batch_started` - When processing begins
    /// * `goal_created` - For each successful goal creation
    /// * `goal_creation_failed` - For each failed goal creation
    /// * `high_value_goal` - For goals with high target amounts
    /// * `batch_completed` - When processing completes
    ///
    /// # Errors
    /// * `EmptyBatch` - If no requests provided
    /// * `BatchTooLarge` - If batch exceeds maximum size
    /// * `Unauthorized` - If caller is not admin
    pub fn batch_set_savings_goals(
        env: Env,
        caller: Address,
        requests: Vec<SavingsGoalRequest>,
    ) -> BatchGoalResult {
        // Verify authorization
        caller.require_auth();
        Self::require_admin(&env, &caller);

        // Validate batch size
        let request_count = requests.len();
        if request_count == 0 {
            panic_with_error!(&env, SavingsGoalError::EmptyBatch);
        }
        if request_count > MAX_BATCH_SIZE {
            panic_with_error!(&env, SavingsGoalError::BatchTooLarge);
        }

        // Get batch ID and increment
        let batch_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::LastBatchId)
            .unwrap_or(0)
            + 1;

        // Emit batch started event
        GoalEvents::batch_started(&env, batch_id, request_count);

        // Get current ledger timestamp
        let current_ledger = env.ledger().sequence() as u64;

        // Initialize result tracking
        let mut results: Vec<GoalResult> = Vec::new(&env);
        let mut successful_count: u32 = 0;
        let mut failed_count: u32 = 0;
        let mut total_target_amount: i128 = 0;
        let mut total_initial_contributions: i128 = 0;
        let mut goal_id_counter: u64 = env
            .storage()
            .instance()
            .get(&DataKey::LastGoalId)
            .unwrap_or(0);

        // Process each request
        for request in requests.iter() {
            // Validate the request
            match validate_goal_request(&env, &request) {
                Ok(()) => {
                    // Check for duplicate goal name for this user
                    if let Err(error_code) =
                        validate_goal_name_unique(&env, &request.user, &request.goal_name)
                    {
                        failed_count += 1;
                        GoalEvents::goal_creation_failed(&env, batch_id, &request.user, error_code);
                        results.push_back(GoalResult::Failure(request.user.clone(), error_code));
                        continue;
                    }

                    // Validation succeeded - create the goal
                    goal_id_counter += 1;

                    let created_at = env.ledger().timestamp();
                    let unlock_at = if request.lock_duration_seconds > 0 {
                        created_at.saturating_add(request.lock_duration_seconds)
                    } else {
                        0
                    };
                    let expires_at = if request.expiration_seconds > 0 {
                        created_at.saturating_add(request.expiration_seconds)
                    } else {
                        0
                    };
                    let mut goal = SavingsGoal {
                        goal_id: goal_id_counter,
                        user: request.user.clone(),
                        goal_name: request.goal_name.clone(),
                        target_amount: request.target_amount,
                        current_amount: request.initial_contribution,
                        deadline: request.deadline,
                        created_at,
                        is_active: true,
                        is_complete: false,
                        priority: request.priority,
                        unlock_at,
                        expires_at,
                        penalty_bps: request.penalty_bps,
                    };

                    // Accumulate metrics
                    total_target_amount = total_target_amount
                        .checked_add(request.target_amount)
                        .unwrap_or(i128::MAX);
                    total_initial_contributions = total_initial_contributions
                        .checked_add(request.initial_contribution)
                        .unwrap_or(i128::MAX);
                    successful_count += 1;

                    // Store the goal (optimized - one write per goal)
                    env.storage()
                        .persistent()
                        .set(&DataKey::Goal(goal_id_counter), &goal);
                    // Store name-to-id mapping for duplicate detection
                    env.storage().persistent().set(
                        &DataKey::GoalByName(request.user.clone(), request.goal_name.clone()),
                        &goal_id_counter,
                    );
                    // Emit milestone events for initial contribution
                    Self::check_and_emit_milestones(&env, goal_id_counter);

                    // Update local goal variable's is_complete if it reached target
                    if request.initial_contribution >= request.target_amount {
                        goal.is_complete = true;
                    }

                    // Update user's goal list
                    let mut user_goals: Vec<u64> = env
                        .storage()
                        .persistent()
                        .get(&DataKey::UserGoals(request.user.clone()))
                        .unwrap_or(Vec::new(&env));
                    user_goals.push_back(goal_id_counter);
                    env.storage()
                        .persistent()
                        .set(&DataKey::UserGoals(request.user.clone()), &user_goals);

                    // Emit success event
                    GoalEvents::goal_created(&env, batch_id, &goal);

                    // Emit high-value goal event if applicable (>= 100,000 XLM)
                    if request.target_amount >= 1_000_000_000_000 {
                        GoalEvents::high_value_goal(
                            &env,
                            batch_id,
                            goal_id_counter,
                            request.target_amount,
                        );
                    }

                    results.push_back(GoalResult::Success(goal));
                }
                Err(error_code) => {
                    // Validation failed - record failure
                    failed_count += 1;

                    // Emit failure event
                    GoalEvents::goal_creation_failed(&env, batch_id, &request.user, error_code);

                    results.push_back(GoalResult::Failure(request.user.clone(), error_code));
                }
            }
        }

        // Calculate average goal amount
        let avg_goal_amount = if successful_count > 0 {
            total_target_amount / successful_count as i128
        } else {
            0
        };

        // Create metrics
        let metrics = BatchGoalMetrics {
            total_requests: request_count,
            successful_goals: successful_count,
            failed_goals: failed_count,
            total_target_amount,
            total_initial_contributions,
            avg_goal_amount,
            processed_at: current_ledger,
        };

        // Update storage (batched at the end for efficiency)
        let total_goals: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalGoalsCreated)
            .unwrap_or(0);
        let total_batches: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalBatchesProcessed)
            .unwrap_or(0);

        env.storage()
            .instance()
            .set(&DataKey::LastBatchId, &batch_id);
        env.storage()
            .instance()
            .set(&DataKey::LastGoalId, &goal_id_counter);
        env.storage().instance().set(
            &DataKey::TotalGoalsCreated,
            &(total_goals + successful_count as u64),
        );
        env.storage()
            .instance()
            .set(&DataKey::TotalBatchesProcessed, &(total_batches + 1));

        // Emit batch completed event
        GoalEvents::batch_completed(
            &env,
            batch_id,
            successful_count,
            failed_count,
            total_target_amount,
        );

        BatchGoalResult {
            batch_id,
            total_requests: request_count,
            successful: successful_count,
            failed: failed_count,
            results,
            metrics,
        }
    }

    /// Contributes funds to a savings goal and emits milestone events at 25/50/75/100%.
    /// Returns the contribution ID, which can be used to reverse the contribution within the reversal window.
    pub fn contribute_to_goal(
        env: Env,
        caller: Address,
        goal_id: u64,
        amount: i128,
        idempotency_token: Bytes,
    ) -> u64 {
        caller.require_auth();

        if amount <= 0 {
            panic_with_error!(&env, SavingsGoalError::InvalidAmount);
        }
        if idempotency_token.is_empty() {
            panic_with_error!(&env, SavingsGoalError::DuplicateContributionRequest);
        }

        let mut goal: SavingsGoal = env
            .storage()
            .persistent()
            .get(&DataKey::Goal(goal_id))
            .unwrap_or_else(|| panic_with_error!(&env, SavingsGoalError::GoalNotFound));

        if goal.user != caller {
            panic_with_error!(&env, SavingsGoalError::Unauthorized);
        }

        let token_key =
            DataKey::ContributionIdempotency(caller.clone(), goal_id, idempotency_token.clone());
        if env.storage().persistent().has(&token_key) {
            panic_with_error!(&env, SavingsGoalError::DuplicateContributionRequest);
        }

        // Check if goal has expired
        let current_time = env.ledger().timestamp();
        if goal.expires_at > 0 && current_time >= goal.expires_at {
            // Mark goal as inactive
            goal.is_active = false;
            env.storage()
                .persistent()
                .set(&DataKey::Goal(goal_id), &goal);
            panic_with_error!(&env, SavingsGoalError::GoalExpired);
        }

        if !goal.is_active {
            panic_with_error!(&env, SavingsGoalError::GoalNotActive);
        }

        // Enforce prerequisite goals: cannot contribute until all prerequisites complete
        let prereqs: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::GoalPrereqs(goal_id))
            .unwrap_or(Vec::new(&env));
        for pid in prereqs.iter() {
            let pgoal_opt: Option<SavingsGoal> =
                env.storage().persistent().get(&DataKey::Goal(pid));
            if let Some(pgoal) = pgoal_opt {
                if !pgoal.is_complete {
                    panic_with_error!(&env, SavingsGoalError::DependencyNotMet);
                }
            } else {
                panic_with_error!(&env, SavingsGoalError::GoalNotFound);
            }
        }

        // Apply contribution, capped at target to avoid overflow accumulation
        let new_amount = goal.current_amount.checked_add(amount).unwrap_or(i128::MAX);
        let actual_contribution = if new_amount > goal.target_amount {
            goal.target_amount.saturating_sub(goal.current_amount)
        } else {
            amount
        };
        goal.current_amount = if new_amount > goal.target_amount {
            goal.target_amount
        } else {
            new_amount
        };
        goal.is_complete = goal.current_amount >= goal.target_amount;

        env.storage()
            .persistent()
            .set(&DataKey::Goal(goal_id), &goal);

        // Store contribution record for reversal eligibility tracking
        let contrib_id: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::LastContribId(goal_id))
            .unwrap_or(0)
            + 1;
        let record = ContributionRecord {
            amount: actual_contribution,
            contributed_at: current_time,
            idempotency_token: idempotency_token.clone(),
            reversed: false,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Contribution(goal_id, contrib_id), &record);
        env.storage()
            .persistent()
            .set(&DataKey::LastContribId(goal_id), &contrib_id);
        env.storage().persistent().set(&token_key, &contrib_id);

        Self::check_and_emit_milestones(&env, goal_id);
        GoalEvents::goal_contributed(
            &env,
            goal_id,
            &caller,
            actual_contribution,
            goal.current_amount,
        );

        contrib_id
    }

    /// Reverses a contribution within the reversal period (REVERSAL_PERIOD_SECS).
    ///
    /// # Arguments
    /// * `caller`       - Must be the goal owner.
    /// * `goal_id`      - The goal that was contributed to.
    /// * `contrib_id`   - The contribution ID returned by `contribute_to_goal`.
    ///
    /// # Errors
    /// * `GoalNotFound`        - Goal does not exist.
    /// * `Unauthorized`        - Caller is not the goal owner.
    /// * `ContributionNotFound`- Contribution ID unknown or already reversed.
    /// * `ReversalExpired`     - The reversal window has closed.
    pub fn reverse_contribution(env: Env, caller: Address, goal_id: u64, contrib_id: u64) -> i128 {
        caller.require_auth();

        let mut goal: SavingsGoal = env
            .storage()
            .persistent()
            .get(&DataKey::Goal(goal_id))
            .unwrap_or_else(|| panic_with_error!(&env, SavingsGoalError::GoalNotFound));

        if goal.user != caller {
            panic_with_error!(&env, SavingsGoalError::Unauthorized);
        }

        let mut record: ContributionRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Contribution(goal_id, contrib_id))
            .unwrap_or_else(|| panic_with_error!(&env, SavingsGoalError::ContributionNotFound));

        if record.reversed {
            panic_with_error!(&env, SavingsGoalError::ContributionNotFound);
        }

        let now = env.ledger().timestamp();
        if now > record.contributed_at.saturating_add(REVERSAL_PERIOD_SECS) {
            panic_with_error!(&env, SavingsGoalError::ReversalExpired);
        }

        goal.current_amount = goal.current_amount.saturating_sub(record.amount);
        goal.is_complete = goal.current_amount >= goal.target_amount;
        record.reversed = true;

        env.storage()
            .persistent()
            .set(&DataKey::Contribution(goal_id, contrib_id), &record);
        env.storage()
            .persistent()
            .set(&DataKey::Goal(goal_id), &goal);

        goal.current_amount
    }

    /// Clones an existing savings goal with a new name and zero balance.
    pub fn clone_savings_goal(
        env: Env,
        caller: Address,
        goal_id: u64,
        new_goal_name: Symbol,
    ) -> u64 {
        caller.require_auth();

        let existing_goal: SavingsGoal = env
            .storage()
            .persistent()
            .get(&DataKey::Goal(goal_id))
            .unwrap_or_else(|| panic_with_error!(&env, SavingsGoalError::GoalNotFound));

        if existing_goal.user != caller {
            panic_with_error!(&env, SavingsGoalError::Unauthorized);
        }

        // Check for duplicate goal name for this user
        if validate_goal_name_unique(&env, &caller, &new_goal_name).is_err() {
            panic_with_error!(&env, SavingsGoalError::InvalidGoalName);
        }

        let mut goal_id_counter: u64 = env
            .storage()
            .instance()
            .get(&DataKey::LastGoalId)
            .unwrap_or(0);
        goal_id_counter += 1;

        let created_at = env.ledger().timestamp();
        // Inherit lock duration if it was originally set
        let lock_duration = if existing_goal.unlock_at > existing_goal.created_at {
            existing_goal.unlock_at - existing_goal.created_at
        } else {
            0
        };
        let unlock_at = if lock_duration > 0 {
            created_at.saturating_add(lock_duration)
        } else {
            0
        };

        // Inherit expiration duration if it was originally set
        let expiration_duration = if existing_goal.expires_at > existing_goal.created_at {
            existing_goal.expires_at - existing_goal.created_at
        } else {
            0
        };
        let expires_at = if expiration_duration > 0 {
            created_at.saturating_add(expiration_duration)
        } else {
            0
        };

        let new_goal = SavingsGoal {
            goal_id: goal_id_counter,
            user: caller.clone(),
            goal_name: new_goal_name.clone(),
            target_amount: existing_goal.target_amount,
            current_amount: 0, // Reset balance
            deadline: existing_goal.deadline,
            created_at,
            is_active: true,
            is_complete: false,
            priority: existing_goal.priority,
            unlock_at,
            expires_at,
            penalty_bps: existing_goal.penalty_bps,
        };

        // Store the goal
        env.storage()
            .persistent()
            .set(&DataKey::Goal(goal_id_counter), &new_goal);
        // Store name-to-id mapping
        env.storage().persistent().set(
            &DataKey::GoalByName(caller.clone(), new_goal_name.clone()),
            &goal_id_counter,
        );

        // Update user's goal list
        let mut user_goals: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::UserGoals(caller.clone()))
            .unwrap_or(Vec::new(&env));
        user_goals.push_back(goal_id_counter);
        env.storage()
            .persistent()
            .set(&DataKey::UserGoals(caller.clone()), &user_goals);

        // Update global counter
        env.storage()
            .instance()
            .set(&DataKey::LastGoalId, &goal_id_counter);

        let total_goals: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalGoalsCreated)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::TotalGoalsCreated, &(total_goals + 1));

        // Emit creation event (reusing batch 0 for manual creation)
        GoalEvents::goal_created(&env, 0, &new_goal);

        goal_id_counter
    }

    /// Withdraws funds from a savings goal. Rejects withdrawals before unlock time when locked.
    pub fn withdraw_from_goal(env: Env, caller: Address, goal_id: u64, amount: i128) -> i128 {
        caller.require_auth();

        if amount <= 0 {
            panic_with_error!(&env, SavingsGoalError::InvalidAmount);
        }

        let mut goal: SavingsGoal = env
            .storage()
            .persistent()
            .get(&DataKey::Goal(goal_id))
            .unwrap_or_else(|| panic_with_error!(&env, SavingsGoalError::GoalNotFound));

        if goal.user != caller {
            panic_with_error!(&env, SavingsGoalError::Unauthorized);
        }
        if !goal.is_active && !goal.is_complete {
            panic_with_error!(&env, SavingsGoalError::GoalNotActive);
        }

        let now = env.ledger().timestamp();
        if goal.unlock_at > 0 && now < goal.unlock_at {
            GoalEvents::goal_withdraw_locked(&env, goal_id, &caller, goal.unlock_at);
            panic_with_error!(&env, SavingsGoalError::GoalLocked);
        }

        let penalty = if goal.is_complete {
            0
        } else {
            let penalty_contract: Address = env
                .storage()
                .instance()
                .get(&DataKey::PenaltyContract)
                .expect("penalty contract not configured");
            let client = PenaltyContractClient::new(&env, &penalty_contract);
            client.calculate_penalty_fee_with_bps(&amount, &goal.penalty_bps)
        };
        let gross_amount = amount.checked_add(penalty).unwrap_or(i128::MAX);

        if gross_amount > goal.current_amount {
            panic_with_error!(&env, SavingsGoalError::InsufficientBalance);
        }

        goal.current_amount -= gross_amount;
        goal.is_complete = goal.current_amount >= goal.target_amount;

        env.storage()
            .persistent()
            .set(&DataKey::Goal(goal_id), &goal);

        GoalEvents::goal_withdrawn(&env, goal_id, &caller, amount, goal.current_amount);
        goal.current_amount
    }

    /// Returns milestone percentages already triggered for a goal.
    pub fn get_triggered_milestone_percents(env: Env, goal_id: u64) -> Vec<u32> {
        env.storage()
            .persistent()
            .get(&DataKey::GoalMilestonesPercent(goal_id))
            .unwrap_or(Vec::new(&env))
    }

    /// Returns whether a goal is currently locked against withdrawals.
    pub fn is_goal_locked(env: Env, goal_id: u64) -> bool {
        let goal: SavingsGoal = match env.storage().persistent().get(&DataKey::Goal(goal_id)) {
            Some(g) => g,
            None => return false,
        };
        goal.unlock_at > 0 && env.ledger().timestamp() < goal.unlock_at
    }

    /// Emits milestone events automatically when goal progress crosses thresholds.
    /// Call this after updating a goal's current_amount.
    pub fn check_and_emit_milestones(env: &Env, goal_id: u64) {
        let mut goal: SavingsGoal = match env.storage().persistent().get(&DataKey::Goal(goal_id)) {
            Some(g) => g,
            None => return,
        };
        let milestones = [25, 50, 75, 100];
        let mut triggered: Vec<u32> = env
            .storage()
            .persistent()
            .get(&DataKey::GoalMilestonesPercent(goal_id))
            .unwrap_or(Vec::new(env));
        let mut progress = if goal.target_amount > 0 {
            (goal.current_amount * 100 / goal.target_amount) as u32
        } else {
            0
        };
        let mut newly_triggered_100 = false;
        if progress > 100 {
            progress = 100;
        }
        let is_complete = goal.current_amount >= goal.target_amount;
        if goal.is_complete != is_complete {
            goal.is_complete = is_complete;
            env.storage()
                .persistent()
                .set(&DataKey::Goal(goal_id), &goal);
            if is_complete {
                GoalEvents::goal_completed(env, goal_id, &goal.user, goal.target_amount);
            }
            env.storage()
                .persistent()
                .set(&DataKey::Goal(goal_id), &goal);
        }
        for &milestone in milestones.iter() {
            if progress >= milestone && !triggered.contains(&milestone) {
                // Emit event
                GoalEvents::milestone_achieved_percent(env, goal_id, milestone);
                triggered.push_back(milestone);
                if milestone == 100 {
                    newly_triggered_100 = true;
                }
            }
        }
        env.storage()
            .persistent()
            .set(&DataKey::GoalMilestonesPercent(goal_id), &triggered);

        // Auto-close the goal the first time 100% is reached
        if newly_triggered_100 && goal.is_active {
            let mut closed_goal = goal.clone();
            closed_goal.is_active = false;
            let closed_at = env.ledger().sequence() as u64;
            env.storage()
                .persistent()
                .set(&DataKey::Goal(goal_id), &closed_goal);
            env.storage()
                .persistent()
                .set(&DataKey::GoalClosedAt(goal_id), &closed_at);
            GoalEvents::goal_closed(
                env,
                goal_id,
                &closed_goal.user,
                closed_goal.current_amount,
                closed_at,
            );
        }

        // Issue a goal completion certificate the first time 100% is reached
        if newly_triggered_100 {
            let cert_key = DataKey::Certificate(goal_id);
            if !env.storage().persistent().has(&cert_key) {
                let certificate = GoalCertificate {
                    goal_id,
                    user: goal.user.clone(),
                    target_amount: goal.target_amount,
                    issued_at: env.ledger().timestamp(),
                };
                env.storage().persistent().set(&cert_key, &certificate);
            }
        }
    }
    // ...existing code...

    /// Partially withdraws funds from a savings goal.
    ///
    /// Updates the remaining current amount and goal completion state.
    /// The caller must be the goal owner.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `caller` - The address requesting the withdrawal (must be goal owner)
    /// * `goal_id` - The ID of the goal to withdraw from
    /// * `amount` - The amount to withdraw (must be > 0, <= current_amount)
    pub fn partial_withdraw(env: Env, caller: Address, goal_id: u64, amount: i128) {
        caller.require_auth();

        if amount <= 0 {
            panic_with_error!(&env, SavingsGoalError::InvalidBatch);
        }

        let mut goal: SavingsGoal = env
            .storage()
            .persistent()
            .get(&DataKey::Goal(goal_id))
            .unwrap_or_else(|| panic_with_error!(&env, SavingsGoalError::InvalidBatch));

        // Verify caller is the goal owner
        if goal.user != caller {
            panic_with_error!(&env, SavingsGoalError::Unauthorized);
        }

        // Verify goal is active or already complete
        if !goal.is_active && !goal.is_complete {
            panic_with_error!(&env, SavingsGoalError::GoalNotActive);
        }

        let now = env.ledger().timestamp();
        if goal.unlock_at > 0 && now < goal.unlock_at {
            GoalEvents::goal_withdraw_locked(&env, goal_id, &caller, goal.unlock_at);
            panic_with_error!(&env, SavingsGoalError::GoalLocked);
        }

        let penalty = if goal.is_complete {
            0
        } else {
            let penalty_contract: Address = env
                .storage()
                .instance()
                .get(&DataKey::PenaltyContract)
                .expect("penalty contract not configured");
            let client = PenaltyContractClient::new(&env, &penalty_contract);
            client.calculate_penalty_fee_with_bps(&amount, &goal.penalty_bps)
        };
        let gross_amount = amount.checked_add(penalty).unwrap_or(i128::MAX);

        // Verify sufficient balance including penalty
        if gross_amount > goal.current_amount {
            panic_with_error!(&env, SavingsGoalError::InsufficientBalance);
        }

        // Update current amount
        goal.current_amount = goal.current_amount.checked_sub(gross_amount).unwrap_or(0);

        // Update completion status
        let was_complete = goal.is_complete;
        goal.is_complete = goal.current_amount >= goal.target_amount;

        // If goal was complete but is no longer, it stays active
        if was_complete && !goal.is_complete {
            goal.is_active = true;
        }

        // Store updated goal
        env.storage()
            .persistent()
            .set(&DataKey::Goal(goal_id), &goal);

        // Update milestones if progress changed
        Self::check_and_emit_milestones(&env, goal_id);

        // Emit withdrawal event
        GoalEvents::partial_withdrawal(&env, goal_id, &caller, amount, goal.current_amount);
    }

    /// Updates the name of an existing savings goal.
    ///
    /// The caller must be the goal owner. Emits a rename event on success.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `caller` - The address requesting the rename (must be goal owner)
    /// * `goal_id` - The ID of the goal to rename
    /// * `new_name` - The new name for the goal
    pub fn update_goal_name(env: Env, caller: Address, goal_id: u64, new_name: Symbol) {
        caller.require_auth();

        let mut goal: SavingsGoal = env
            .storage()
            .persistent()
            .get(&DataKey::Goal(goal_id))
            .unwrap_or_else(|| panic_with_error!(&env, SavingsGoalError::InvalidBatch));

        // Verify caller is the goal owner
        if goal.user != caller {
            panic_with_error!(&env, SavingsGoalError::Unauthorized);
        }

        let old_name = goal.goal_name.clone();
        goal.goal_name = new_name.clone();

        // Store updated goal
        env.storage()
            .persistent()
            .set(&DataKey::Goal(goal_id), &goal);

        // Emit rename event
        GoalEvents::goal_renamed(&env, goal_id, &old_name, &new_name);
    }

    /// Retrieves a savings goal by ID.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `goal_id` - The ID of the goal to retrieve
    ///
    /// # Returns
    /// * `Option<SavingsGoal>` - The goal if found
    pub fn get_goal(env: Env, goal_id: u64) -> Option<SavingsGoal> {
        env.storage().persistent().get(&DataKey::Goal(goal_id))
    }

    /// Gets current progress details for a savings goal.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `goal_id` - The ID of the goal to query
    ///
    /// # Returns
    /// * `Option<SavingsGoalProgress>` - Progress summary if goal exists
    pub fn get_goal_progress(env: Env, goal_id: u64) -> Option<SavingsGoalProgress> {
        env.storage()
            .persistent()
            .get(&DataKey::Goal(goal_id))
            .map(|goal: SavingsGoal| {
                let mut progress_percentage = if goal.target_amount > 0 {
                    (goal.current_amount * 100 / goal.target_amount) as u32
                } else {
                    0
                };
                if progress_percentage > 100 {
                    progress_percentage = 100;
                }
                let is_complete = goal.current_amount >= goal.target_amount;
                SavingsGoalProgress {
                    goal_id: goal.goal_id,
                    current_amount: goal.current_amount,
                    target_amount: goal.target_amount,
                    progress_percentage,
                    is_complete,
                }
            })
    }

    /// Retrieves all goal IDs for a specific user.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `user` - The user's address
    ///
    /// # Returns
    /// * `Vec<u64>` - Vector of goal IDs for the user
    pub fn get_user_goals(env: Env, user: Address) -> Vec<u64> {
        env.storage()
            .persistent()
            .get(&DataKey::UserGoals(user))
            .unwrap_or(Vec::new(&env))
    }

    /// Returns a previously recorded contribution for a goal.
    pub fn get_contribution_record(
        env: Env,
        goal_id: u64,
        contribution_id: u64,
    ) -> Option<ContributionRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::Contribution(goal_id, contribution_id))
    }

    /// Returns the configured alert thresholds for a goal, falling back to contract defaults.
    pub fn get_goal_alert_thresholds(env: Env, goal_id: u64) -> Vec<u64> {
        env.storage()
            .persistent()
            .get(&DataKey::GoalDeadlineAlertThresholds(goal_id))
            .unwrap_or_else(|| {
                env.storage()
                    .instance()
                    .get(&DataKey::DefaultDeadlineAlertThresholds)
                    .unwrap_or_else(|| Self::default_deadline_alert_thresholds(&env))
            })
    }

    /// Returns the alert thresholds that have already fired for a goal.
    pub fn get_goal_alerts_emitted(env: Env, goal_id: u64) -> Vec<u64> {
        let configured = Self::get_goal_alert_thresholds(env.clone(), goal_id);
        let mut emitted = Vec::new(&env);
        for threshold in configured.iter() {
            if env
                .storage()
                .persistent()
                .has(&DataKey::GoalDeadlineAlertSent(goal_id, threshold))
            {
                emitted.push_back(threshold);
            }
        }
        emitted
    }

    /// Updates the default deadline alert thresholds for newly created and unconfigured goals.
    pub fn set_default_alert_thresholds(env: Env, caller: Address, thresholds: Vec<u64>) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let normalized = Self::normalize_deadline_alert_thresholds(&env, thresholds);
        if normalized.is_empty() {
            panic_with_error!(&env, SavingsGoalError::InvalidAlertThreshold);
        }

        env.storage()
            .instance()
            .set(&DataKey::DefaultDeadlineAlertThresholds, &normalized);
    }

    /// Overrides deadline alert thresholds for a specific goal.
    pub fn set_goal_alert_thresholds(
        env: Env,
        caller: Address,
        goal_id: u64,
        thresholds: Vec<u64>,
    ) {
        caller.require_auth();

        let goal: SavingsGoal = env
            .storage()
            .persistent()
            .get(&DataKey::Goal(goal_id))
            .unwrap_or_else(|| panic_with_error!(&env, SavingsGoalError::GoalNotFound));

        if caller != goal.user {
            Self::require_admin(&env, &caller);
        }

        let normalized = Self::normalize_deadline_alert_thresholds(&env, thresholds);
        if normalized.is_empty() {
            panic_with_error!(&env, SavingsGoalError::InvalidAlertThreshold);
        }

        env.storage()
            .persistent()
            .set(&DataKey::GoalDeadlineAlertThresholds(goal_id), &normalized);

        GoalEvents::deadline_alert_thresholds_updated(&env, goal_id, &goal.user, normalized.len());
    }

    /// Emits any due deadline reminder events for an incomplete goal.
    ///
    /// Returns the number of newly emitted reminders.
    pub fn process_goal_alerts(env: Env, goal_id: u64) -> u32 {
        let goal: SavingsGoal = match env.storage().persistent().get(&DataKey::Goal(goal_id)) {
            Some(goal) => goal,
            None => return 0,
        };

        if !goal.is_active || goal.is_complete {
            return 0;
        }

        let current_sequence = env.ledger().sequence() as u64;
        if current_sequence >= goal.deadline {
            return 0;
        }

        let remaining_ledgers = goal.deadline - current_sequence;
        let remaining_amount = goal.target_amount.saturating_sub(goal.current_amount);
        let thresholds = Self::get_goal_alert_thresholds(env.clone(), goal_id);
        let mut emitted = 0u32;

        for threshold in thresholds.iter() {
            let sent_key = DataKey::GoalDeadlineAlertSent(goal_id, threshold);
            if remaining_ledgers <= threshold && !env.storage().persistent().has(&sent_key) {
                GoalEvents::deadline_alert(
                    &env,
                    goal_id,
                    &goal.user,
                    threshold,
                    remaining_ledgers,
                    remaining_amount,
                );
                env.storage().persistent().set(&sent_key, &true);
                emitted += 1;
            }
        }

        emitted
    }

    /// Number of recurring auto-contribution cycles due between
    /// `last_contributed_at` and the current ledger time for a given interval.
    ///
    /// Use `604_800` seconds for a weekly schedule or `2_592_000` for monthly.
    /// Missed cycles are counted (not collapsed to one) so they can be settled
    /// safely in a single catch-up call.
    pub fn contributions_due(env: Env, last_contributed_at: u64, interval_seconds: u64) -> u64 {
        if interval_seconds == 0 {
            return 0;
        }

        let now = env.ledger().timestamp();
        if now <= last_contributed_at {
            0
        } else {
            (now - last_contributed_at) / interval_seconds
        }
    }

    /// Records a historical snapshot of the goal's current progress.
    ///
    /// The caller must be the goal owner. Emits an event on success.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `caller` - The address recording the snapshot (must be goal owner)
    /// * `goal_id` - The ID of the goal
    pub fn record_goal_snapshot(env: Env, caller: Address, goal_id: u64) {
        caller.require_auth();

        let goal: SavingsGoal = env
            .storage()
            .persistent()
            .get(&DataKey::Goal(goal_id))
            .unwrap_or_else(|| panic_with_error!(&env, SavingsGoalError::GoalNotFound));

        // Verify caller is the goal owner
        if goal.user != caller {
            panic_with_error!(&env, SavingsGoalError::Unauthorized);
        }

        let now = env.ledger().timestamp();
        let snapshot = GoalSnapshot {
            goal_id,
            amount: goal.current_amount,
            timestamp: now,
        };

        let mut snapshots: Vec<GoalSnapshot> = env
            .storage()
            .persistent()
            .get(&DataKey::GoalSnapshots(goal_id))
            .unwrap_or(Vec::new(&env));

        snapshots.push_back(snapshot);

        env.storage()
            .persistent()
            .set(&DataKey::GoalSnapshots(goal_id), &snapshots);

        env.storage().persistent().extend_ttl(
            &DataKey::GoalSnapshots(goal_id),
            PERSISTENT_TTL_BUMP,
            PERSISTENT_TTL_BUMP,
        );

        GoalEvents::goal_snapshot_recorded(&env, goal_id, goal.current_amount, now);
    }

    /// Retrieves all historical snapshots for a specific goal.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `goal_id` - The goal ID
    ///
    /// # Returns
    /// * `Vec<GoalSnapshot>` - Vector of historical snapshots
    pub fn get_goal_snapshots(env: Env, goal_id: u64) -> Vec<GoalSnapshot> {
        let key = DataKey::GoalSnapshots(goal_id);
        if env.storage().persistent().has(&key) {
            env.storage()
                .persistent()
                .extend_ttl(&key, PERSISTENT_TTL_BUMP, PERSISTENT_TTL_BUMP);
        }

        env.storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(&env))
    }

    /// Returns the admin address.
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Contract not initialized")
    }

    /// Updates the admin address.
    pub fn set_admin(env: Env, current_admin: Address, new_admin: Address) {
        current_admin.require_auth();
        Self::require_admin(&env, &current_admin);

        env.storage().instance().set(&DataKey::Admin, &new_admin);
    }

    /// Sets the penalty contract address for early-withdrawal fee calculation.
    /// May only be called by the admin.
    pub fn set_penalty_contract(env: Env, caller: Address, penalty_contract: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        env.storage()
            .instance()
            .set(&DataKey::PenaltyContract, &penalty_contract);
    }

    /// Returns the last created batch ID.
    pub fn get_last_batch_id(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::LastBatchId)
            .unwrap_or(0)
    }

    /// Returns the last created goal ID.
    pub fn get_last_goal_id(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::LastGoalId)
            .unwrap_or(0)
    }

    /// Returns the total number of goals created.
    pub fn get_total_goals_created(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalGoalsCreated)
            .unwrap_or(0)
    }

    /// Returns the total number of batches processed.
    pub fn get_total_batches_processed(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalBatchesProcessed)
            .unwrap_or(0)
    }

    /// Retrieves a milestone by ID.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `milestone_id` - The ID of the milestone to retrieve
    ///
    /// # Returns
    /// * `Option<MilestoneAchievement>` - The milestone if found
    pub fn get_milestone(env: Env, milestone_id: u64) -> Option<MilestoneAchievement> {
        env.storage()
            .persistent()
            .get(&DataKey::Milestone(milestone_id))
    }

    /// Retrieves all milestone IDs for a specific goal.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `goal_id` - The goal ID
    ///
    /// # Returns
    /// * `Vec<u64>` - Vector of milestone IDs for the goal
    pub fn get_goal_milestones(env: Env, goal_id: u64) -> Vec<u64> {
        env.storage()
            .persistent()
            .get(&DataKey::GoalMilestones(goal_id))
            .unwrap_or(Vec::new(&env))
    }

    /// Returns the last created milestone ID.
    pub fn get_last_milestone_id(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::LastMilestoneId)
            .unwrap_or(0)
    }

    /// Returns the total number of milestones achieved.
    pub fn get_total_milestones_achieved(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalMilestonesAchieved)
            .unwrap_or(0)
    }

    /// Returns the ledger sequence at which a goal was automatically closed,
    /// or `None` if the goal has not yet been closed.
    pub fn get_goal_closed_at(env: Env, goal_id: u64) -> Option<u64> {
        env.storage()
            .persistent()
            .get(&DataKey::GoalClosedAt(goal_id))
    }

    /// Retrieves the goal completion certificate for a given goal.
    /// Returns `None` if the goal hasn't reached 100% yet.
    pub fn get_certificate(env: Env, goal_id: u64) -> Option<GoalCertificate> {
        env.storage()
            .persistent()
            .get(&DataKey::Certificate(goal_id))
    }

    /// Merges an active source goal into an active target goal.
    /// The caller must own both goals. The source goal is marked as inactive
    /// and its balance and progress are rolled into the target goal.
    pub fn merge_goals(env: Env, caller: Address, source_goal_id: u64, target_goal_id: u64) {
        caller.require_auth();

        if source_goal_id == target_goal_id {
            panic_with_error!(&env, SavingsGoalError::InvalidBatch); // Can't merge into itself
        }

        let mut source_goal: SavingsGoal = env
            .storage()
            .persistent()
            .get(&DataKey::Goal(source_goal_id))
            .unwrap_or_else(|| panic_with_error!(&env, SavingsGoalError::GoalNotFound));

        let mut target_goal: SavingsGoal = env
            .storage()
            .persistent()
            .get(&DataKey::Goal(target_goal_id))
            .unwrap_or_else(|| panic_with_error!(&env, SavingsGoalError::GoalNotFound));

        if source_goal.user != caller || target_goal.user != caller {
            panic_with_error!(&env, SavingsGoalError::Unauthorized);
        }

        if !source_goal.is_active || !target_goal.is_active {
            panic_with_error!(&env, SavingsGoalError::GoalNotActive);
        }

        // Perform merge
        let transferred_amount = source_goal.current_amount;
        target_goal.current_amount = target_goal
            .current_amount
            .saturating_add(transferred_amount);
        source_goal.current_amount = 0;
        source_goal.is_active = false;

        target_goal.is_complete = target_goal.current_amount >= target_goal.target_amount;

        env.storage()
            .persistent()
            .set(&DataKey::Goal(source_goal_id), &source_goal);
        env.storage()
            .persistent()
            .set(&DataKey::Goal(target_goal_id), &target_goal);

        Self::check_and_emit_milestones(&env, target_goal_id);

        let topics = (symbol_short!("goal"), symbol_short!("merged"));
        env.events()
            .publish(topics, (source_goal_id, target_goal_id, transferred_amount));
    }

    fn default_deadline_alert_thresholds(env: &Env) -> Vec<u64> {
        let mut thresholds = Vec::new(env);
        thresholds.push_back(100);
        thresholds.push_back(25);
        thresholds.push_back(5);
        thresholds
    }

    fn normalize_deadline_alert_thresholds(env: &Env, thresholds: Vec<u64>) -> Vec<u64> {
        let mut normalized = Vec::new(env);
        for threshold in thresholds.iter() {
            if threshold > 0 && !normalized.contains(&threshold) {
                normalized.push_back(threshold);
            }
        }
        normalized
    }

    // Internal helper to verify admin
    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Contract not initialized");

        if *caller != admin {
            panic_with_error!(env, SavingsGoalError::Unauthorized);
        }
    }

    /// Sets prerequisite goal IDs for a specific goal. The caller must be the goal owner.
    /// Prevents circular dependencies when setting prerequisites.
    pub fn set_goal_prerequisites(env: Env, caller: Address, goal_id: u64, prereq_ids: Vec<u64>) {
        caller.require_auth();

        let goal: SavingsGoal = env
            .storage()
            .persistent()
            .get(&DataKey::Goal(goal_id))
            .unwrap_or_else(|| panic_with_error!(&env, SavingsGoalError::GoalNotFound));

        if goal.user != caller {
            panic_with_error!(&env, SavingsGoalError::Unauthorized);
        }

        // Validate prerequisites exist, belong to the same owner, and do not form cycles
        for p in prereq_ids.iter() {
            if p == goal_id {
                panic_with_error!(&env, SavingsGoalError::DependencyNotMet);
            }

            let pgoal: SavingsGoal = env
                .storage()
                .persistent()
                .get(&DataKey::Goal(p))
                .unwrap_or_else(|| panic_with_error!(&env, SavingsGoalError::GoalNotFound));

            if pgoal.user != caller {
                // Disallow cross-user prerequisites
                panic_with_error!(&env, SavingsGoalError::Unauthorized);
            }

            // Check for circular dependency: is there a path from p -> goal_id ?
            if Self::has_dependency_path(&env, p, goal_id) {
                panic_with_error!(&env, SavingsGoalError::DependencyNotMet);
            }
        }

        env.storage()
            .persistent()
            .set(&DataKey::GoalPrereqs(goal_id), &prereq_ids);
    }

    // Depth-first search to detect if there's a path from `start` to `target`.
    fn has_dependency_path(env: &Env, start: u64, target: u64) -> bool {
        let mut stack: Vec<u64> = Vec::new(env);
        let mut visited: Vec<u64> = Vec::new(env);
        stack.push_back(start);

        while stack.len() > 0 {
            let node = stack.pop_back().unwrap();
            if node == target {
                return true;
            }
            if visited.contains(&node) {
                continue;
            }
            visited.push_back(node);

            let neighbors: Vec<u64> = env
                .storage()
                .persistent()
                .get(&DataKey::GoalPrereqs(node))
                .unwrap_or(Vec::new(env));

            for n in neighbors.iter() {
                if !visited.contains(&n) {
                    stack.push_back(n);
                }
            }
        }

        false
    }
}

#[cfg(test)]
mod test;

// Place test-only contract impl at the very end, outside all other blocks
#[cfg(test)]
#[contractimpl]
impl SavingsGoalsContract {
    /// Test-only: update a goal's current_amount for test setup.
    pub fn test_set_goal_current_amount(env: Env, goal_id: u64, amount: i128) {
        let contract_id = env.current_contract_address();
        env.as_contract(&contract_id, || {
            let key = crate::types::DataKey::Goal(goal_id);
            if let Some(mut goal) = env
                .storage()
                .persistent()
                .get::<crate::types::DataKey, crate::types::SavingsGoal>(&key)
            {
                goal.current_amount = amount;
                goal.is_complete = amount >= goal.target_amount;
                env.storage().persistent().set(&key, &goal);
            }
        });
    }
}

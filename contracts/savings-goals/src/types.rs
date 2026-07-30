//! Data types and events for batch savings goal operations.

use soroban_sdk::{contracttype, symbol_short, Address, Bytes, Env, Symbol, Vec};

/// Maximum number of user-goal pairs in a single batch for optimization.
pub const MAX_BATCH_SIZE: u32 = 100;

/// Minimum goal amount (1 XLM in stroops)
pub const MIN_GOAL_AMOUNT: i128 = 10_000_000;

/// Maximum goal amount (1 billion XLM in stroops)
pub const MAX_GOAL_AMOUNT: i128 = 1_000_000_000_000_000_000;

/// Represents a savings goal request for a user.
#[derive(Clone, Debug)]
#[contracttype]
pub struct SavingsGoalRequest {
    /// User's address
    pub user: Address,
    /// Goal name/description (e.g., "vacation", "emergency_fund", "house")
    pub goal_name: Symbol,
    /// Target amount to save (in stroops)
    pub target_amount: i128,
    /// Deadline timestamp (ledger sequence number)
    pub deadline: u64,
    /// Initial contribution amount (optional, can be 0)
    pub initial_contribution: i128,
    /// Goal priority. Higher values are funded first for automatic contributions.
    pub priority: u32,
    /// Optional lock duration in seconds (0 = no lock, withdrawals allowed immediately)
    pub lock_duration_seconds: u64,
    /// Early withdrawal penalty in basis points (0 = no penalty)
    pub penalty_bps: u32,
    /// Expiration duration in seconds (0 = no expiration)
    pub expiration_seconds: u64,
}

/// Represents a completion certificate for a savings goal.
#[derive(Clone, Debug)]
#[contracttype]
pub struct GoalCertificate {
    /// Associated goal ID
    pub goal_id: u64,
    /// User's address
    pub user: Address,
    /// Target amount achieved (in stroops)
    pub target_amount: i128,
    /// Ledger sequence when certificate was issued
    pub issued_at: u64,
}

/// Represents a created savings goal.
#[derive(Clone, Debug)]
#[contracttype]
pub struct SavingsGoal {
    /// Unique goal ID
    pub goal_id: u64,
    /// User's address
    pub user: Address,
    /// Goal name/description
    pub goal_name: Symbol,
    /// Target amount to save (in stroops)
    pub target_amount: i128,
    /// Current saved amount (in stroops)
    pub current_amount: i128,
    /// Deadline timestamp (ledger sequence number)
    pub deadline: u64,
    /// Goal creation timestamp
    pub created_at: u64,
    /// Whether the goal is active
    pub is_active: bool,
    /// Whether the goal has reached its target amount
    pub is_complete: bool,
    /// Goal priority. Higher values are funded first for automatic contributions.
    pub priority: u32,
    /// Timestamp after which withdrawals are allowed (0 = no lock)
    pub unlock_at: u64,
    /// Timestamp after which the goal expires (0 = no expiration)
    pub expires_at: u64,
    /// Early withdrawal penalty in basis points (0 = no penalty)
    pub penalty_bps: u32,
}

/// Represents progress information for a savings goal.
#[derive(Clone, Debug)]
#[contracttype]
pub struct SavingsGoalProgress {
    /// Unique goal ID
    pub goal_id: u64,
    /// Current saved amount
    pub current_amount: i128,
    /// Target amount to save
    pub target_amount: i128,
    /// Progress percentage capped at 100
    pub progress_percentage: u32,
    /// Whether the goal is complete
    pub is_complete: bool,
}

/// Represents a historical snapshot of a savings goal.
#[derive(Clone, Debug)]
#[contracttype]
pub struct GoalSnapshot {
    /// Associated goal ID
    pub goal_id: u64,
    /// Amount saved at the time of snapshot
    pub amount: i128,
    /// Ledger sequence when the snapshot was recorded
    pub timestamp: u64,
}

/// Result of processing a single goal creation.
#[derive(Clone, Debug)]
#[contracttype]
pub enum GoalResult {
    Success(SavingsGoal),
    Failure(Address, u32), // user address, error code
}

/// Aggregated metrics for a batch of goal creations.
#[derive(Clone, Debug)]
#[contracttype]
pub struct BatchGoalMetrics {
    /// Total number of goal requests
    pub total_requests: u32,
    /// Number of successful goal creations
    pub successful_goals: u32,
    /// Number of failed goal creations
    pub failed_goals: u32,
    /// Total target amount across all goals
    pub total_target_amount: i128,
    /// Total initial contributions
    pub total_initial_contributions: i128,
    /// Average goal amount
    pub avg_goal_amount: i128,
    /// Batch processing timestamp
    pub processed_at: u64,
}

/// Result of batch goal creation.
#[derive(Clone, Debug)]
#[contracttype]
pub struct BatchGoalResult {
    /// Batch ID
    pub batch_id: u64,
    /// Total number of requests
    pub total_requests: u32,
    /// Number of successful creations
    pub successful: u32,
    /// Number of failed creations
    pub failed: u32,
    /// Individual goal results
    pub results: Vec<GoalResult>,
    /// Aggregated metrics
    pub metrics: BatchGoalMetrics,
}

/// Represents a milestone achievement request for a goal.
#[derive(Clone, Debug)]
#[contracttype]
pub struct MilestoneAchievementRequest {
    /// Goal ID to mark milestone for
    pub goal_id: u64,
    /// User's address (must be the goal owner)
    pub user: Address,
    /// Milestone percentage (1-100)
    pub milestone_percentage: u32,
    /// Achievement timestamp (ledger sequence number)
    pub achieved_at: u64,
}

/// Represents an achieved milestone for a goal.
#[derive(Clone, Debug)]
#[contracttype]
pub struct MilestoneAchievement {
    /// Unique milestone ID
    pub milestone_id: u64,
    /// Associated goal ID
    pub goal_id: u64,
    /// User's address
    pub user: Address,
    /// Milestone percentage (1-100)
    pub milestone_percentage: u32,
    /// Current goal amount at time of achievement
    pub goal_amount_at_achievement: i128,
    /// Ledger sequence when milestone was achieved
    pub achieved_at: u64,
}

/// Result of processing a single milestone achievement.
#[derive(Clone, Debug)]
#[contracttype]
pub enum MilestoneResult {
    Success(MilestoneAchievement),
    Failure(u64, u32), // goal_id, error_code
}

/// Aggregated metrics for a batch of milestone achievements.
#[derive(Clone, Debug)]
#[contracttype]
pub struct BatchMilestoneMetrics {
    /// Total number of milestone requests
    pub total_requests: u32,
    /// Number of successful milestones
    pub successful_milestones: u32,
    /// Number of failed milestones
    pub failed_milestones: u32,
    /// Total percentage points achieved
    pub total_percentage_points: u32,
    /// Average percentage per milestone
    pub avg_percentage: u32,
    /// Batch processing timestamp
    pub processed_at: u64,
}

/// Result of batch milestone achievement marking.
#[derive(Clone, Debug)]
#[contracttype]
pub struct BatchMilestoneResult {
    /// Batch ID
    pub batch_id: u64,
    /// Total number of requests
    pub total_requests: u32,
    /// Number of successful milestones
    pub successful: u32,
    /// Number of failed milestones
    pub failed: u32,
    /// Individual milestone results
    pub results: Vec<MilestoneResult>,
    /// Aggregated metrics
    pub metrics: BatchMilestoneMetrics,
}

/// Reversal window in seconds (24 hours).
pub const REVERSAL_PERIOD_SECS: u64 = 86_400;

/// A record of a single contribution, stored for reversal eligibility.
#[derive(Clone, Debug)]
#[contracttype]
pub struct ContributionRecord {
    /// The contribution amount.
    pub amount: i128,
    /// Ledger timestamp when the contribution was made.
    pub contributed_at: u64,
    /// Client-supplied idempotency token used to dedupe retries.
    pub idempotency_token: Bytes,
    /// Whether this contribution has already been reversed.
    pub reversed: bool,
}

/// #779: Allocation target for multi-goal auto-allocation.
/// Each entry specifies a goal and the percentage of the deposit to allocate.
#[derive(Clone, Debug)]
#[contracttype]
pub struct AllocationGoal {
    /// Target goal ID
    pub goal_id: u64,
    /// Percentage of the total deposit to allocate (1-100, must sum to 100 across all entries)
    pub percentage: u32,
}

/// #779: Request for multi-goal auto-allocation.
#[derive(Clone, Debug)]
#[contracttype]
pub struct AutoAllocationRequest {
    /// The user making the deposit
    pub user: Address,
    /// The total amount to split across goals
    pub total_amount: i128,
    /// The target goals and their allocation percentages
    pub allocations: Vec<AllocationGoal>,
    /// Idempotency token to prevent double-allocation
    pub idempotency_token: Bytes,
}

/// #779: Result of a multi-goal auto-allocation.
#[derive(Clone, Debug)]
#[contracttype]
pub struct AutoAllocationResult {
    /// Whether the allocation was fully successful
    pub success: bool,
    /// Number of goals allocated to
    pub goals_allocated: u32,
    /// Number of goals that failed allocation
    pub goals_failed: u32,
    /// Total amount actually distributed
    pub total_distributed: i128,
    /// Individual contribution IDs per goal (goal_id -> contrib_id)
    pub contribution_ids: Vec<u64>,
}

/// Storage keys for contract state.
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    /// Admin address
    Admin,
    /// Last created batch ID
    LastBatchId,
    /// Last created goal ID
    LastGoalId,
    /// Stored goal by goal_id
    Goal(u64),
    /// User's goals (user address -> Vec<goal_id>)
    UserGoals(Address),
    /// Total goals created lifetime
    TotalGoalsCreated,
    /// Total batches processed lifetime
    TotalBatchesProcessed,
    /// Last created milestone ID
    LastMilestoneId,
    /// Stored milestone by milestone_id
    Milestone(u64),
    /// Goal's milestones (goal_id -> Vec<milestone_id>)
    GoalMilestones(u64),
    /// Goal's milestone percentages triggered (goal_id -> Vec<u32>)
    GoalMilestonesPercent(u64),
    /// Goal prerequisite relationships (goal_id -> Vec<goal_id>)
    GoalPrereqs(u64),
    /// Total milestones achieved lifetime
    TotalMilestonesAchieved,
    /// Ledger sequence at which a goal was automatically closed
    GoalClosedAt(u64),
    /// Maps (user, goal_name) -> goal_id for duplicate detection
    GoalByName(Address, Symbol),
    /// Snapshots for a goal (goal_id -> Vec<GoalSnapshot>)
    GoalSnapshots(u64),
    /// Contribution record keyed by (goal_id, contribution sequential index)
    Contribution(u64, u64),
    /// Last contribution index per goal
    LastContribId(u64),
    /// Idempotency token keyed by (goal owner, goal_id, token) -> contribution_id
    ContributionIdempotency(Address, u64, Bytes),
    /// Default alert thresholds applied to goals that do not override them.
    DefaultDeadlineAlertThresholds,
    /// Per-goal override for deadline alert thresholds.
    GoalDeadlineAlertThresholds(u64),
    /// Tracks which alert thresholds have already fired for a goal.
    GoalDeadlineAlertSent(u64, u64),
    /// Certificate for completed goal (goal_id -> GoalCertificate)
    Certificate(u64),
    /// Penalty contract address for early-withdrawal fee calculation
    PenaltyContract,
    /// #779: Beneficiary address for a goal (goal_id -> beneficiary Address)
    GoalBeneficiary(u64),
    /// #779: Auto-allocation idempotency key (user, token)
    AutoAllocationIdempotency(Address, Bytes),
}

/// Error codes for goal validation and creation.
pub mod ErrorCode {
    /// Milestone not yet achieved (progress too low)
    pub const MILESTONE_NOT_YET_ACHIEVED: u32 = 10;
    /// Invalid goal amount (too low or negative)
    pub const INVALID_AMOUNT: u32 = 0;
    /// Invalid deadline (in the past or too far in future)
    pub const INVALID_DEADLINE: u32 = 1;
    /// Invalid initial contribution (negative or exceeds target)
    pub const INVALID_INITIAL_CONTRIBUTION: u32 = 2;
    /// Goal name is empty or invalid
    pub const INVALID_GOAL_NAME: u32 = 3;
    /// User address is invalid
    pub const INVALID_USER_ADDRESS: u32 = 4;
    /// Goal does not exist
    pub const GOAL_NOT_FOUND: u32 = 5;
    /// Invalid milestone percentage (not 1-100)
    pub const INVALID_MILESTONE_PERCENTAGE: u32 = 6;
    /// Goal is not active
    pub const GOAL_NOT_ACTIVE: u32 = 7;
    /// User is not the goal owner
    pub const UNAUTHORIZED_USER: u32 = 8;
    /// Goal has already achieved this milestone
    pub const MILESTONE_ALREADY_ACHIEVED: u32 = 9;
    /// Goal is closed (target met) and no longer accepts contributions
    pub const GOAL_CLOSED: u32 = 11;
    /// Contribution amount is invalid (zero or negative)
    pub const INVALID_CONTRIBUTION_AMOUNT: u32 = 12;
    /// Duplicate goal name for the same user
    pub const DUPLICATE_GOAL_NAME: u32 = 11;
    /// Goal is locked; withdrawals not yet allowed
    pub const GOAL_LOCKED: u32 = 12;
    /// Withdrawal amount exceeds current balance
    pub const INSUFFICIENT_BALANCE: u32 = 13;
    /// Invalid withdrawal or contribution amount
    pub const INVALID_WITHDRAW_AMOUNT: u32 = 14;
    /// Deadline alert threshold configuration is invalid
    pub const INVALID_ALERT_THRESHOLD: u32 = 15;
    /// Contribution retry re-used an existing idempotency token
    pub const DUPLICATE_CONTRIBUTION_REQUEST: u32 = 16;
    /// #779: Beneficiary transfer rejected — caller is not the goal owner
    pub const BENEFICIARY_TRANSFER_UNAUTHORIZED: u32 = 17;
    /// #779: Auto-allocation percentages do not sum to 100
    pub const ALLOCATION_PERCENTAGES_INVALID: u32 = 18;
    /// #779: Auto-allocation token already used (duplicate)
    pub const DUPLICATE_ALLOCATION_REQUEST: u32 = 19;
}

/// Events emitted by the savings goals contract.
pub struct GoalEvents;

impl GoalEvents {
    /// Event emitted when batch goal creation starts.
    pub fn batch_started(env: &Env, batch_id: u64, request_count: u32) {
        let topics = (symbol_short!("batch"), symbol_short!("started"));
        env.events().publish(topics, (batch_id, request_count));
    }

    /// Event emitted when a goal is successfully created.
    pub fn goal_created(env: &Env, batch_id: u64, goal: &SavingsGoal) {
        let topics = (symbol_short!("goal"), symbol_short!("created"), batch_id);
        env.events().publish(
            topics,
            (goal.goal_id, goal.user.clone(), goal.target_amount),
        );
    }

    /// Event emitted when goal creation fails.
    pub fn goal_creation_failed(env: &Env, batch_id: u64, user: &Address, error_code: u32) {
        let topics = (symbol_short!("goal"), symbol_short!("failed"), batch_id);
        env.events().publish(topics, (user.clone(), error_code));
    }

    /// Event emitted when batch goal creation completes.
    pub fn batch_completed(
        env: &Env,
        batch_id: u64,
        successful: u32,
        failed: u32,
        total_amount: i128,
    ) {
        let topics = (symbol_short!("batch"), symbol_short!("completed"), batch_id);
        env.events()
            .publish(topics, (successful, failed, total_amount));
    }

    /// Event emitted for high-value goals (>= 10,000 XLM).
    pub fn high_value_goal(env: &Env, batch_id: u64, goal_id: u64, amount: i128) {
        let topics = (symbol_short!("goal"), symbol_short!("highval"), batch_id);
        env.events().publish(topics, (goal_id, amount));
    }

    /// Event emitted when batch milestone achievement starts.
    pub fn milestone_batch_started(env: &Env, batch_id: u64, request_count: u32) {
        let topics = (symbol_short!("milestone"), symbol_short!("start"));
        env.events().publish(topics, (batch_id, request_count));
    }

    /// Event emitted when a milestone is successfully achieved.
    pub fn milestone_achieved(env: &Env, batch_id: u64, milestone: &MilestoneAchievement) {
        let topics = (
            symbol_short!("milestone"),
            symbol_short!("achieved"),
            batch_id,
        );
        env.events().publish(
            topics,
            (
                milestone.milestone_id,
                milestone.goal_id,
                milestone.milestone_percentage,
            ),
        );
    }

    /// Event emitted when a milestone percentage is achieved automatically.
    pub fn milestone_achieved_percent(env: &Env, goal_id: u64, milestone_percent: u32) {
        let topics = (symbol_short!("milestone"), symbol_short!("auto"), goal_id);
        env.events().publish(topics, (goal_id, milestone_percent));
    }

    /// Event emitted when a contribution is made to a goal.
    pub fn goal_contributed(
        env: &Env,
        goal_id: u64,
        user: &Address,
        amount: i128,
        new_total: i128,
    ) {
        let topics = (symbol_short!("goal"), symbol_short!("contrib"), goal_id);
        env.events()
            .publish(topics, (user.clone(), amount, new_total));
    }

    /// Event emitted when a withdrawal is rejected because the goal is locked.
    pub fn goal_withdraw_locked(env: &Env, goal_id: u64, user: &Address, unlock_at: u64) {
        let topics = (symbol_short!("goal"), symbol_short!("wd_lock"), goal_id);
        env.events().publish(topics, (user.clone(), unlock_at));
    }

    /// Event emitted when funds are withdrawn from a goal.
    pub fn goal_withdrawn(env: &Env, goal_id: u64, user: &Address, amount: i128, remaining: i128) {
        let topics = (symbol_short!("goal"), symbol_short!("withdraw"), goal_id);
        env.events()
            .publish(topics, (user.clone(), amount, remaining));
    }

    /// Event emitted when milestone achievement fails.
    pub fn milestone_achievement_failed(env: &Env, batch_id: u64, goal_id: u64, error_code: u32) {
        let topics = (
            symbol_short!("milestone"),
            symbol_short!("failed"),
            batch_id,
        );
        env.events().publish(topics, (goal_id, error_code));
    }

    /// Event emitted when batch milestone achievement completes.
    pub fn milestone_batch_completed(
        env: &Env,
        batch_id: u64,
        successful: u32,
        failed: u32,
        total_percentage: u32,
    ) {
        let topics = (symbol_short!("milestone"), symbol_short!("done"));
        env.events()
            .publish(topics, (batch_id, successful, failed, total_percentage));
    }

    /// Event emitted when a goal is automatically closed because the target amount was reached.
    pub fn goal_closed(
        env: &Env,
        goal_id: u64,
        user: &Address,
        final_amount: i128,
        closed_at: u64,
    ) {
        let topics = (symbol_short!("goal"), symbol_short!("closed"), goal_id);
        env.events()
            .publish(topics, (goal_id, user.clone(), final_amount, closed_at));
    }
    /// Event emitted when a savings goal target is reached (completed).
    pub fn goal_completed(env: &Env, goal_id: u64, user: &Address, target_amount: i128) {
        let topics = (
            symbol_short!("goal"),
            symbol_short!("completed"),
            goal_id,
            user.clone(),
        );
        env.events().publish(topics, target_amount);
    }

    /// Event emitted when a partial withdrawal is made from a goal.
    pub fn partial_withdrawal(
        env: &Env,
        goal_id: u64,
        user: &Address,
        amount: i128,
        remaining: i128,
    ) {
        let topics = (symbol_short!("goal"), symbol_short!("withdraw"));
        env.events()
            .publish(topics, (goal_id, user.clone(), amount, remaining));
    }

    /// Event emitted when a goal is renamed.
    pub fn goal_renamed(env: &Env, goal_id: u64, old_name: &Symbol, new_name: &Symbol) {
        let topics = (symbol_short!("goal"), symbol_short!("renamed"), goal_id);
        env.events()
            .publish(topics, (old_name.clone(), new_name.clone()));
    }

    /// Event emitted when a snapshot is successfully captured.
    pub fn goal_snapshot_recorded(env: &Env, goal_id: u64, amount: i128, timestamp: u64) {
        let topics = (symbol_short!("goal"), symbol_short!("snapshot"), goal_id);
        env.events().publish(topics, (goal_id, amount, timestamp));
    }

    /// Event emitted when deadline alert thresholds are configured for a goal.
    pub fn deadline_alert_thresholds_updated(
        env: &Env,
        goal_id: u64,
        user: &Address,
        threshold_count: u32,
    ) {
        let topics = (symbol_short!("goal"), symbol_short!("al_cfg"), goal_id);
        env.events()
            .publish(topics, (user.clone(), threshold_count));
    }

    /// Event emitted when a goal is approaching its deadline while still underfunded.
    pub fn deadline_alert(
        env: &Env,
        goal_id: u64,
        user: &Address,
        threshold: u64,
        remaining_ledgers: u64,
        remaining_amount: i128,
    ) {
        let topics = (symbol_short!("goal"), symbol_short!("dl_alert"), goal_id);
        env.events().publish(
            topics,
            (user.clone(), threshold, remaining_ledgers, remaining_amount),
        );
    }

    /// #779: Event emitted when a goal's beneficiary is transferred.
    pub fn beneficiary_transferred(
        env: &Env,
        goal_id: u64,
        previous_owner: &Address,
        new_beneficiary: &Address,
    ) {
        let topics = (
            symbol_short!("goal"),
            symbol_short!("benef_xfer"),
            goal_id,
        );
        env.events()
            .publish(topics, (previous_owner.clone(), new_beneficiary.clone()));
    }

    /// #780: Event emitted when auto-allocation splits a deposit across goals.
    pub fn auto_allocation_executed(
        env: &Env,
        user: &Address,
        total_amount: i128,
        goal_count: u32,
    ) {
        let topics = (
            symbol_short!("goal"),
            symbol_short!("auto_alloc"),
        );
        env.events()
            .publish(topics, (user.clone(), total_amount, goal_count));
    }
}

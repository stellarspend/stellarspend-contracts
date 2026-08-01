//! Data types and events for batch spending limit operations.

use soroban_sdk::{contracttype, symbol_short, Address, Env, Symbol, Vec};

/// Maximum number of user-limit pairs in a single batch for optimization.
pub const MAX_BATCH_SIZE: u32 = 100;

/// Minimum monthly spending limit (0.1 XLM in stroops)
pub const MIN_SPENDING_LIMIT: i128 = 1_000_000;

/// Maximum monthly spending limit (10 million XLM in stroops)
pub const MAX_SPENDING_LIMIT: i128 = 100_000_000_000_000_000;

/// Minimum reset window duration. Window-based spending limits must reset at least
/// once every 24 hours to prevent permanent blocking after reaching a daily cap.
pub const MIN_RESET_WINDOW_SECONDS: u64 = 86_400;

/// Maximum reset window duration (30 days)
pub const MAX_RESET_WINDOW_SECONDS: u64 = 2_592_000;

/// Represents a spending limit update request for a user.
#[derive(Clone, Debug)]
#[contracttype]
pub struct SpendingLimitRequest {
    /// User's address
    pub user: Address,
    /// New monthly spending limit (in stroops)
    pub monthly_limit: i128,
    /// Reset window for the spending limit (in seconds)
    pub reset_window_seconds: u64,
    /// Optional category-specific limit (e.g., "food", "entertainment")
    pub category: Option<soroban_sdk::Symbol>,
}

/// Represents a user's spending limit configuration.
#[derive(Clone, Debug)]
#[contracttype]
pub struct SpendingLimit {
    /// User's address
    pub user: Address,
    /// Monthly spending limit (in stroops)
    pub monthly_limit: i128,
    /// Reset window for the spending limit (in seconds)
    pub reset_window_seconds: u64,
    /// Current month's spending (in stroops)
    pub current_spending: i128,
    /// Optional category
    pub category: Option<soroban_sdk::Symbol>,
    /// Last update timestamp
    pub updated_at: u64,
    /// Whether the limit is active
    pub is_active: bool,
}

/// Result of processing a single limit update.
#[derive(Clone, Debug)]
#[contracttype]
pub enum LimitUpdateResult {
    Success(SpendingLimit),
    Failure(Address, u32), // user address, error code
}

/// Aggregated metrics for a batch of limit updates.
#[derive(Clone, Debug)]
#[contracttype]
pub struct BatchLimitMetrics {
    /// Total number of update requests
    pub total_requests: u32,
    /// Number of successful updates
    pub successful_updates: u32,
    /// Number of failed updates
    pub failed_updates: u32,
    /// Total limits value across all updates
    pub total_limits_value: i128,
    /// Average limit amount
    pub avg_limit_amount: i128,
    /// Batch processing timestamp
    pub processed_at: u64,
}

/// Result of batch limit updates.
#[derive(Clone, Debug)]
#[contracttype]
pub struct BatchLimitResult {
    /// Batch ID
    pub batch_id: u64,
    /// Total number of requests
    pub total_requests: u32,
    /// Number of successful updates
    pub successful: u32,
    /// Number of failed updates
    pub failed: u32,
    /// Individual update results
    pub results: Vec<LimitUpdateResult>,
    /// Aggregated metrics
    pub metrics: BatchLimitMetrics,
}

/// Storage keys for contract state.
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    /// Admin address
    Admin,
    /// Last created batch ID
    LastBatchId,
    /// Stored spending limit by user address
    SpendingLimit(Address),
    /// Total limits updated lifetime
    TotalLimitsUpdated,
    /// Total batches processed lifetime
    TotalBatchesProcessed,
    /// Per-user spending for a given logical time window identifier.
    WindowSpending(Address, u64),
    /// Per-user monthly spending for a given logical month identifier.
    MonthlySpending(Address, u64),
    /// Whitelisted destination addresses
    Whitelist(Address),
}

/// Error codes for spending limit validation and updates.
pub mod ErrorCode {
    /// Invalid limit amount (too low, too high, or negative)
    pub const INVALID_LIMIT: u32 = 0;
    /// User address is invalid
    pub const INVALID_USER_ADDRESS: u32 = 1;
    /// Category name is invalid
    pub const INVALID_CATEGORY: u32 = 2;
    /// Limit already exists and cannot be overwritten
    pub const LIMIT_ALREADY_EXISTS: u32 = 3;
}

/// Events emitted by the spending limits contract.
pub struct LimitEvents;

impl LimitEvents {
    /// Event emitted when batch limit update starts.
    pub fn batch_started(env: &Env, batch_id: u64, request_count: u32) {
        let topics = (symbol_short!("batch"), symbol_short!("started"));
        env.events().publish(topics, (batch_id, request_count));
    }

    /// Event emitted when a limit is successfully updated.
    pub fn limit_updated(env: &Env, batch_id: u64, limit: &SpendingLimit) {
        let topics = (symbol_short!("limit"), symbol_short!("updated"), batch_id);
        env.events()
            .publish(topics, (limit.user.clone(), limit.monthly_limit));
    }

    /// Event emitted when limit update fails.
    pub fn limit_update_failed(env: &Env, batch_id: u64, user: &Address, error_code: u32) {
        let topics = (symbol_short!("limit"), symbol_short!("failed"), batch_id);
        env.events().publish(topics, (user.clone(), error_code));
    }

    /// Event emitted when batch limit update completes.
    pub fn batch_completed(
        env: &Env,
        batch_id: u64,
        successful: u32,
        failed: u32,
        total_limits: i128,
    ) {
        let topics = (symbol_short!("batch"), symbol_short!("completed"), batch_id);
        env.events()
            .publish(topics, (successful, failed, total_limits));
    }

    /// Event emitted for high-value limits (>= 1,000,000 XLM).
    pub fn high_value_limit(env: &Env, batch_id: u64, user: &Address, amount: i128) {
        let topics = (symbol_short!("limit"), symbol_short!("highval"), batch_id);
        env.events().publish(topics, (user.clone(), amount));
    }

    /// Event emitted when a spend attempt exceeds either the daily or monthly limit.
    pub fn limit_exceeded(
        env: &Env,
        user: &Address,
        attempted_amount: i128,
        remaining_daily: i128,
        remaining_monthly: i128,
    ) {
        let topics = (symbol_short!("limit"), symbol_short!("exceeded"));
        env.events().publish(
            topics,
            (
                user.clone(),
                attempted_amount,
                remaining_daily,
                remaining_monthly,
            ),
        );
    }
}
>>>>>>> 067107d (fix(contracts): fix CI compilation errors across batch-transfer, spending-limits, multi-currency-wallet, and batch-rewards)

/// Maximum number of user-limit pairs in a single batch for optimization.
/// Maximum number of requests in a single batch for optimization.
pub const MAX_BATCH_SIZE: u32 = 100;

/// Minimum spending limit (1 XLM in stroops).
pub const MIN_SPENDING_LIMIT: i128 = 10_000_000;

/// Maximum spending limit (1 billion XLM in stroops).
pub const MAX_SPENDING_LIMIT: i128 = 1_000_000_000_000_000_000;

/// Minimum reset window in seconds (1 hour).
pub const MIN_RESET_WINDOW_SECONDS: u64 = 3_600;

/// Maximum reset window in seconds (90 days).
pub const MAX_RESET_WINDOW_SECONDS: u64 = 7_776_000;

/// Escalation levels for spending limit enforcement.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum EscalationLevel {
    /// Small spend — automatic approval
    Small,
    /// Medium spend — logged but automatically approved
    Medium,
    /// Large spend — requires admin approval
    Large,
}

/// Strategies for spending limit adjustment.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum LimitStrategy {
    /// Fixed monthly limit
    Static,
    /// Limit increases automatically based on usage
    Adaptive,
}

/// Configuration for spending escalation rules.
#[derive(Clone, Debug)]
#[contracttype]
pub struct EscalationConfig {
    /// Threshold for small-to-medium escalation (in stroops)
    pub small_threshold: i128,
    /// Threshold for medium-to-large escalation (in stroops)
    pub medium_threshold: i128,
    /// Whether escalation rules are enabled
    pub enabled: bool,
}

/// Represents a spending limit request for a user.
#[derive(Clone, Debug)]
#[contracttype]
pub struct SpendingLimitRequest {
    /// User's address
    pub user: Address,
    /// Monthly spending limit amount (in stroops)
    pub monthly_limit: i128,
    /// New daily spending limit (in stroops)
    pub daily_limit: i128,
    /// New hourly spending limit (in stroops)
    pub hourly_limit: i128,
    /// Reset window for the spending limit (in seconds)
    /// Reset window in seconds (e.g., 86400 for daily)
    pub reset_window_seconds: u64,
    /// Optional spending category
    pub category: Option<Symbol>,
    /// Adjustment strategy
    pub strategy: LimitStrategy,
}

/// Represents a configured spending limit for a user.
#[derive(Clone, Debug)]
#[contracttype]
pub struct SpendingLimit {
    /// User's address
    pub user: Address,
    /// Monthly spending limit amount (in stroops)
    pub monthly_limit: i128,

    /// Daily spending limit (in stroops)
    pub daily_limit: i128,
    /// Hourly spending limit (in stroops)
    pub hourly_limit: i128,

    /// Reset window in seconds
    pub reset_window_seconds: u64,
    /// Current spending tracked in this period
    pub current_spending: i128,
    /// Optional category for the limit
    pub category: Option<Symbol>,
    /// When the limit was last updated (ledger timestamp)
    pub updated_at: u64,
    /// Whether the limit is active
    pub is_active: bool,
    /// Adjustment strategy
    pub strategy: LimitStrategy,
}

/// Consolidated instance-storage configuration for the spending limits contract.
#[derive(Clone)]
#[contracttype]
pub struct LimitsConfig {
    pub admin: Address,
    pub last_batch_id: u64,
    pub total_limits_updated: u64,
    pub total_batches_processed: u64,
}

/// Result of processing a single limit update.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum LimitUpdateResult {
    Success(SpendingLimit),
    Failure(Address, u32), // user address, error code
}

/// Aggregated metrics for a batch of limit updates.
#[derive(Clone, Debug)]
#[contracttype]
pub struct BatchLimitMetrics {
    /// Total number of limit update requests
    pub total_requests: u32,
    /// Number of successful updates
    pub successful_updates: u32,
    /// Number of failed updates
    pub failed_updates: u32,
    /// Total value of all limits
    pub total_limits_value: i128,
    /// Average limit amount
    pub avg_limit_amount: i128,
    /// Batch processing timestamp
    pub processed_at: u64,
}

/// Result of batch limit updates.
#[derive(Clone, Debug)]
#[contracttype]
pub struct BatchLimitResult {
    /// Batch ID
    pub batch_id: u64,
    /// Total number of requests
    pub total_requests: u32,
    /// Number of successful updates
    pub successful: u32,
    /// Number of failed updates
    pub failed: u32,
    /// Individual update results
    pub results: Vec<LimitUpdateResult>,
    /// Aggregated metrics
    pub metrics: BatchLimitMetrics,
}

/// Represents a spending limit exception granted to a user for a specific approved category.
#[derive(Clone, Debug)]
#[contracttype]
pub struct ExceptionRule {
    /// User address granted the exception
    pub user: Address,
    /// The approved category for which spending limits are bypassed
    pub category: Symbol,
    /// Ledger sequence when the exception was created
    pub created_at: u64,
    /// Whether the exception is currently active
    pub is_active: bool,
}

/// Storage keys for contract state.
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    /// Consolidated limits configuration.
    LimitsConfig,
    /// Stored spending limit by user address.
    SpendingLimit(Address),
    /// Windowed spending tracking (user, window_id).
    WindowSpending(Address, u64),
    /// Monthly spending tracking (user, month_id).
    MonthlySpending(Address, u64),
    /// Per-user hourly spending for a given logical hour identifier.
    HourlySpending(Address, u64),
    /// Per-user daily spending for a given logical day identifier.
    DailySpending(Address, u64),
    /// Exception rule for a specific user+category pair
    ExceptionRule(Address, Symbol),
    /// Admin-approved categories eligible for exception rules
    ApprovedCategories,
    /// Escalation configuration.
    EscalationConfig,
}

/// Error codes for limit validation and enforcement.
pub mod error_code {
    /// Invalid limit amount (negative or zero)
    pub const INVALID_LIMIT: u32 = 0;
    /// Invalid user address
    pub const INVALID_USER_ADDRESS: u32 = 1;
    /// Category name is invalid
    pub const INVALID_CATEGORY: u32 = 2;
    /// Limit already exists and cannot be overwritten
    pub const LIMIT_ALREADY_EXISTS: u32 = 3;
    /// Exception rule not found for user+category pair
    pub const EXCEPTION_NOT_FOUND: u32 = 4;
    /// Category is not in the approved categories list
    pub const CATEGORY_NOT_APPROVED: u32 = 5;
    /// Exception rule already exists for this user+category pair
    pub const EXCEPTION_ALREADY_EXISTS: u32 = 6;
    /// Invalid reset window
    pub const INVALID_RESET_WINDOW: u32 = 2;
    /// Limit not found
    pub const LIMIT_NOT_FOUND: u32 = 3;
}

/// Event helpers for the spending limits contract.
pub struct LimitEvents;

impl LimitEvents {
    pub fn batch_started(env: &Env, batch_id: u64, count: u32) {
        env.events().publish(
            (symbol_short!("limit"), symbol_short!("batch_st")),
            (batch_id, count),
        );
    }

    pub fn limit_updated(env: &Env, batch_id: u64, limit: &SpendingLimit) {
        env.events().publish(
            (symbol_short!("limit"), symbol_short!("updated")),
            (batch_id, limit.user.clone(), limit.monthly_limit),
        );
    }

    pub fn limit_adjusted(env: &Env, user: &Address, old_limit: i128, new_limit: i128) {
        env.events().publish(
            (symbol_short!("limit"), symbol_short!("adjusted")),
            (user.clone(), old_limit, new_limit),
        );
    }

    pub fn high_value_limit(env: &Env, batch_id: u64, user: &Address, amount: i128) {
        env.events().publish(
            (symbol_short!("limit"), symbol_short!("high_val")),
            (batch_id, user.clone(), amount),
        );
    }

    pub fn limit_update_failed(env: &Env, batch_id: u64, user: &Address, error_code: u32) {
        env.events().publish(
            (symbol_short!("limit"), symbol_short!("upd_fail")),
            (batch_id, user.clone(), error_code),
        );
    }

    pub fn batch_completed(env: &Env, batch_id: u64, success: u32, failed: u32, total: i128) {
        env.events().publish(
            (symbol_short!("limit"), symbol_short!("batch_cp")),
            (batch_id, success, failed, total),
        );
    }

    /// Event emitted when a spend attempt exceeds either the hourly, daily, or monthly limit.
    pub fn limit_exceeded(
        env: &Env,
        user: &Address,
        attempted_amount: i128,
        remaining_hourly: i128,
        remaining_daily: i128,
        remaining_monthly: i128,
    ) {
        env.events().publish(
            (symbol_short!("limit"), symbol_short!("exceeded")),
            (
                user.clone(),
                attempted_amount,
                remaining_hourly,
                remaining_daily,
                remaining_monthly,
            ),
        );
    }

    pub fn escalation_configured(env: &Env, small: i128, medium: i128, enabled: bool) {
        env.events().publish(
            (symbol_short!("limit"), symbol_short!("esc_cfg")),
            (small, medium, enabled),
        );
    }

    pub fn escalation_approved(env: &Env, admin: &Address, user: &Address, amount: i128) {
        env.events().publish(
            (symbol_short!("limit"), symbol_short!("esc_app")),
            (admin.clone(), user.clone(), amount),
        );
    }

    pub fn spending_limit_overridden(
        env: &Env,
        admin: &Address,
        user: &Address,
        old_limit: i128,
        new_limit: i128,
    ) {
        let topics = (symbol_short!("limit"), symbol_short!("override"));

        env.events()
            .publish(topics, (admin.clone(), user.clone(), old_limit, new_limit));
    }

    /// Event emitted when an exception rule is granted to a user for a category.
    pub fn exception_added(env: &Env, user: &Address, category: &Symbol) {
        let topics = (symbol_short!("exception"), symbol_short!("added"));
        env.events()
            .publish(topics, (user.clone(), category.clone()));
    }

    /// Event emitted when an exception rule is removed for a user+category pair.
    pub fn exception_removed(env: &Env, user: &Address, category: &Symbol) {
        let topics = (symbol_short!("exception"), symbol_short!("removed"));
        env.events()
            .publish(topics, (user.clone(), category.clone()));
    }

    /// Event emitted when a transaction bypasses spending limits via an active exception rule.
    pub fn exception_bypassed(env: &Env, user: &Address, amount: i128, category: &Symbol) {
        let topics = (symbol_short!("exception"), symbol_short!("bypass"));
        env.events()
            .publish(topics, (user.clone(), amount, category.clone()));
    }

    /// Event emitted when an approved category is added to the exception allow-list.
    pub fn approved_category_added(env: &Env, category: &Symbol) {
        let topics = (symbol_short!("category"), symbol_short!("approved"));
        env.events().publish(topics, category.clone());
    }

    /// Event emitted when an approved category is removed from the exception allow-list.
    pub fn approved_category_removed(env: &Env, category: &Symbol) {
        let topics = (symbol_short!("category"), symbol_short!("removed"));
        env.events().publish(topics, category.clone());
    }
}

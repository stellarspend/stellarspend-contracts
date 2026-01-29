//! Data types and events for batch spending limit operations.

use soroban_sdk::{contracttype, symbol_short, Address, Env, Vec};

/// Maximum number of user-limit pairs in a single batch for optimization.
pub const MAX_BATCH_SIZE: u32 = 100;

/// Minimum monthly spending limit (0.1 XLM in stroops)
pub const MIN_SPENDING_LIMIT: i128 = 1_000_000;

/// Maximum monthly spending limit (10 million XLM in stroops)
pub const MAX_SPENDING_LIMIT: i128 = 100_000_000_000_000_000;

/// Represents a spending limit update request for a user.
#[derive(Clone, Debug)]
#[contracttype]
pub struct SpendingLimitRequest {
    /// User's address
    pub user: Address,
    /// New monthly spending limit (in stroops)
    pub monthly_limit: i128,
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
}

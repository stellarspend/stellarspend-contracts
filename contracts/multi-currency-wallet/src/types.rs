//! Data types and events for batch multi-currency wallet operations.

use soroban_sdk::{contracttype, symbol_short, Address, Env, Symbol, Vec};

/// Maximum number of balance updates in a single batch for optimization.
pub const MAX_BATCH_SIZE: u32 = 100;

/// Minimum balance value (preventing dust)
pub const MIN_BALANCE: i128 = 1;

/// Maximum balance value (preventing overflow)
pub const MAX_BALANCE: i128 = i128::MAX;

/// Represents a balance update request for a user in a specific currency.
#[derive(Clone, Debug)]
#[contracttype]
pub struct BalanceUpdateRequest {
    /// User's address
    pub user: Address,
    /// Currency identifier (e.g., "USDC", "XLM", "EURC")
    pub currency: Symbol,
    /// New balance amount (in smallest unit)
    pub amount: i128,
    /// Update type: "set", "add", or "subtract"
    pub operation: Symbol,
}

/// Represents a user's balance in a specific currency.
#[derive(Clone, Debug)]
#[contracttype]
pub struct CurrencyBalance {
    /// User's address
    pub user: Address,
    /// Currency identifier
    pub currency: Symbol,
    /// Current balance amount
    pub balance: i128,
    /// Last update timestamp
    pub updated_at: u64,
}

/// Result of processing a single balance update.
#[derive(Clone, Debug)]
#[contracttype]
pub enum BalanceUpdateResult {
    Success(CurrencyBalance),
    Failure(Address, Symbol, u32), // user address, currency, error code
}

/// Aggregated metrics for a batch of balance updates.
#[derive(Clone, Debug)]
#[contracttype]
pub struct BatchBalanceMetrics {
    /// Total number of update requests
    pub total_requests: u32,
    /// Number of successful updates
    pub successful_updates: u32,
    /// Number of failed updates
    pub failed_updates: u32,
    /// Number of unique users affected
    pub unique_users: u32,
    /// Number of unique currencies updated
    pub unique_currencies: u32,
    /// Batch processing timestamp
    pub processed_at: u64,
}

/// Result of batch balance updates.
#[derive(Clone, Debug)]
#[contracttype]
pub struct BatchBalanceResult {
    /// Batch ID
    pub batch_id: u64,
    /// Total number of requests
    pub total_requests: u32,
    /// Number of successful updates
    pub successful: u32,
    /// Number of failed updates
    pub failed: u32,
    /// Individual update results
    pub results: Vec<BalanceUpdateResult>,
    /// Aggregated metrics
    pub metrics: BatchBalanceMetrics,
}

/// Storage keys for contract state.
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    /// Admin address
    Admin,
    /// Last created batch ID
    LastBatchId,
    /// Balance for user and currency: (user_address, currency)
    Balance(Address, Symbol),
    /// Total balances updated lifetime
    TotalBalancesUpdated,
    /// Total batches processed lifetime
    TotalBatchesProcessed,
}

/// Error codes for balance update validation.
pub mod ErrorCode {
    /// Invalid balance amount (negative or exceeds max)
    pub const INVALID_AMOUNT: u32 = 0;
    /// User address is invalid
    pub const INVALID_USER_ADDRESS: u32 = 1;
    /// Currency identifier is invalid or empty
    pub const INVALID_CURRENCY: u32 = 2;
    /// Invalid operation type
    pub const INVALID_OPERATION: u32 = 3;
    /// Insufficient balance for subtract operation
    pub const INSUFFICIENT_BALANCE: u32 = 4;
    /// Arithmetic overflow
    pub const ARITHMETIC_OVERFLOW: u32 = 5;
}

/// Events emitted by the multi-currency wallet contract.
pub struct WalletEvents;

impl WalletEvents {
    /// Event emitted when batch balance update starts.
    pub fn batch_started(env: &Env, batch_id: u64, request_count: u32) {
        let topics = (symbol_short!("batch"), symbol_short!("started"));
        env.events().publish(topics, (batch_id, request_count));
    }

    /// Event emitted when a balance is successfully updated.
    pub fn balance_updated(env: &Env, batch_id: u64, balance: &CurrencyBalance) {
        let topics = (
            symbol_short!("balance"),
            symbol_short!("updated"),
            batch_id,
        );
        env.events().publish(
            topics,
            (
                balance.user.clone(),
                balance.currency.clone(),
                balance.balance,
            ),
        );
    }

    /// Event emitted when balance update fails.
    pub fn balance_update_failed(
        env: &Env,
        batch_id: u64,
        user: &Address,
        currency: &Symbol,
        error_code: u32,
    ) {
        let topics = (symbol_short!("balance"), symbol_short!("failed"), batch_id);
        env.events()
            .publish(topics, (user.clone(), currency.clone(), error_code));
    }

    /// Event emitted when batch balance update completes.
    pub fn batch_completed(env: &Env, batch_id: u64, successful: u32, failed: u32) {
        let topics = (symbol_short!("batch"), symbol_short!("completed"), batch_id);
        env.events().publish(topics, (successful, failed));
    }

    /// Event emitted for large balance updates (>= 1,000,000 units).
    pub fn large_balance_update(
        env: &Env,
        batch_id: u64,
        user: &Address,
        currency: &Symbol,
        amount: i128,
    ) {
        let topics = (symbol_short!("balance"), symbol_short!("large"), batch_id);
        env.events()
            .publish(topics, (user.clone(), currency.clone(), amount));
    }
}

//! Data types and events for batch transaction analytics.

use soroban_sdk::{contracttype, symbol_short, Address, Env, Symbol, Vec};

/// Maximum number of transactions in a single batch for optimization.
pub const MAX_BATCH_SIZE: u32 = 100;

/// Represents a single transaction record for analytics.
#[derive(Clone, Debug)]
#[contracttype]
pub struct Transaction {
    /// Unique transaction identifier
    pub tx_id: u64,
    /// Sender address
    pub from: Address,
    /// Recipient address
    pub to: Address,
    /// Transaction amount in stroops
    pub amount: i128,
    /// Transaction timestamp (ledger sequence)
    pub timestamp: u64,
    /// Transaction category (e.g., "transfer", "budget", "savings")
    pub category: Symbol,
}

/// Aggregated metrics for a batch of transactions.
#[derive(Clone, Debug, Default)]
#[contracttype]
pub struct BatchMetrics {
    /// Total number of transactions in the batch
    pub tx_count: u32,
    /// Total volume of all transactions
    pub total_volume: i128,
    /// Average transaction amount
    pub avg_amount: i128,
    /// Minimum transaction amount
    pub min_amount: i128,
    /// Maximum transaction amount
    pub max_amount: i128,
    /// Number of unique senders
    pub unique_senders: u32,
    /// Number of unique recipients
    pub unique_recipients: u32,
    /// Batch processing timestamp
    pub processed_at: u64,
}

/// Category-specific metrics for analytics breakdown.
#[derive(Clone, Debug)]
#[contracttype]
pub struct CategoryMetrics {
    /// Category name
    pub category: Symbol,
    /// Number of transactions in this category
    pub tx_count: u32,
    /// Total volume for this category
    pub total_volume: i128,
    /// Percentage of total batch volume (basis points, 10000 = 100%)
    pub volume_percentage_bps: u32,
}

/// Represents a transaction to be bundled into a transaction group.
/// This extends the base Transaction with bundling-specific metadata.
#[derive(Clone, Debug)]
#[contracttype]
pub struct BundledTransaction {
    /// The transaction to bundle
    pub transaction: Transaction,
    /// Optional memo or metadata for the transaction
    pub memo: Option<Symbol>,
}

/// Result of validating a single transaction in a bundle.
#[derive(Clone, Debug)]
#[contracttype]
pub struct ValidationResult {
    /// Transaction ID that was validated
    pub tx_id: u64,
    /// Whether the transaction is valid
    pub is_valid: bool,
    /// Error message if validation failed (empty if valid)
    pub error: Symbol,
}

/// Result of bundling multiple transactions.
#[derive(Clone, Debug)]
#[contracttype]
pub struct BundleResult {
    /// Unique bundle ID
    pub bundle_id: u64,
    /// Total number of transactions in the bundle
    pub total_count: u32,
    /// Number of valid transactions
    pub valid_count: u32,
    /// Number of invalid transactions
    pub invalid_count: u32,
    /// Validation results for each transaction
    pub validation_results: Vec<ValidationResult>,
    /// Whether the bundle can be created (all transactions valid)
    pub can_bundle: bool,
    /// Total volume of valid transactions
    pub total_volume: i128,
    /// Bundle creation timestamp
    pub created_at: u64,
}

/// Storage keys for contract state.
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    /// Admin address
    Admin,
    /// Last processed batch ID
    LastBatchId,
    /// Stored metrics for a specific batch ID
    BatchMetrics(u64),
    /// Total transactions processed lifetime
    TotalTxProcessed,
    /// Last bundle ID
    LastBundleId,
    /// Stored bundle result for a specific bundle ID
    BundleResult(u64),
}

/// Events emitted by the analytics contract.
pub struct AnalyticsEvents;

impl AnalyticsEvents {
    /// Event emitted when a batch is processed.
    pub fn batch_processed(env: &Env, batch_id: u64, metrics: &BatchMetrics) {
        let topics = (symbol_short!("batch"), symbol_short!("processed"), batch_id);
        env.events().publish(topics, metrics.clone());
    }

    /// Event emitted for each category in a batch.
    pub fn category_analytics(env: &Env, batch_id: u64, category_metrics: &CategoryMetrics) {
        let topics = (symbol_short!("category"), batch_id, &category_metrics.category);
        env.events().publish(topics, category_metrics.clone());
    }

    /// Event emitted when analytics computation starts.
    pub fn analytics_started(env: &Env, batch_id: u64, tx_count: u32) {
        let topics = (symbol_short!("analytics"), symbol_short!("started"));
        env.events().publish(topics, (batch_id, tx_count));
    }

    /// Event emitted when analytics computation completes.
    pub fn analytics_completed(env: &Env, batch_id: u64, processing_cost: u64) {
        let topics = (symbol_short!("analytics"), symbol_short!("complete"));
        env.events().publish(topics, (batch_id, processing_cost));
    }

    /// Event emitted for high-value transaction alerts.
    pub fn high_value_alert(env: &Env, batch_id: u64, tx_id: u64, amount: i128) {
        let topics = (symbol_short!("alert"), symbol_short!("highval"));
        env.events().publish(topics, (batch_id, tx_id, amount));
    }

    /// Event emitted when a transaction bundle is created.
    pub fn bundle_created(env: &Env, bundle_id: u64, result: &BundleResult) {
        let topics = (symbol_short!("bundle"), symbol_short!("created"), bundle_id);
        env.events().publish(topics, result.clone());
    }

    /// Event emitted when a transaction in a bundle is validated.
    pub fn transaction_validated(env: &Env, bundle_id: u64, validation_result: &ValidationResult) {
        let topics = (
            symbol_short!("bundle"),
            symbol_short!("validated"),
            bundle_id,
        );
        env.events().publish(topics, validation_result.clone());
    }

    /// Event emitted when bundling starts.
    pub fn bundling_started(env: &Env, bundle_id: u64, tx_count: u32) {
        let topics = (symbol_short!("bundle"), symbol_short!("started"));
        env.events().publish(topics, (bundle_id, tx_count));
    }

    /// Event emitted when bundling completes.
    pub fn bundling_completed(env: &Env, bundle_id: u64, can_bundle: bool) {
        let topics = (symbol_short!("bundle"), symbol_short!("completed"));
        env.events().publish(topics, (bundle_id, can_bundle));
    }

    /// Event emitted when a transaction fails validation in a bundle.
    pub fn transaction_validation_failed(
        env: &Env,
        bundle_id: u64,
        tx_id: u64,
        error: &Symbol,
    ) {
        let topics = (symbol_short!("bundle"), symbol_short!("failed"), bundle_id);
        env.events().publish(topics, (tx_id, error.clone()));
    }
}

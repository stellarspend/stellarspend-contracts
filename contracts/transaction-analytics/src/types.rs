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

/// Represents a single audit log entry.
#[derive(Clone, Debug)]
#[contracttype]
pub struct AuditLog {
    /// Address of the actor who performed the operation
    pub actor: Address,
    /// The operation performed (e.g., "init", "config_update")
    pub operation: Symbol,
    /// Timestamp of the operation
    pub timestamp: u64,
    /// Status of the operation (e.g., "success", "failure")
    pub status: Symbol,
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
    /// Total fees collected for the batch
    pub total_fees: i128,
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
    /// Total fees for this category
    pub total_fees: i128,
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

/// Input for submitting a rating for a transaction.
#[derive(Clone, Debug)]
#[contracttype]
pub struct RatingInput {
    pub tx_id: u64,
    pub score: u32,
}

/// Status of a submitted rating.
#[derive(Clone, Debug)]
#[contracttype]
pub enum RatingStatus {
    Success,
    InvalidScore,
    UnknownTransaction,
}

/// Result of a submitted rating.
#[derive(Clone, Debug)]
#[contracttype]
pub struct RatingResult {
    pub tx_id: u64,
    pub score: u32,
    pub status: RatingStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum TransactionStatus {
    Pending,
    Completed,
    Failed,
    Refunded,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct TransactionStatusUpdate {
    pub tx_id: u64,
    pub status: TransactionStatus,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct StatusUpdateResult {
    pub tx_id: u64,
    pub is_valid: bool,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct BatchStatusUpdateResult {
    pub total_requests: u32,
    pub successful: u32,
    pub failed: u32,
    pub results: Vec<StatusUpdateResult>,
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
    /// Stored audit log for a specific index
    AuditLog(u64),
    /// Total number of audit logs stored
    TotalAuditLogs,

    /// Last bundle ID
    LastBundleId,
    /// Stored bundle result for a specific bundle ID
    BundleResult(u64),
     #Batch-refund

    /// Last refund batch ID
    LastRefundBatchId,
    /// Stored refund metrics for a specific batch ID
    RefundBatchMetrics(u64),
    /// Total refund amount processed lifetime
    TotalRefundAmount,
    /// Set of refunded transaction IDs (for duplicate prevention)
    RefundedTransactions,
    /// Known transaction IDs (for validation)
    KnownTransaction(u64),
}

/// Status indicating refund eligibility for a transaction.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub enum RefundStatus {
    /// Transaction is eligible for refund (failed or canceled)
    Eligible,
    /// Transaction has already been refunded
    AlreadyRefunded,
    /// Transaction is still pending/processing
    Pending,
    /// Transaction was successful, not eligible for refund
    NotEligible,
    /// Transaction ID not found
    NotFound,
      /// Marker for a known transaction ID
    KnownTransaction(u64),
    /// Stored rating per (tx_id, user)
    Rating(u64, Address),
    /// Stored status per transaction ID
    TransactionStatus(u64),
}

/// Request structure for a single transaction refund.
#[derive(Clone, Debug)]
#[contracttype]
pub struct RefundRequest {
    /// Transaction ID to refund
    pub tx_id: u64,
    /// Reason for refund (optional)
    pub reason: Option<Symbol>,
}

/// Result of a refund attempt for a single transaction.
#[derive(Clone, Debug)]
#[contracttype]
pub struct RefundResult {
    /// Transaction ID that was attempted to refund
    pub tx_id: u64,
    /// Whether the refund was successful
    pub success: bool,
    /// Refund eligibility status
    pub status: RefundStatus,
    /// Amount refunded (if successful)
    pub amount_refunded: i128,
    /// Error message if refund failed
    pub error_message: Option<Symbol>,
}

/// Aggregated metrics for a batch of refunds.
#[derive(Clone, Debug, Default)]
#[contracttype]
pub struct RefundBatchMetrics {
    /// Total number of refund requests
    pub request_count: u32,
    /// Number of successful refunds
    pub successful_refunds: u32,
    /// Number of failed refunds
    pub failed_refunds: u32,
    /// Total amount refunded
    pub total_refunded_amount: i128,
    /// Average refund amount
    pub avg_refund_amount: i128,
    /// Timestamp when batch was processed
    pub processed_at: u64,
main
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
        let topics = (
            symbol_short!("category"),
            batch_id,
        );
        env.events().publish(topics, (category_metrics.category.clone(), category_metrics.clone()));
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

    /// Event emitted when an audit log is created.
    pub fn audit_logged(env: &Env, actor: &Address, operation: &Symbol, status: &Symbol) {
        let topics = (symbol_short!("audit"), symbol_short!("log"));
        env.events().publish(topics, (actor.clone(), operation.clone(), status.clone()));
    }

    /// Event emitted when a rating is submitted.
    pub fn rating_submitted(
        env: &Env,
        user: &Address,
        tx_id: u64,
        score: u32,
        status: RatingStatus,
    ) {
        let topics = (symbol_short!("rating"), symbol_short!("submit"), user);
        env.events().publish(topics, (tx_id, score, status));
    }

    pub fn transaction_status_updated(
        env: &Env,
        tx_id: u64,
        previous_status: Option<TransactionStatus>,
        new_status: TransactionStatus,
    ) {
        let topics = (symbol_short!("status"), symbol_short!("updated"));
        env.events().publish(topics, (tx_id, previous_status, new_status));
    }

    pub fn transaction_status_update_failed(env: &Env, tx_id: u64) {
        let topics = (symbol_short!("status"), symbol_short!("failed"));
        env.events().publish(topics, tx_id);
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
    pub fn transaction_validation_failed(env: &Env, bundle_id: u64, tx_id: u64, error: &Symbol) {
        let topics = (symbol_short!("bundle"), symbol_short!("failed"));
        env.events().publish(topics, (bundle_id, tx_id, error.clone()));
    }

    /// Event emitted when a refund batch processing starts.
    pub fn refund_batch_started(env: &Env, batch_id: u64, request_count: u32) {
        let topics = (symbol_short!("refund"), symbol_short!("started"));
        env.events().publish(topics, (batch_id, request_count));
    }

    /// Event emitted for each individual refund result.
    pub fn refund_processed(env: &Env, batch_id: u64, refund_result: &RefundResult) {
        let topics = (symbol_short!("refund"), symbol_short!("processed"), batch_id);
        env.events().publish(topics, refund_result.clone());
    }

    /// Event emitted when a refund batch completes.
    pub fn refund_batch_completed(env: &Env, batch_id: u64, metrics: &RefundBatchMetrics) {
        let topics = (symbol_short!("refund"), symbol_short!("completed"), batch_id);
        env.events().publish(topics, metrics.clone());
    }

    /// Event emitted for refund errors or warnings.
    pub fn refund_error(env: &Env, batch_id: u64, tx_id: u64, error_msg: Symbol) {
        let topics = (symbol_short!("refund"), symbol_short!("error"));
        env.events().publish(topics, (batch_id, tx_id, error_msg));
    }
}

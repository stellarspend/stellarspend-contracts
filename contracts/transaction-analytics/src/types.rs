//! Data types and events for batch transaction analytics.

use soroban_sdk::{contracttype, symbol_short, Address, Env, Symbol};

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
    /// Last refund batch ID
    LastRefundBatchId,
    /// Stored refund metrics for a specific batch ID
    RefundBatchMetrics(u64),
    /// Total refund amount processed lifetime
    TotalRefundAmount,
    /// Set of refunded transaction IDs (for duplicate prevention)
    RefundedTransactions,
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

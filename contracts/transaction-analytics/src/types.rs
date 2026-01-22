//! Data types and events for batch transaction analytics.

use soroban_sdk::{contracttype, symbol_short, Address, Env, Map, Symbol};

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

/// User budget data for AI recommendations.
#[derive(Clone, Debug)]
#[contracttype]
pub struct UserBudgetData {
    /// User address
    pub user: Address,
    /// Monthly income
    pub monthly_income: i128,
    /// Current spending by category (category -> amount)
    pub spending_by_category: Map<Symbol, i128>,
    /// Savings goal (optional)
    pub savings_goal: Option<i128>,
    /// Risk tolerance level (1-5, where 1 is conservative, 5 is aggressive)
    pub risk_tolerance: u8,
}

/// AI-generated budget recommendation for a user.
#[derive(Clone, Debug)]
#[contracttype]
pub struct BudgetRecommendation {
    /// User address
    pub user: Address,
    /// Recommended spending limit per category (category -> limit)
    pub recommended_limits: Map<Symbol, i128>,
    /// Recommended savings amount
    pub recommended_savings: i128,
    /// Recommended emergency fund amount
    pub recommended_emergency_fund: i128,
    /// Confidence score (0-100, representing recommendation confidence)
    pub confidence_score: u8,
    /// Recommendation timestamp
    pub generated_at: u64,
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
    /// Last recommendation batch ID
    LastRecommendationBatchId,
    /// Stored recommendations for a specific batch ID
    RecommendationBatch(u64),
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

    /// Event emitted when budget recommendation batch processing starts.
    pub fn recommendations_started(env: &Env, batch_id: u64, user_count: u32) {
        let topics = (symbol_short!("recommend"), symbol_short!("started"));
        env.events().publish(topics, (batch_id, user_count));
    }

    /// Event emitted for each generated budget recommendation.
    pub fn recommendation_generated(env: &Env, batch_id: u64, recommendation: &BudgetRecommendation) {
        let topics = (symbol_short!("recommend"), symbol_short!("generated"), batch_id);
        env.events().publish(topics, recommendation.clone());
    }

    /// Event emitted when budget recommendation batch processing completes.
    pub fn recommendations_completed(env: &Env, batch_id: u64, recommendation_count: u32) {
        let topics = (symbol_short!("recommend"), symbol_short!("complete"));
        env.events().publish(topics, (batch_id, recommendation_count));
    }
}

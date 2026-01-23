//! Data types and events for batch budget recommendations.

use soroban_sdk::{contracttype, symbol_short, Address, Env, Symbol, Vec};

/// Maximum number of users in a single batch for optimization.
pub const MAX_BATCH_SIZE: u32 = 100;

/// Represents a user's financial profile for budget recommendations.
#[derive(Clone, Debug)]
#[contracttype]
pub struct UserProfile {
    /// Unique user identifier
    pub user_id: u64,
    /// User's address
    pub address: Address,
    /// Monthly income in stroops
    pub monthly_income: i128,
    /// Current monthly expenses in stroops
    pub monthly_expenses: i128,
    /// Current savings balance in stroops
    pub savings_balance: i128,
    /// Spending category preferences (comma-separated categories)
    pub spending_categories: Symbol,
    /// Risk tolerance level (1-5, where 1 is conservative, 5 is aggressive)
    pub risk_tolerance: u32,
}

/// Represents a budget recommendation for a user.
#[derive(Clone, Debug)]
#[contracttype]
pub struct BudgetRecommendation {
    /// User ID this recommendation is for
    pub user_id: u64,
    /// Recommended monthly budget allocation in stroops
    pub recommended_budget: i128,
    /// Recommended savings amount per month in stroops
    pub recommended_savings: i128,
    /// Recommended spending limit per month in stroops
    pub recommended_spending_limit: i128,
    /// Recommended emergency fund target in stroops
    pub emergency_fund_target: i128,
    /// Confidence score (0-100, where 100 is highest confidence)
    pub confidence_score: u32,
    /// Recommendation category (e.g., "conservative", "moderate", "aggressive")
    pub recommendation_type: Symbol,
    /// Additional recommendation notes
    pub notes: Symbol,
}

/// Aggregated metrics for a batch of recommendations.
#[derive(Clone, Debug, Default)]
#[contracttype]
pub struct BatchRecommendationMetrics {
    /// Total number of users processed
    pub user_count: u32,
    /// Number of successful recommendations
    pub successful_recommendations: u32,
    /// Number of failed recommendations
    pub failed_recommendations: u32,
    /// Total recommended budget across all users
    pub total_recommended_budget: i128,
    /// Total recommended savings across all users
    pub total_recommended_savings: i128,
    /// Average confidence score
    pub avg_confidence_score: u32,
    /// Batch processing timestamp
    pub processed_at: u64,
}

/// Result of processing a single user's recommendation.
#[derive(Clone, Debug)]
#[contracttype]
pub enum RecommendationResult {
    Success(BudgetRecommendation),
    Failure(u64, Symbol), // user_id, error message
}

/// Result of batch recommendation processing.
#[derive(Clone, Debug)]
#[contracttype]
pub struct BatchRecommendationResult {
    /// Batch ID
    pub batch_id: u64,
    /// Total number of users in batch
    pub total_users: u32,
    /// Number of successful recommendations
    pub successful: u32,
    /// Number of failed recommendations
    pub failed: u32,
    /// Individual recommendation results
    pub results: Vec<RecommendationResult>,
    /// Aggregated metrics
    pub metrics: BatchRecommendationMetrics,
}

/// Storage keys for contract state.
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    /// Admin address
    Admin,
    /// Last processed batch ID
    LastBatchId,
    /// Stored recommendations for a specific batch ID
    BatchRecommendations(u64),
    /// Total users processed lifetime
    TotalUsersProcessed,
    /// Total recommendations generated lifetime
    TotalRecommendationsGenerated,
}

/// Events emitted by the budget recommendations contract.
pub struct RecommendationEvents;

impl RecommendationEvents {
    /// Event emitted when batch recommendation processing starts.
    pub fn batch_started(env: &Env, batch_id: u64, user_count: u32) {
        let topics = (symbol_short!("batch"), symbol_short!("started"));
        env.events().publish(topics, (batch_id, user_count));
    }

    /// Event emitted when a recommendation is generated for a user.
    pub fn recommendation_generated(
        env: &Env,
        batch_id: u64,
        user_id: u64,
        recommendation: &BudgetRecommendation,
    ) {
        let topics = (
            symbol_short!("recommend"),
            symbol_short!("generated"),
            batch_id,
        );
        env.events().publish(topics, (user_id, recommendation.clone()));
    }

    /// Event emitted when a recommendation fails for a user.
    pub fn recommendation_failed(env: &Env, batch_id: u64, user_id: u64, error: &Symbol) {
        let topics = (symbol_short!("recommend"), symbol_short!("failed"), batch_id);
        env.events().publish(topics, (user_id, error.clone()));
    }

    /// Event emitted when batch recommendation processing completes.
    pub fn batch_completed(env: &Env, batch_id: u64, metrics: &BatchRecommendationMetrics) {
        let topics = (symbol_short!("batch"), symbol_short!("completed"), batch_id);
        env.events().publish(topics, metrics.clone());
    }

    /// Event emitted for high-confidence recommendations.
    pub fn high_confidence_recommendation(
        env: &Env,
        batch_id: u64,
        user_id: u64,
        confidence_score: u32,
    ) {
        let topics = (symbol_short!("recommend"), symbol_short!("highconf"), batch_id);
        env.events().publish(topics, (user_id, confidence_score));
    }
}

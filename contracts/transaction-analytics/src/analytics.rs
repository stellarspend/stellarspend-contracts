//! Core batch analytics computation logic.
//!
//! This module provides optimized batch processing for transaction analytics,
//! following Soroban best practices:
//! - Minimizes storage operations by accumulating changes locally
//! - Uses fixed-size structures where possible
//! - Batches computations to reduce gas costs

use soroban_sdk::{Address, Env, Map, Symbol, Vec};

use crate::types::{
    BatchMetrics, BudgetRecommendation, CategoryMetrics, Transaction, UserBudgetData,
    MAX_BATCH_SIZE,
};

/// Computes aggregated metrics for a batch of transactions.
///
/// Optimized to perform a single pass over the transaction data,
/// computing all metrics in O(n) time complexity.
pub fn compute_batch_metrics(
    env: &Env,
    transactions: &Vec<Transaction>,
    processed_at: u64,
) -> BatchMetrics {
    let tx_count = transactions.len();

    if tx_count == 0 {
        return BatchMetrics {
            tx_count: 0,
            total_volume: 0,
            avg_amount: 0,
            min_amount: 0,
            max_amount: 0,
            unique_senders: 0,
            unique_recipients: 0,
            processed_at,
        };
    }

    // Accumulate metrics in a single pass (optimization: avoid multiple iterations)
    let mut total_volume: i128 = 0;
    let mut min_amount: i128 = i128::MAX;
    let mut max_amount: i128 = i128::MIN;

    // Use maps to track unique addresses (more efficient than vectors for lookups)
    let mut senders: Map<Address, bool> = Map::new(env);
    let mut recipients: Map<Address, bool> = Map::new(env);

    for tx in transactions.iter() {
        // Accumulate volume
        total_volume = total_volume.checked_add(tx.amount).unwrap_or(i128::MAX);

        // Track min/max
        if tx.amount < min_amount {
            min_amount = tx.amount;
        }
        if tx.amount > max_amount {
            max_amount = tx.amount;
        }

        // Track unique addresses
        if !senders.contains_key(tx.from.clone()) {
            senders.set(tx.from.clone(), true);
        }
        if !recipients.contains_key(tx.to.clone()) {
            recipients.set(tx.to.clone(), true);
        }
    }

    // Calculate average (avoiding division by zero)
    let avg_amount = total_volume / (tx_count as i128);

    BatchMetrics {
        tx_count,
        total_volume,
        avg_amount,
        min_amount,
        max_amount,
        unique_senders: senders.len(),
        unique_recipients: recipients.len(),
        processed_at,
    }
}

/// Computes category-specific metrics for analytics breakdown.
///
/// Groups transactions by category and computes volume distribution.
pub fn compute_category_metrics(
    env: &Env,
    transactions: &Vec<Transaction>,
    total_volume: i128,
) -> Vec<CategoryMetrics> {
    let mut category_map: Map<Symbol, (u32, i128)> = Map::new(env);

    // Single pass to aggregate by category
    for tx in transactions.iter() {
        let current = category_map.get(tx.category.clone()).unwrap_or((0, 0));
        category_map.set(
            tx.category.clone(),
            (current.0 + 1, current.1.checked_add(tx.amount).unwrap_or(i128::MAX)),
        );
    }

    // Convert to CategoryMetrics vector
    let mut result: Vec<CategoryMetrics> = Vec::new(env);

    for (category, (tx_count, volume)) in category_map.iter() {
        // Calculate percentage in basis points (10000 = 100%)
        let volume_percentage_bps = if total_volume > 0 {
            ((volume * 10000) / total_volume) as u32
        } else {
            0
        };

        result.push_back(CategoryMetrics {
            category,
            tx_count,
            total_volume: volume,
            volume_percentage_bps,
        });
    }

    result
}

/// Identifies high-value transactions that exceed a threshold.
///
/// Returns a vector of (tx_id, amount) tuples for transactions above the threshold.
pub fn find_high_value_transactions(
    env: &Env,
    transactions: &Vec<Transaction>,
    threshold: i128,
) -> Vec<(u64, i128)> {
    let mut high_value: Vec<(u64, i128)> = Vec::new(env);

    for tx in transactions.iter() {
        if tx.amount >= threshold {
            high_value.push_back((tx.tx_id, tx.amount));
        }
    }

    high_value
}

/// Validates a batch of transactions before processing.
///
/// Returns Ok(()) if valid, or an error message if invalid.
pub fn validate_batch(transactions: &Vec<Transaction>) -> Result<(), &'static str> {
    let count = transactions.len();

    if count == 0 {
        return Err("Batch cannot be empty");
    }

    if count > MAX_BATCH_SIZE {
        return Err("Batch exceeds maximum size");
    }

    // Validate individual transactions
    for tx in transactions.iter() {
        if tx.amount < 0 {
            return Err("Transaction amount cannot be negative");
        }
    }

    Ok(())
}

/// Computes a simple checksum for batch integrity verification.
pub fn compute_batch_checksum(transactions: &Vec<Transaction>) -> u64 {
    let mut checksum: u64 = 0;

    for tx in transactions.iter() {
        // XOR tx_id and lower bits of amount for simple integrity check
        checksum ^= tx.tx_id;
        checksum ^= (tx.amount & 0xFFFFFFFF) as u64;
    }

    checksum
}

/// Validates user budget data before processing recommendations.
///
/// Returns Ok(()) if valid, or an error message if invalid.
pub fn validate_user_budget_data(user_data: &UserBudgetData) -> Result<(), &'static str> {
    // Validate monthly income
    if user_data.monthly_income <= 0 {
        return Err("Monthly income must be positive");
    }

    // Validate risk tolerance (1-5 scale)
    if user_data.risk_tolerance < 1 || user_data.risk_tolerance > 5 {
        return Err("Risk tolerance must be between 1 and 5");
    }

    // Validate spending amounts are non-negative
    for (_, amount) in user_data.spending_by_category.iter() {
        if amount < 0 {
            return Err("Spending amounts cannot be negative");
        }
    }

    // Validate savings goal if provided
    if let Some(goal) = user_data.savings_goal {
        if goal < 0 {
            return Err("Savings goal cannot be negative");
        }
    }

    Ok(())
}

/// Generates AI-driven budget recommendation for a single user.
///
/// This function simulates AI recommendations using rule-based logic optimized for on-chain execution.
/// The algorithm considers:
/// - Income level
/// - Current spending patterns
/// - Risk tolerance
/// - Savings goals
///
/// Returns a BudgetRecommendation with category limits and savings recommendations.
pub fn generate_budget_recommendation(
    env: &Env,
    user_data: &UserBudgetData,
    current_ledger: u64,
) -> BudgetRecommendation {
    let monthly_income = user_data.monthly_income;

    // Calculate total current spending
    let mut total_spending: i128 = 0;
    for (_, amount) in user_data.spending_by_category.iter() {
        total_spending = total_spending.checked_add(amount).unwrap_or(i128::MAX);
    }

    // AI Recommendation Algorithm:
    // 1. Allocate 50% for needs, 30% for wants, 20% for savings (50/30/20 rule)
    // 2. Adjust based on risk tolerance (higher risk = more aggressive savings)
    // 3. Consider current spending patterns to suggest realistic limits

    // Base allocation percentages (in basis points, 10000 = 100%)
    let mut needs_percentage_bps = 5000u32; // 50%
    let mut wants_percentage_bps = 3000u32; // 30%
    let mut savings_percentage_bps = 2000u32; // 20%

    // Adjust based on risk tolerance
    // Higher risk tolerance (4-5) = more aggressive savings
    // Lower risk tolerance (1-2) = more conservative, higher emergency fund
    let risk_factor = user_data.risk_tolerance as i128;
    if risk_factor >= 4 {
        // Aggressive: 40% savings, 40% needs, 20% wants
        needs_percentage_bps = 4000;
        wants_percentage_bps = 2000;
        savings_percentage_bps = 4000;
    } else if risk_factor <= 2 {
        // Conservative: 30% savings, 50% needs, 20% wants
        needs_percentage_bps = 5000;
        wants_percentage_bps = 2000;
        savings_percentage_bps = 3000;
    }

    // Calculate recommended amounts
    let recommended_needs = (monthly_income * needs_percentage_bps as i128) / 10000;
    let recommended_wants = (monthly_income * wants_percentage_bps as i128) / 10000;
    let recommended_savings = (monthly_income * savings_percentage_bps as i128) / 10000;

    // Emergency fund recommendation: 3-6 months of expenses based on risk tolerance
    // Conservative (1-2): 6 months, Moderate (3): 4 months, Aggressive (4-5): 3 months
    let emergency_fund_months = if risk_factor <= 2 {
        6
    } else if risk_factor == 3 {
        4
    } else {
        3
    };
    let monthly_expenses = recommended_needs + recommended_wants;
    let recommended_emergency_fund = monthly_expenses * emergency_fund_months as i128;

    // Generate category-specific recommendations
    // Distribute recommended limits based on current spending patterns
    let mut recommended_limits: Map<Symbol, i128> = Map::new(env);

    if total_spending > 0 {
        // Allocate based on current spending proportions
        // Distribute the total budget (needs + wants) proportionally across categories
        let total_budget = recommended_needs + recommended_wants;
        
        for (category, current_spending) in user_data.spending_by_category.iter() {
            // Calculate proportion of total spending for this category
            let proportion = (current_spending * 10000) / total_spending;
            
            // Allocate proportional budget to this category
            let category_budget = (total_budget * proportion) / 10000;
            
            // Add 10% buffer for flexibility
            let limit = (category_budget * 110) / 100;
            recommended_limits.set(category, limit);
        }
    } else {
        // If no spending history, use default allocations
        // This would be enhanced with category defaults in production
        let default_category = Symbol::new(env, "general");
        recommended_limits.set(default_category, recommended_needs + recommended_wants);
    }

    // Adjust savings goal if user provided one
    let final_savings = if let Some(user_goal) = user_data.savings_goal {
        // Use user goal if it's reasonable (not more than 50% of income)
        if user_goal <= monthly_income / 2 {
            user_goal
        } else {
            recommended_savings
        }
    } else {
        recommended_savings
    };

    // Calculate confidence score based on data quality
    // More categories and spending history = higher confidence
    let category_count = user_data.spending_by_category.len();
    let confidence_score = if category_count >= 5 && total_spending > 0 {
        90u8 // High confidence with good data
    } else if category_count >= 3 {
        75u8 // Medium confidence
    } else {
        60u8 // Lower confidence with limited data
    };

    BudgetRecommendation {
        user: user_data.user.clone(),
        recommended_limits,
        recommended_savings: final_savings,
        recommended_emergency_fund,
        confidence_score,
        generated_at: current_ledger,
    }
}

/// Generates budget recommendations for multiple users in a batch.
///
/// Optimized to process all users in a single pass, computing recommendations
/// efficiently while minimizing storage operations.
pub fn generate_batch_recommendations(
    env: &Env,
    users: &Vec<UserBudgetData>,
    current_ledger: u64,
) -> Vec<BudgetRecommendation> {
    let mut recommendations: Vec<BudgetRecommendation> = Vec::new(env);

    // Single pass through all users
    for user_data in users.iter() {
        let recommendation = generate_budget_recommendation(env, &user_data, current_ledger);
        recommendations.push_back(recommendation);
    }

    recommendations
}

/// Validates a batch of user budget data.
///
/// Returns Ok(()) if all users are valid, or an error message if any are invalid.
pub fn validate_batch_budget_data(users: &Vec<UserBudgetData>) -> Result<(), &'static str> {
    let count = users.len();

    if count == 0 {
        return Err("Batch cannot be empty");
    }

    if count > MAX_BATCH_SIZE {
        return Err("Batch exceeds maximum size");
    }

    // Validate each user's data
    for user_data in users.iter() {
        if let Err(e) = validate_user_budget_data(&user_data) {
            return Err(e);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    fn create_test_transaction(env: &Env, tx_id: u64, amount: i128, category: &str) -> Transaction {
        Transaction {
            tx_id,
            from: Address::generate(env),
            to: Address::generate(env),
            amount,
            timestamp: 12345,
            category: Symbol::new(env, category),
        }
    }

    #[test]
    fn test_compute_batch_metrics_single_tx() {
        let env = Env::default();
        let mut transactions: Vec<Transaction> = Vec::new(&env);
        transactions.push_back(create_test_transaction(&env, 1, 1000, "transfer"));

        let metrics = compute_batch_metrics(&env, &transactions, 100);

        assert_eq!(metrics.tx_count, 1);
        assert_eq!(metrics.total_volume, 1000);
        assert_eq!(metrics.avg_amount, 1000);
        assert_eq!(metrics.min_amount, 1000);
        assert_eq!(metrics.max_amount, 1000);
        assert_eq!(metrics.unique_senders, 1);
        assert_eq!(metrics.unique_recipients, 1);
    }

    #[test]
    fn test_compute_batch_metrics_multiple_tx() {
        let env = Env::default();
        let mut transactions: Vec<Transaction> = Vec::new(&env);
        transactions.push_back(create_test_transaction(&env, 1, 100, "transfer"));
        transactions.push_back(create_test_transaction(&env, 2, 200, "transfer"));
        transactions.push_back(create_test_transaction(&env, 3, 300, "budget"));

        let metrics = compute_batch_metrics(&env, &transactions, 100);

        assert_eq!(metrics.tx_count, 3);
        assert_eq!(metrics.total_volume, 600);
        assert_eq!(metrics.avg_amount, 200);
        assert_eq!(metrics.min_amount, 100);
        assert_eq!(metrics.max_amount, 300);
    }

    #[test]
    fn test_compute_batch_metrics_empty() {
        let env = Env::default();
        let transactions: Vec<Transaction> = Vec::new(&env);

        let metrics = compute_batch_metrics(&env, &transactions, 100);

        assert_eq!(metrics.tx_count, 0);
        assert_eq!(metrics.total_volume, 0);
    }

    #[test]
    fn test_compute_category_metrics() {
        let env = Env::default();
        let mut transactions: Vec<Transaction> = Vec::new(&env);
        transactions.push_back(create_test_transaction(&env, 1, 500, "transfer"));
        transactions.push_back(create_test_transaction(&env, 2, 300, "transfer"));
        transactions.push_back(create_test_transaction(&env, 3, 200, "budget"));

        let category_metrics = compute_category_metrics(&env, &transactions, 1000);

        assert_eq!(category_metrics.len(), 2);
    }

    #[test]
    fn test_find_high_value_transactions() {
        let env = Env::default();
        let mut transactions: Vec<Transaction> = Vec::new(&env);
        transactions.push_back(create_test_transaction(&env, 1, 100, "transfer"));
        transactions.push_back(create_test_transaction(&env, 2, 5000, "transfer"));
        transactions.push_back(create_test_transaction(&env, 3, 10000, "budget"));

        let high_value = find_high_value_transactions(&env, &transactions, 1000);

        assert_eq!(high_value.len(), 2);
        assert_eq!(high_value.get(0).unwrap(), (2, 5000));
        assert_eq!(high_value.get(1).unwrap(), (3, 10000));
    }

    #[test]
    fn test_validate_batch_valid() {
        let env = Env::default();
        let mut transactions: Vec<Transaction> = Vec::new(&env);
        transactions.push_back(create_test_transaction(&env, 1, 100, "transfer"));

        assert!(validate_batch(&transactions).is_ok());
    }

    #[test]
    fn test_validate_batch_empty() {
        let env = Env::default();
        let transactions: Vec<Transaction> = Vec::new(&env);

        assert_eq!(validate_batch(&transactions), Err("Batch cannot be empty"));
    }

    #[test]
    fn test_validate_batch_negative_amount() {
        let env = Env::default();
        let mut transactions: Vec<Transaction> = Vec::new(&env);
        transactions.push_back(create_test_transaction(&env, 1, -100, "transfer"));

        assert_eq!(
            validate_batch(&transactions),
            Err("Transaction amount cannot be negative")
        );
    }

    #[test]
    fn test_compute_batch_checksum() {
        let env = Env::default();
        let mut transactions: Vec<Transaction> = Vec::new(&env);
        transactions.push_back(create_test_transaction(&env, 1, 100, "transfer"));
        transactions.push_back(create_test_transaction(&env, 2, 200, "transfer"));

        let checksum1 = compute_batch_checksum(&transactions);
        let checksum2 = compute_batch_checksum(&transactions);

        // Same batch should produce same checksum
        assert_eq!(checksum1, checksum2);
    }

    #[test]
    fn test_validate_user_budget_data_valid() {
        let env = Env::default();
        let mut spending: Map<Symbol, i128> = Map::new(&env);
        spending.set(Symbol::new(&env, "food"), 500);
        spending.set(Symbol::new(&env, "transport"), 300);

        let user_data = UserBudgetData {
            user: Address::generate(&env),
            monthly_income: 5000,
            spending_by_category: spending,
            savings_goal: Some(1000),
            risk_tolerance: 3,
        };

        assert!(validate_user_budget_data(&user_data).is_ok());
    }

    #[test]
    fn test_validate_user_budget_data_invalid_income() {
        let env = Env::default();
        let spending: Map<Symbol, i128> = Map::new(&env);

        let user_data = UserBudgetData {
            user: Address::generate(&env),
            monthly_income: 0,
            spending_by_category: spending,
            savings_goal: None,
            risk_tolerance: 3,
        };

        assert_eq!(
            validate_user_budget_data(&user_data),
            Err("Monthly income must be positive")
        );
    }

    #[test]
    fn test_validate_user_budget_data_invalid_risk_tolerance() {
        let env = Env::default();
        let spending: Map<Symbol, i128> = Map::new(&env);

        let user_data = UserBudgetData {
            user: Address::generate(&env),
            monthly_income: 5000,
            spending_by_category: spending,
            savings_goal: None,
            risk_tolerance: 6, // Invalid: should be 1-5
        };

        assert_eq!(
            validate_user_budget_data(&user_data),
            Err("Risk tolerance must be between 1 and 5")
        );
    }

    #[test]
    fn test_generate_budget_recommendation() {
        let env = Env::default();
        let mut spending: Map<Symbol, i128> = Map::new(&env);
        spending.set(Symbol::new(&env, "food"), 1000);
        spending.set(Symbol::new(&env, "transport"), 500);

        let user_data = UserBudgetData {
            user: Address::generate(&env),
            monthly_income: 5000,
            spending_by_category: spending,
            savings_goal: Some(1000),
            risk_tolerance: 3,
        };

        let recommendation = generate_budget_recommendation(&env, &user_data, 100);

        assert_eq!(recommendation.user, user_data.user);
        assert!(recommendation.recommended_savings > 0);
        assert!(recommendation.recommended_emergency_fund > 0);
        assert!(recommendation.confidence_score >= 60 && recommendation.confidence_score <= 100);
        assert_eq!(recommendation.generated_at, 100);
    }

    #[test]
    fn test_generate_budget_recommendation_aggressive_risk() {
        let env = Env::default();
        let mut spending: Map<Symbol, i128> = Map::new(&env);
        spending.set(Symbol::new(&env, "food"), 1000);

        let user_data = UserBudgetData {
            user: Address::generate(&env),
            monthly_income: 5000,
            spending_by_category: spending,
            savings_goal: None,
            risk_tolerance: 5, // Aggressive
        };

        let recommendation = generate_budget_recommendation(&env, &user_data, 100);

        // Aggressive risk should have higher savings percentage (40%)
        // So recommended_savings should be around 2000 (40% of 5000)
        assert!(recommendation.recommended_savings >= 1500);
    }

    #[test]
    fn test_generate_budget_recommendation_conservative_risk() {
        let env = Env::default();
        let mut spending: Map<Symbol, i128> = Map::new(&env);
        spending.set(Symbol::new(&env, "food"), 1000);

        let user_data = UserBudgetData {
            user: Address::generate(&env),
            monthly_income: 5000,
            spending_by_category: spending,
            savings_goal: None,
            risk_tolerance: 1, // Conservative
        };

        let recommendation = generate_budget_recommendation(&env, &user_data, 100);

        // Conservative risk should have 6 months emergency fund
        // Emergency fund should be higher than aggressive
        assert!(recommendation.recommended_emergency_fund > 0);
    }

    #[test]
    fn test_generate_batch_recommendations() {
        let env = Env::default();
        let mut users: Vec<UserBudgetData> = Vec::new(&env);

        for i in 0..3 {
            let mut spending: Map<Symbol, i128> = Map::new(&env);
            spending.set(Symbol::new(&env, "food"), 500 + (i * 100) as i128);

            let user_data = UserBudgetData {
                user: Address::generate(&env),
                monthly_income: 3000 + (i * 1000) as i128,
                spending_by_category: spending,
                savings_goal: None,
                risk_tolerance: 3,
            };
            users.push_back(user_data);
        }

        let recommendations = generate_batch_recommendations(&env, &users, 100);

        assert_eq!(recommendations.len(), 3);
        for rec in recommendations.iter() {
            assert!(rec.recommended_savings > 0);
            assert!(rec.confidence_score >= 60);
        }
    }

    #[test]
    fn test_validate_batch_budget_data_valid() {
        let env = Env::default();
        let mut users: Vec<UserBudgetData> = Vec::new(&env);

        let mut spending: Map<Symbol, i128> = Map::new(&env);
        spending.set(Symbol::new(&env, "food"), 500);

        let user_data = UserBudgetData {
            user: Address::generate(&env),
            monthly_income: 5000,
            spending_by_category: spending,
            savings_goal: None,
            risk_tolerance: 3,
        };
        users.push_back(user_data);

        assert!(validate_batch_budget_data(&users).is_ok());
    }

    #[test]
    fn test_validate_batch_budget_data_empty() {
        let env = Env::default();
        let users: Vec<UserBudgetData> = Vec::new(&env);

        assert_eq!(
            validate_batch_budget_data(&users),
            Err("Batch cannot be empty")
        );
    }
}

//! Core AI-driven budget recommendation computation logic.
//!
//! This module provides optimized batch processing for budget recommendations,
//! following Soroban best practices:
//! - Minimizes storage operations by accumulating changes locally
//! - Uses fixed-size structures where possible
//! - Batches computations to reduce gas costs
//! - Implements rule-based AI recommendations (deterministic for blockchain)

use soroban_sdk::{Env, Symbol, Vec};

use crate::types::{
    BatchRecommendationMetrics, BudgetRecommendation, RecommendationResult, UserProfile,
};

/// Generates a budget recommendation for a single user.
///
/// Uses rule-based AI logic to generate recommendations based on:
/// - Income vs expenses ratio
/// - Savings balance
/// - Risk tolerance
/// - Spending patterns
///
/// This is deterministic and optimized for blockchain execution.
pub fn generate_recommendation(
    env: &Env,
    profile: &UserProfile,
) -> Result<BudgetRecommendation, Symbol> {
    // Calculate disposable income
    let disposable_income = profile.monthly_income
        .checked_sub(profile.monthly_expenses)
        .unwrap_or(0);

    // Determine recommendation type based on risk tolerance
    let recommendation_type = match profile.risk_tolerance {
        1 => Symbol::new(env, "conservative"),
        2 => Symbol::new(env, "moderate_conservative"),
        3 => Symbol::new(env, "moderate"),
        4 => Symbol::new(env, "moderate_aggressive"),
        5 => Symbol::new(env, "aggressive"),
        _ => Symbol::new(env, "moderate"),
    };

    // Calculate recommended savings percentage based on risk tolerance
    // Conservative (1): 30-40% savings
    // Moderate (3): 20-30% savings
    // Aggressive (5): 10-20% savings
    let savings_percentage = match profile.risk_tolerance {
        1 => 35, // 35% of disposable income
        2 => 30,
        3 => 25,
        4 => 20,
        5 => 15,
        _ => 25,
    };

    // Calculate recommended budget (remaining after savings)
    let recommended_savings = if disposable_income > 0 {
        (disposable_income * savings_percentage as i128) / 100
    } else {
        0
    };

    let recommended_budget = profile.monthly_expenses
        + (disposable_income - recommended_savings);

    // Calculate recommended spending limit (budget + small buffer)
    let buffer_percentage = 5; // 5% buffer
    let recommended_spending_limit = recommended_budget
        + (recommended_budget * buffer_percentage as i128) / 100;

    // Calculate emergency fund target (3-6 months of expenses based on risk tolerance)
    let emergency_fund_months = match profile.risk_tolerance {
        1 => 6, // Conservative: 6 months
        2 => 5,
        3 => 4, // Moderate: 4 months
        4 => 3,
        5 => 3, // Aggressive: 3 months
        _ => 4,
    };
    let emergency_fund_target = profile.monthly_expenses * emergency_fund_months as i128;

    // Calculate confidence score based on data quality
    let mut confidence_score = 80u32; // Base confidence

    // Increase confidence if user has positive disposable income
    if disposable_income > 0 {
        confidence_score += 10;
    }

    // Increase confidence if user has existing savings
    if profile.savings_balance > 0 {
        confidence_score += 5;
    }

    // Decrease confidence if expenses exceed income
    if profile.monthly_expenses > profile.monthly_income {
        confidence_score = confidence_score.saturating_sub(20);
    }

    // Cap confidence at 100
    confidence_score = confidence_score.min(100);

    // Generate recommendation notes
    let notes = if disposable_income < 0 {
        Symbol::new(env, "expenses_exceed_income_review_needed")
    } else if recommended_savings == 0 {
        Symbol::new(env, "minimal_savings_capacity")
    } else if profile.savings_balance < emergency_fund_target / 2 {
        Symbol::new(env, "build_emergency_fund_priority")
    } else {
        Symbol::new(env, "on_track_continue_current_strategy")
    };

    Ok(BudgetRecommendation {
        user_id: profile.user_id,
        recommended_budget,
        recommended_savings,
        recommended_spending_limit,
        emergency_fund_target,
        confidence_score,
        recommendation_type,
        notes,
    })
}

/// Generates batch recommendations for multiple users.
///
/// Optimized to perform a single pass over the user profiles,
/// computing all recommendations in O(n) time complexity.
pub fn generate_batch_recommendations(
    env: &Env,
    profiles: &Vec<UserProfile>,
    processed_at: u64,
) -> (Vec<RecommendationResult>, BatchRecommendationMetrics) {
    let user_count = profiles.len();
    let mut results: Vec<RecommendationResult> = Vec::new(env);
    let mut successful_count: u32 = 0;
    let mut failed_count: u32 = 0;
    let mut total_recommended_budget: i128 = 0;
    let mut total_recommended_savings: i128 = 0;
    let mut total_confidence: u64 = 0;

    // Process each user profile
    for profile in profiles.iter() {
        match generate_recommendation(env, &profile) {
            Ok(recommendation) => {
                // Accumulate metrics
                total_recommended_budget = total_recommended_budget
                    .checked_add(recommendation.recommended_budget)
                    .unwrap_or(i128::MAX);
                total_recommended_savings = total_recommended_savings
                    .checked_add(recommendation.recommended_savings)
                    .unwrap_or(i128::MAX);
                total_confidence += recommendation.confidence_score as u64;
                successful_count += 1;

                results.push_back(RecommendationResult::Success(recommendation));
            }
            Err(error) => {
                failed_count += 1;
                results.push_back(RecommendationResult::Failure(profile.user_id, error));
            }
        }
    }

    // Calculate average confidence score
    let avg_confidence_score = if successful_count > 0 {
        (total_confidence / successful_count as u64) as u32
    } else {
        0
    };

    let metrics = BatchRecommendationMetrics {
        user_count,
        successful_recommendations: successful_count,
        failed_recommendations: failed_count,
        total_recommended_budget,
        total_recommended_savings,
        avg_confidence_score,
        processed_at,
    };

    (results, metrics)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env, Symbol};

    fn create_test_profile(env: &Env, user_id: u64, income: i128, expenses: i128) -> UserProfile {
        UserProfile {
            user_id,
            address: Address::generate(env),
            monthly_income: income,
            monthly_expenses: expenses,
            savings_balance: 100000,
            spending_categories: Symbol::new(env, "food,transport"),
            risk_tolerance: 3,
        }
    }

    #[test]
    fn test_generate_recommendation_positive_income() {
        let env = Env::default();
        let profile = create_test_profile(&env, 1, 100000, 50000);

        let recommendation = generate_recommendation(&env, &profile).unwrap();

        assert_eq!(recommendation.user_id, 1);
        assert!(recommendation.recommended_budget > 0);
        assert!(recommendation.recommended_savings > 0);
        assert!(recommendation.confidence_score >= 80u32);
    }

    #[test]
    fn test_generate_recommendation_conservative_risk() {
        let env = Env::default();
        let mut profile = create_test_profile(&env, 1, 100000, 50000);
        profile.risk_tolerance = 1; // Conservative

        let recommendation = generate_recommendation(&env, &profile).unwrap();

        // Conservative should have higher savings percentage
        assert!(recommendation.recommended_savings > 0);
        assert_eq!(
            recommendation.recommendation_type,
            Symbol::new(&env, "conservative")
        );
    }

    #[test]
    fn test_generate_recommendation_aggressive_risk() {
        let env = Env::default();
        let mut profile = create_test_profile(&env, 1, 100000, 50000);
        profile.risk_tolerance = 5; // Aggressive

        let recommendation = generate_recommendation(&env, &profile).unwrap();

        assert_eq!(
            recommendation.recommendation_type,
            Symbol::new(&env, "aggressive")
        );
    }

    #[test]
    fn test_generate_batch_recommendations() {
        let env = Env::default();
        let mut profiles: Vec<UserProfile> = Vec::new(&env);
        profiles.push_back(create_test_profile(&env, 1, 100000, 50000));
        profiles.push_back(create_test_profile(&env, 2, 200000, 100000));

        let (results, metrics) = generate_batch_recommendations(&env, &profiles, 100);

        assert_eq!(results.len(), 2);
        assert_eq!(metrics.user_count, 2);
        assert_eq!(metrics.successful_recommendations, 2);
        assert_eq!(metrics.failed_recommendations, 0);
    }
}

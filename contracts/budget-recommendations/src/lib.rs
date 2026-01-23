//! # Budget Recommendations Contract
//!
//! A Soroban smart contract for generating AI-driven budget recommendations
//! for multiple users in batch operations.
//!
//! ## Features
//!
//! - **Batch Processing**: Efficiently process multiple users in a single call
//! - **AI-Driven Recommendations**: Rule-based AI logic for budget suggestions
//! - **Risk-Based Strategies**: Recommendations tailored to user risk tolerance
//! - **Event Emission**: Emit recommendation events for off-chain consumption
//! - **Optimized Computation**: Single-pass processing for O(n) complexity
//!
//! ## Optimization Strategies
//!
//! - Single-pass computation for O(n) complexity
//! - Minimized storage operations
//! - Efficient data structures
//! - Batched event emissions

#![no_std]

mod recommendations;
mod types;
mod validation;

use soroban_sdk::{contract, contractimpl, panic_with_error, Address, Env, Vec};

pub use crate::recommendations::{generate_batch_recommendations, generate_recommendation};
pub use crate::types::{
    BatchRecommendationMetrics, BatchRecommendationResult, BudgetRecommendation, DataKey,
    RecommendationEvents, RecommendationResult, UserProfile, MAX_BATCH_SIZE,
};
use crate::validation::validate_batch;

/// Error codes for the budget recommendations contract.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum BudgetRecommendationError {
    /// Contract not initialized
    NotInitialized = 1,
    /// Caller is not authorized
    Unauthorized = 2,
    /// Invalid batch data
    InvalidBatch = 3,
    /// Batch is empty
    EmptyBatch = 4,
    /// Batch exceeds maximum size
    BatchTooLarge = 5,
    /// Invalid user profile
    InvalidUserProfile = 6,
}

impl From<BudgetRecommendationError> for soroban_sdk::Error {
    fn from(e: BudgetRecommendationError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

#[contract]
pub struct BudgetRecommendationsContract;

#[contractimpl]
impl BudgetRecommendationsContract {
    /// Initializes the contract with an admin address.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - The admin address that can manage the contract
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Contract already initialized");
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::LastBatchId, &0u64);
        env.storage().instance().set(&DataKey::TotalUsersProcessed, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::TotalRecommendationsGenerated, &0u64);
    }

    /// Generates batch budget recommendations for multiple users.
    ///
    /// This is the main entry point for processing user profiles and generating
    /// AI-driven budget recommendations. It computes recommendations, emits events,
    /// and stores results.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `caller` - The address calling this function (must be admin)
    /// * `user_profiles` - Vector of user profiles to process
    ///
    /// # Returns
    /// * `BatchRecommendationResult` - Result containing recommendations and metrics
    ///
    /// # Events Emitted
    /// * `batch_started` - When processing begins
    /// * `recommendation_generated` - For each successful recommendation
    /// * `recommendation_failed` - For each failed recommendation
    /// * `high_confidence_recommendation` - For recommendations with high confidence
    /// * `batch_completed` - When processing completes
    pub fn generate_batch_recommendations(
        env: Env,
        caller: Address,
        user_profiles: Vec<UserProfile>,
    ) -> BatchRecommendationResult {
        // Verify authorization
        caller.require_auth();
        Self::require_admin(&env, &caller);

        // Validate batch
        let user_count = user_profiles.len();
        if user_count == 0 {
            panic_with_error!(&env, BudgetRecommendationError::EmptyBatch);
        }
        if user_count > MAX_BATCH_SIZE {
            panic_with_error!(&env, BudgetRecommendationError::BatchTooLarge);
        }

        // Validate batch of user profiles
        if let Err(_) = validate_batch(&user_profiles) {
            panic_with_error!(&env, BudgetRecommendationError::InvalidBatch);
        }

        // Get next batch ID
        let batch_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::LastBatchId)
            .unwrap_or(0)
            + 1;

        // Emit start event
        RecommendationEvents::batch_started(&env, batch_id, user_count);

        // Get current ledger timestamp
        let current_ledger = env.ledger().sequence() as u64;

        // Generate batch recommendations (single pass over data)
        let (results, metrics) = generate_batch_recommendations(&env, &user_profiles, current_ledger);

        // Emit events for each recommendation
        for result in results.iter() {
            match result {
                RecommendationResult::Success(recommendation) => {
                    RecommendationEvents::recommendation_generated(
                        &env,
                        batch_id,
                        recommendation.user_id,
                        &recommendation,
                    );

                    // Emit high confidence event if applicable
                    if recommendation.confidence_score >= 90 {
                        RecommendationEvents::high_confidence_recommendation(
                            &env,
                            batch_id,
                            recommendation.user_id,
                            recommendation.confidence_score,
                        );
                    }
                }
                RecommendationResult::Failure(user_id, error) => {
                    RecommendationEvents::recommendation_failed(&env, batch_id, user_id, &error);
                }
            }
        }

        // Update storage (batched at the end for efficiency)
        let total_processed: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalUsersProcessed)
            .unwrap_or(0);
        let total_recommendations: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalRecommendationsGenerated)
            .unwrap_or(0);

        env.storage().instance().set(&DataKey::LastBatchId, &batch_id);
        env.storage()
            .instance()
            .set(&DataKey::TotalUsersProcessed, &(total_processed + user_count as u64));
        env.storage().instance().set(
            &DataKey::TotalRecommendationsGenerated,
            &(total_recommendations + metrics.successful_recommendations as u64),
        );
        env.storage()
            .persistent()
            .set(&DataKey::BatchRecommendations(batch_id), &results);

        // Create batch result
        let batch_result = BatchRecommendationResult {
            batch_id,
            total_users: user_count,
            successful: metrics.successful_recommendations,
            failed: metrics.failed_recommendations,
            results: results.clone(),
            metrics: metrics.clone(),
        };

        // Emit completion event
        RecommendationEvents::batch_completed(&env, batch_id, &metrics);

        batch_result
    }

    /// Retrieves stored recommendations for a specific batch.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `batch_id` - The ID of the batch to retrieve
    ///
    /// # Returns
    /// * `Option<Vec<RecommendationResult>>` - The stored recommendations if found
    pub fn get_batch_recommendations(
        env: Env,
        batch_id: u64,
    ) -> Option<Vec<RecommendationResult>> {
        env.storage()
            .persistent()
            .get(&DataKey::BatchRecommendations(batch_id))
    }

    /// Generates a recommendation for a single user (view-only, no storage).
    ///
    /// Useful for simulating recommendations before committing.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `user_profile` - The user profile to generate recommendation for
    ///
    /// # Returns
    /// * `BudgetRecommendation` - The generated recommendation
    pub fn simulate_recommendation(
        env: Env,
        user_profile: UserProfile,
    ) -> Option<BudgetRecommendation> {
        match generate_recommendation(&env, &user_profile) {
            Ok(rec) => Some(rec),
            Err(_) => None,
        }
    }

    /// Returns the admin address.
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Contract not initialized")
    }

    /// Updates the admin address.
    pub fn set_admin(env: Env, current_admin: Address, new_admin: Address) {
        current_admin.require_auth();
        Self::require_admin(&env, &current_admin);

        env.storage().instance().set(&DataKey::Admin, &new_admin);
    }

    /// Returns the last processed batch ID.
    pub fn get_last_batch_id(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::LastBatchId)
            .unwrap_or(0)
    }

    /// Returns the total number of users processed.
    pub fn get_total_users_processed(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalUsersProcessed)
            .unwrap_or(0)
    }

    /// Returns the total number of recommendations generated.
    pub fn get_total_recommendations(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalRecommendationsGenerated)
            .unwrap_or(0)
    }

    // Internal helper to verify admin
    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Contract not initialized");

        if *caller != admin {
            panic_with_error!(env, BudgetRecommendationError::Unauthorized);
        }
    }
}

#[cfg(test)]
mod test;

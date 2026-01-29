//! # Spending Limits Contract
//!
//! A Soroban smart contract for managing batch spending limit updates
//! for multiple users simultaneously.
//!
//! ## Features
//!
//! - **Batch Processing**: Efficiently update spending limits for multiple users in a single call
//! - **Comprehensive Validation**: Validates limit amounts and user addresses
//! - **Event Emission**: Emits events for limit updates and batch processing
//! - **Error Handling**: Gracefully handles invalid inputs with detailed error codes
//! - **Optimized Storage**: Minimizes storage writes by batching operations
//! - **Partial Failure Support**: Invalid updates don't affect valid ones
//!
//! ## Optimization Strategies
//!
//! - Single-pass processing for O(n) complexity
//! - Minimized storage operations (batch writes at the end)
//! - Efficient data structures
//! - Batched event emissions

#![no_std]

mod types;
mod validation;

use soroban_sdk::{contract, contractimpl, panic_with_error, Address, Env, Vec};

pub use crate::types::{
    BatchLimitMetrics, BatchLimitResult, DataKey, ErrorCode, LimitEvents, LimitUpdateResult,
    SpendingLimit, SpendingLimitRequest, MAX_BATCH_SIZE,
};
use crate::validation::validate_limit_request;

/// Error codes for the spending limits contract.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum SpendingLimitError {
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
}

impl From<SpendingLimitError> for soroban_sdk::Error {
    fn from(e: SpendingLimitError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

#[contract]
pub struct SpendingLimitsContract;

#[contractimpl]
impl SpendingLimitsContract {
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
        env.storage()
            .instance()
            .set(&DataKey::TotalLimitsUpdated, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::TotalBatchesProcessed, &0u64);
    }

    /// Updates monthly spending limits for multiple users in a batch.
    ///
    /// This is the main entry point for batch limit updates. It validates all requests,
    /// updates limits, emits events, and handles partial failures gracefully.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `caller` - The address calling this function (must be admin)
    /// * `requests` - Vector of spending limit update requests
    ///
    /// # Returns
    /// * `BatchLimitResult` - Result containing updated limits and metrics
    ///
    /// # Events Emitted
    /// * `batch_started` - When processing begins
    /// * `limit_updated` - For each successful limit update
    /// * `limit_update_failed` - For each failed limit update
    /// * `high_value_limit` - For limits with high values
    /// * `batch_completed` - When processing completes
    ///
    /// # Errors
    /// * `EmptyBatch` - If no requests provided
    /// * `BatchTooLarge` - If batch exceeds maximum size
    /// * `Unauthorized` - If caller is not admin
    pub fn batch_update_spending_limits(
        env: Env,
        caller: Address,
        requests: Vec<SpendingLimitRequest>,
    ) -> BatchLimitResult {
        // Verify authorization
        caller.require_auth();
        Self::require_admin(&env, &caller);

        // Validate batch size
        let request_count = requests.len();
        if request_count == 0 {
            panic_with_error!(&env, SpendingLimitError::EmptyBatch);
        }
        if request_count > MAX_BATCH_SIZE {
            panic_with_error!(&env, SpendingLimitError::BatchTooLarge);
        }

        // Get batch ID and increment
        let batch_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::LastBatchId)
            .unwrap_or(0)
            + 1;

        // Emit batch started event
        LimitEvents::batch_started(&env, batch_id, request_count);

        // Get current ledger timestamp
        let current_ledger = env.ledger().sequence() as u64;

        // Initialize result tracking
        let mut results: Vec<LimitUpdateResult> = Vec::new(&env);
        let mut successful_count: u32 = 0;
        let mut failed_count: u32 = 0;
        let mut total_limits_value: i128 = 0;

        // Process each request
        for request in requests.iter() {
            // Validate the request
            match validate_limit_request(&request) {
                Ok(()) => {
                    // Validation succeeded - update the limit
                    let limit = SpendingLimit {
                        user: request.user.clone(),
                        monthly_limit: request.monthly_limit,
                        current_spending: 0, // Reset spending when updating limit
                        category: request.category.clone(),
                        updated_at: current_ledger,
                        is_active: true,
                    };

                    // Accumulate metrics
                    total_limits_value = total_limits_value
                        .checked_add(request.monthly_limit)
                        .unwrap_or(i128::MAX);
                    successful_count += 1;

                    // Store the limit (optimized - one write per limit)
                    env.storage()
                        .persistent()
                        .set(&DataKey::SpendingLimit(request.user.clone()), &limit);

                    // Emit success event
                    LimitEvents::limit_updated(&env, batch_id, &limit);

                    // Emit high-value limit event if applicable (>= 1,000,000 XLM)
                    if request.monthly_limit >= 10_000_000_000_000_000 {
                        LimitEvents::high_value_limit(
                            &env,
                            batch_id,
                            &request.user,
                            request.monthly_limit,
                        );
                    }

                    results.push_back(LimitUpdateResult::Success(limit));
                }
                Err(error_code) => {
                    // Validation failed - record failure
                    failed_count += 1;

                    // Emit failure event
                    LimitEvents::limit_update_failed(&env, batch_id, &request.user, error_code);

                    results.push_back(LimitUpdateResult::Failure(
                        request.user.clone(),
                        error_code,
                    ));
                }
            }
        }

        // Calculate average limit amount
        let avg_limit_amount = if successful_count > 0 {
            total_limits_value / successful_count as i128
        } else {
            0
        };

        // Create metrics
        let metrics = BatchLimitMetrics {
            total_requests: request_count,
            successful_updates: successful_count,
            failed_updates: failed_count,
            total_limits_value,
            avg_limit_amount,
            processed_at: current_ledger,
        };

        // Update storage (batched at the end for efficiency)
        let total_limits: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalLimitsUpdated)
            .unwrap_or(0);
        let total_batches: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalBatchesProcessed)
            .unwrap_or(0);

        env.storage()
            .instance()
            .set(&DataKey::LastBatchId, &batch_id);
        env.storage().instance().set(
            &DataKey::TotalLimitsUpdated,
            &(total_limits + successful_count as u64),
        );
        env.storage()
            .instance()
            .set(&DataKey::TotalBatchesProcessed, &(total_batches + 1));

        // Emit batch completed event
        LimitEvents::batch_completed(
            &env,
            batch_id,
            successful_count,
            failed_count,
            total_limits_value,
        );

        BatchLimitResult {
            batch_id,
            total_requests: request_count,
            successful: successful_count,
            failed: failed_count,
            results,
            metrics,
        }
    }

    /// Retrieves a user's spending limit.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `user` - The user's address
    ///
    /// # Returns
    /// * `Option<SpendingLimit>` - The limit if found
    pub fn get_spending_limit(env: Env, user: Address) -> Option<SpendingLimit> {
        env.storage()
            .persistent()
            .get(&DataKey::SpendingLimit(user))
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

    /// Returns the last created batch ID.
    pub fn get_last_batch_id(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::LastBatchId)
            .unwrap_or(0)
    }

    /// Returns the total number of limits updated.
    pub fn get_total_limits_updated(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalLimitsUpdated)
            .unwrap_or(0)
    }

    /// Returns the total number of batches processed.
    pub fn get_total_batches_processed(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalBatchesProcessed)
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
            panic_with_error!(env, SpendingLimitError::Unauthorized);
        }
    }
}

#[cfg(test)]
mod test;

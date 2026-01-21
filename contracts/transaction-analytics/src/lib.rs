//! # Transaction Analytics Contract
//!
//! A Soroban smart contract for generating batch analytics for multiple transactions.
//!
//! ## Features
//!
//! - **Batch Processing**: Efficiently process multiple transactions in a single call
//! - **Aggregated Metrics**: Compute total volume, averages, min/max, unique addresses
//! - **Category Breakdown**: Analytics grouped by transaction category
//! - **Event Emission**: Emit analytics events for off-chain consumption
//! - **High-Value Alerts**: Detect and flag high-value transactions
//!
//! ## Optimization Strategies
//!
//! - Single-pass computation for O(n) complexity
//! - Minimized storage operations
//! - Efficient data structures (Maps for lookups)
//! - Batched event emissions

#![no_std]

mod analytics;
mod types;

use soroban_sdk::{contract, contractimpl, panic_with_error, Address, Env, Vec};

pub use crate::analytics::{
    compute_batch_checksum, compute_batch_metrics, compute_category_metrics,
    find_high_value_transactions, validate_batch,
};
pub use crate::types::{
    AnalyticsEvents, BatchMetrics, CategoryMetrics, DataKey, Transaction, MAX_BATCH_SIZE,
};

/// Error codes for the analytics contract.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum AnalyticsError {
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
    /// Invalid transaction amount
    InvalidAmount = 6,
}

impl From<AnalyticsError> for soroban_sdk::Error {
    fn from(e: AnalyticsError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

#[contract]
pub struct TransactionAnalyticsContract;

#[contractimpl]
impl TransactionAnalyticsContract {
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
        env.storage().instance().set(&DataKey::TotalTxProcessed, &0u64);
    }

    /// Generates batch analytics for multiple transactions.
    ///
    /// This is the main entry point for processing transaction batches.
    /// It computes metrics, emits events, and stores results.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `caller` - The address calling this function (must be admin)
    /// * `transactions` - Vector of transactions to analyze
    /// * `high_value_threshold` - Optional threshold for high-value alerts
    ///
    /// # Returns
    /// * `BatchMetrics` - Aggregated metrics for the batch
    ///
    /// # Events Emitted
    /// * `analytics_started` - When processing begins
    /// * `batch_processed` - When batch metrics are computed
    /// * `category_analytics` - For each category in the batch
    /// * `high_value_alert` - For transactions above threshold
    /// * `analytics_completed` - When processing completes
    pub fn process_batch(
        env: Env,
        caller: Address,
        transactions: Vec<Transaction>,
        high_value_threshold: Option<i128>,
    ) -> BatchMetrics {
        // Verify authorization
        caller.require_auth();
        Self::require_admin(&env, &caller);

        // Validate batch
        let tx_count = transactions.len();
        if tx_count == 0 {
            panic_with_error!(&env, AnalyticsError::EmptyBatch);
        }
        if tx_count > MAX_BATCH_SIZE {
            panic_with_error!(&env, AnalyticsError::BatchTooLarge);
        }

        // Validate individual transactions
        if let Err(_) = validate_batch(&transactions) {
            panic_with_error!(&env, AnalyticsError::InvalidBatch);
        }

        // Get next batch ID (single read, single write at the end)
        let batch_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::LastBatchId)
            .unwrap_or(0)
            + 1;

        // Emit start event
        AnalyticsEvents::analytics_started(&env, batch_id, tx_count);

        // Compute batch metrics (single pass over data)
        let current_ledger = env.ledger().sequence() as u64;
        let metrics = compute_batch_metrics(&env, &transactions, current_ledger);

        // Emit batch processed event
        AnalyticsEvents::batch_processed(&env, batch_id, &metrics);

        // Compute and emit category metrics
        let category_metrics = compute_category_metrics(&env, &transactions, metrics.total_volume);
        for cat_metric in category_metrics.iter() {
            AnalyticsEvents::category_analytics(&env, batch_id, &cat_metric);
        }

        // Process high-value alerts if threshold provided
        if let Some(threshold) = high_value_threshold {
            let high_value_txs = find_high_value_transactions(&env, &transactions, threshold);
            for (tx_id, amount) in high_value_txs.iter() {
                AnalyticsEvents::high_value_alert(&env, batch_id, tx_id, amount);
            }
        }

        // Update storage (batched at the end for efficiency)
        let total_processed: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalTxProcessed)
            .unwrap_or(0);

        env.storage().instance().set(&DataKey::LastBatchId, &batch_id);
        env.storage()
            .instance()
            .set(&DataKey::TotalTxProcessed, &(total_processed + tx_count as u64));
        env.storage()
            .persistent()
            .set(&DataKey::BatchMetrics(batch_id), &metrics);

        // Emit completion event
        AnalyticsEvents::analytics_completed(&env, batch_id, tx_count as u64);

        metrics
    }

    /// Retrieves stored metrics for a specific batch.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `batch_id` - The ID of the batch to retrieve
    ///
    /// # Returns
    /// * `Option<BatchMetrics>` - The stored metrics if found
    pub fn get_batch_metrics(env: Env, batch_id: u64) -> Option<BatchMetrics> {
        env.storage()
            .persistent()
            .get(&DataKey::BatchMetrics(batch_id))
    }

    /// Returns the last processed batch ID.
    pub fn get_last_batch_id(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::LastBatchId)
            .unwrap_or(0)
    }

    /// Returns the total number of transactions processed.
    pub fn get_total_transactions_processed(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalTxProcessed)
            .unwrap_or(0)
    }

    /// Computes analytics without storing results (view-only).
    ///
    /// Useful for simulating analytics before committing.
    pub fn simulate_batch(env: Env, transactions: Vec<Transaction>) -> BatchMetrics {
        if let Err(_) = validate_batch(&transactions) {
            panic_with_error!(&env, AnalyticsError::InvalidBatch);
        }

        let current_ledger = env.ledger().sequence() as u64;
        compute_batch_metrics(&env, &transactions, current_ledger)
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

    // Internal helper to verify admin
    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Contract not initialized");

        if *caller != admin {
            panic_with_error!(env, AnalyticsError::Unauthorized);
        }
    }
}

#[cfg(test)]
mod test;

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
mod fees;
pub mod indexer;
mod types;
mod validation;

use soroban_sdk::{contract, contractimpl, panic_with_error, Address, Env, Symbol, Vec};

// Analytics exports (main branch — complete set)
pub use crate::analytics::{
    compute_aggregated_analytics, compute_batch_checksum, compute_batch_metrics,
    compute_category_metrics, compute_monthly_analytics, compute_refund_metrics,
    compute_user_spending_summary, create_bundle_result, find_high_value_transactions,
    process_refund_batch, update_monthly_analytics_storage, validate_audit_logs, validate_batch,
    validate_bundle_transactions, validate_refund_batch, validate_refund_eligibility,
    validate_transaction_for_bundle,
};

// Fees exports (single, de-duplicated block)
pub use crate::fees::{
    calculate_batch_fees, calculate_transaction_fee, deduct_fees, distribute_fee,
    get_current_fee_config, get_operation_fee_config, is_fee_paused, store_fee_config,
    store_operation_fee_config, update_fee_config, update_operation_fee_config,
    validate_fee_config, validate_recipient_shares,
};

// Types exports
pub use crate::indexer::{CategorySpendWindow, TransactionEvent};
pub use crate::types::{
    AnalyticsEvents, AuditLog, BatchMetrics, BatchStatusUpdateResult, BundleResult,
    BundledTransaction, CategoryMetrics, DataKey, FeeCalculationResult, FeeConfig,
    FeeDeductionEvent, FeeModel, FeeTier, MonthlySpendingAnalytics, PaginatedBatchMetrics,
    RatingInput, RatingResult, RatingStatus, RefundBatchMetrics, RefundRequest, RefundResult,
    RefundStatus, StatusUpdateResult, Transaction, TransactionStatus, TransactionStatusUpdate,
    UserSpendingSummary, ValidationError, ValidationResult, MAX_BATCH_SIZE, MAX_PAGE_SIZE,
};

// Validation exports (single, de-duplicated block)
pub use crate::validation::{
    validate_address, validate_amount, validate_amounts, validate_asset_amount,
    validate_asset_amounts, validate_asset_type, validate_bundled_transaction,
    validate_bundled_transactions, validate_different_addresses, validate_percentage_basis_points,
    validate_rating_input, validate_rating_inputs, validate_refund_request,
    validate_refund_requests, validate_transaction, validate_transaction_status_update,
    validate_transaction_status_updates, validate_transactions, validate_user_address,
    validate_year_month,
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
    /// Invalid page size for pagination
    InvalidPageSize = 6,
    /// Invalid transaction amount
    InvalidAmount = 7,
    /// Invalid audit log data
    InvalidAuditLog = 15,
    /// Bundle is empty
    EmptyBundle = 8,
    /// Bundle exceeds maximum size
    BundleTooLarge = 9,
    /// All transactions in bundle are invalid
    AllTransactionsInvalid = 10,
    /// Invalid refund batch data
    InvalidRefundBatch = 11,
    /// Refund batch is empty
    EmptyRefundBatch = 12,
    /// Refund batch exceeds maximum size
    RefundBatchTooLarge = 13,
    /// Contract already initialized
    AlreadyInitialized = 14,
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
            panic_with_error!(&env, AnalyticsError::AlreadyInitialized);
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::LastBatchId, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::TotalTxProcessed, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::TotalAuditLogs, &0u64);
        env.storage().instance().set(&DataKey::LastBundleId, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::LastRefundBatchId, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::TotalRefundAmount, &0i128);
        env.storage().instance().set(
            &DataKey::RefundedTransactions,
            &soroban_sdk::Map::<u64, bool>::new(&env),
        );
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

        // Validate the transaction batch
        if let Err(validation_error) = validate_transactions(&transactions) {
            match validation_error {
                ValidationError::EmptyBatch => panic_with_error!(&env, AnalyticsError::EmptyBatch),
                ValidationError::BatchTooLarge => {
                    panic_with_error!(&env, AnalyticsError::BatchTooLarge)
                }
                _ => panic_with_error!(&env, AnalyticsError::InvalidBatch),
            }
        }

        // FIX: define tx_count before use
        let tx_count = transactions.len() as u32;

        for tx in transactions.iter() {
            env.storage()
                .persistent()
                .set(&DataKey::KnownTransaction(tx.tx_id), &true);
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

        env.storage()
            .instance()
            .set(&DataKey::LastBatchId, &batch_id);
        env.storage().instance().set(
            &DataKey::TotalTxProcessed,
            &(total_processed + tx_count as u64),
        );
        env.storage()
            .persistent()
            .set(&DataKey::BatchMetrics(batch_id), &metrics);

        // Emit completion event
        AnalyticsEvents::analytics_completed(&env, batch_id, tx_count as u64);

        metrics
    }

    /// Logs multiple operations in a single batch (Audit Logging).
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `caller` - The address calling this function (must be admin)
    /// * `logs` - Vector of audit logs to store
    pub fn batch_audit_log(env: Env, caller: Address, logs: Vec<AuditLog>) {
        // Verify authorization
        caller.require_auth();
        Self::require_admin(&env, &caller);

        if logs.is_empty() {
            panic_with_error!(&env, AnalyticsError::EmptyBatch);
        }

        if logs.len() > MAX_BATCH_SIZE {
            panic_with_error!(&env, AnalyticsError::BatchTooLarge);
        }

        for log in logs.iter() {
            if log.timestamp == 0 {
                panic_with_error!(&env, AnalyticsError::InvalidAuditLog);
            }
        }

        // Get current total audit logs
        let mut total_logs: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalAuditLogs)
            .unwrap_or(0);

        // Store each log and emit event
        for log in logs.iter() {
            total_logs += 1;
            env.storage()
                .persistent()
                .set(&DataKey::AuditLog(total_logs), &log);

            AnalyticsEvents::audit_logged(&env, &log.actor, &log.operation, &log.status);
        }

        // Update total count
        env.storage()
            .instance()
            .set(&DataKey::TotalAuditLogs, &total_logs);
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

    /// Returns paginated batch metrics for historical analytics data.
    ///
    /// Empty pages return an empty result set.
    /// Page size values above the maximum are capped.
    pub fn get_batch_metrics_paginated(
        env: Env,
        page: u32,
        page_size: u32,
    ) -> PaginatedBatchMetrics {
        if page_size == 0 {
            panic_with_error!(&env, AnalyticsError::InvalidPageSize);
        }

        let page_size = core::cmp::min(page_size, MAX_PAGE_SIZE);
        let total_count = env
            .storage()
            .instance()
            .get(&DataKey::LastBatchId)
            .unwrap_or(0) as u32;

        if total_count == 0 {
            return PaginatedBatchMetrics {
                metrics: Vec::new(&env),
                total_count,
                page_number: page,
                page_size,
                has_next: false,
                has_previous: false,
            };
        }

        let start_index = page.saturating_mul(page_size) as usize;
        let end_index = core::cmp::min(start_index + page_size as usize, total_count as usize);

        if start_index >= total_count as usize {
            return PaginatedBatchMetrics {
                metrics: Vec::new(&env),
                total_count,
                page_number: page,
                page_size,
                has_next: false,
                has_previous: page > 0 && total_count > 0,
            };
        }

        let mut metrics: Vec<BatchMetrics> = Vec::new(&env);
        for index in start_index..end_index {
            let batch_id = (index as u64) + 1;
            if let Some(batch_metrics) = env
                .storage()
                .persistent()
                .get(&DataKey::BatchMetrics(batch_id))
            {
                metrics.push_back(batch_metrics);
            }
        }

        PaginatedBatchMetrics {
            metrics,
            total_count,
            page_number: page,
            page_size,
            has_next: end_index < total_count as usize,
            has_previous: page > 0,
        }
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

    /// Retrieves an audit log by its index.
    pub fn get_audit_log(env: Env, index: u64) -> Option<AuditLog> {
        env.storage().persistent().get(&DataKey::AuditLog(index))
    }

    /// Returns the total number of audit logs stored.
    pub fn get_total_audit_logs(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalAuditLogs)
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

    pub fn update_transaction_statuses(
        env: Env,
        caller: Address,
        updates: Vec<TransactionStatusUpdate>,
    ) -> BatchStatusUpdateResult {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        if let Err(validation_error) = validate_transaction_status_updates(&updates) {
            match validation_error {
                ValidationError::EmptyBatch => panic_with_error!(&env, AnalyticsError::EmptyBatch),
                ValidationError::BatchTooLarge => {
                    panic_with_error!(&env, AnalyticsError::BatchTooLarge)
                }
                _ => panic_with_error!(&env, AnalyticsError::InvalidBatch),
            }
        }

        let mut results: Vec<StatusUpdateResult> = Vec::new(&env);
        let mut successful: u32 = 0;
        let mut failed: u32 = 0;
        let count = updates.len() as u32;

        for update in updates.iter() {
            let known = env
                .storage()
                .persistent()
                .has(&DataKey::KnownTransaction(update.tx_id));

            if !known {
                failed += 1;
                AnalyticsEvents::transaction_status_update_failed(&env, update.tx_id);
                results.push_back(StatusUpdateResult {
                    tx_id: update.tx_id,
                    is_valid: false,
                });
                continue;
            }

            let previous_status: Option<TransactionStatus> = env
                .storage()
                .persistent()
                .get(&DataKey::TransactionStatus(update.tx_id));

            env.storage()
                .persistent()
                .set(&DataKey::TransactionStatus(update.tx_id), &update.status);

            successful += 1;
            AnalyticsEvents::transaction_status_updated(
                &env,
                update.tx_id,
                previous_status.clone(),
                update.status.clone(),
            );

            results.push_back(StatusUpdateResult {
                tx_id: update.tx_id,
                is_valid: true,
            });
        }

        BatchStatusUpdateResult {
            total_requests: count,
            successful,
            failed,
            results,
        }
    }

    pub fn submit_ratings(env: Env, user: Address, ratings: Vec<RatingInput>) -> Vec<RatingResult> {
        user.require_auth();

        if let Err(validation_error) = validate_rating_inputs(&ratings) {
            match validation_error {
                ValidationError::EmptyBatch => panic_with_error!(&env, AnalyticsError::EmptyBatch),
                ValidationError::BatchTooLarge => {
                    panic_with_error!(&env, AnalyticsError::BatchTooLarge)
                }
                _ => panic_with_error!(&env, AnalyticsError::InvalidBatch),
            }
        }

        let mut results: Vec<RatingResult> = Vec::new(&env);

        for input in ratings.iter() {
            let mut status = RatingStatus::Success;

            if input.score == 0 || input.score > 5 {
                status = RatingStatus::InvalidScore;
            } else {
                let known = env
                    .storage()
                    .persistent()
                    .has(&DataKey::KnownTransaction(input.tx_id));
                if !known {
                    status = RatingStatus::UnknownTransaction;
                } else {
                    env.storage()
                        .persistent()
                        .set(&DataKey::Rating(input.tx_id, user.clone()), &input.score);
                }
            }

            let result = RatingResult {
                tx_id: input.tx_id,
                score: input.score,
                status: status.clone(),
            };

            AnalyticsEvents::rating_submitted(&env, &user, input.tx_id, input.score, status);

            results.push_back(result);
        }

        results
    }

    /// Returns the admin address.
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Contract not initialized")
    }

    /// Returns the stored status for a transaction, if any.
    pub fn get_transaction_status(env: Env, tx_id: u64) -> Option<TransactionStatus> {
        env.storage()
            .persistent()
            .get(&DataKey::TransactionStatus(tx_id))
    }

    /// Updates the admin address.
    pub fn set_admin(env: Env, current_admin: Address, new_admin: Address) {
        current_admin.require_auth();
        Self::require_admin(&env, &current_admin);

        env.storage().instance().set(&DataKey::Admin, &new_admin);
    }

    /// Bundles multiple StellarSpend transactions into a single transaction group.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `caller` - The address calling this function (must be admin)
    /// * `bundled_transactions` - Vector of transactions to bundle
    ///
    /// # Returns
    /// * `BundleResult` - Result containing validation results and bundle status
    ///
    /// # Events Emitted
    /// * `bundling_started` - When bundling begins
    /// * `transaction_validated` - For each transaction validation result
    /// * `transaction_validation_failed` - For each failed validation
    /// * `bundle_created` - When bundle is created with results
    /// * `bundling_completed` - When bundling completes
    ///
    /// # Errors
    /// * `EmptyBundle` - If no transactions provided
    /// * `BundleTooLarge` - If bundle exceeds maximum size
    /// * `Unauthorized` - If caller is not admin
    pub fn bundle_transactions(
        env: Env,
        caller: Address,
        bundled_transactions: Vec<BundledTransaction>,
    ) -> BundleResult {
        // Verify authorization
        caller.require_auth();
        Self::require_admin(&env, &caller);

        if let Err(validation_error) = validate_bundled_transactions(&bundled_transactions) {
            match validation_error {
                ValidationError::EmptyBatch => {
                    panic_with_error!(&env, AnalyticsError::EmptyBundle)
                }
                ValidationError::BatchTooLarge => {
                    panic_with_error!(&env, AnalyticsError::BundleTooLarge)
                }
                _ => panic_with_error!(&env, AnalyticsError::InvalidBatch),
            }
        }

        // FIX: define tx_count before use
        let tx_count = bundled_transactions.len() as u32;

        // Get next bundle ID
        let bundle_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::LastBundleId)
            .unwrap_or(0)
            + 1;

        // Emit bundling started event
        AnalyticsEvents::bundling_started(&env, bundle_id, tx_count);

        // Validate all transactions (handles partial failures gracefully)
        let validation_results = validate_bundle_transactions(&env, &bundled_transactions);

        // Emit validation events for each transaction
        let mut _valid_count: u32 = 0;
        let mut _invalid_count: u32 = 0;

        for result in validation_results.iter() {
            AnalyticsEvents::transaction_validated(&env, bundle_id, &result);

            if result.is_valid {
                _valid_count += 1;
            } else {
                _invalid_count += 1;
                AnalyticsEvents::transaction_validation_failed(
                    &env,
                    bundle_id,
                    result.tx_id,
                    &result.error,
                );
            }
        }

        // Create bundle result
        let current_ledger = env.ledger().sequence() as u64;
        let bundle_result = create_bundle_result(
            &env,
            bundle_id,
            &bundled_transactions,
            &validation_results,
            current_ledger,
        );

        // Emit bundle created event
        AnalyticsEvents::bundle_created(&env, bundle_id, &bundle_result);

        // Store bundle result
        env.storage()
            .instance()
            .set(&DataKey::LastBundleId, &bundle_id);
        env.storage()
            .persistent()
            .set(&DataKey::BundleResult(bundle_id), &bundle_result);

        // Emit completion event
        AnalyticsEvents::bundling_completed(&env, bundle_id, bundle_result.can_bundle);

        bundle_result
    }

    /// Retrieves stored bundle result for a specific bundle ID.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `bundle_id` - The ID of the bundle to retrieve
    ///
    /// # Returns
    /// * `Option<BundleResult>` - The stored bundle result if found
    pub fn get_bundle_result(env: Env, bundle_id: u64) -> Option<BundleResult> {
        env.storage()
            .persistent()
            .get(&DataKey::BundleResult(bundle_id))
    }

    /// Returns the last created bundle ID.
    pub fn get_last_bundle_id(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::LastBundleId)
            .unwrap_or(0)
    }

    /// Processes a batch of refunds for failed or canceled transactions.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `caller` - The address calling this function (must be admin)
    /// * `refund_requests` - Vector of refund requests to process
    /// * `transaction_lookup` - Map of transaction IDs to Transaction objects for amount lookup
    ///
    /// # Returns
    /// * `RefundBatchMetrics` - Aggregated metrics for the refund batch
    ///
    /// # Events Emitted
    /// * `refund_batch_started` - When refund processing begins
    /// * `refund_processed` - For each individual refund attempt
    /// * `refund_batch_completed` - When batch processing completes
    /// * `refund_error` - For failed refund attempts
    pub fn refund_batch(
        env: Env,
        caller: Address,
        refund_requests: Vec<RefundRequest>,
        transaction_lookup: soroban_sdk::Map<u64, Transaction>,
    ) -> RefundBatchMetrics {
        // Verify authorization
        caller.require_auth();
        Self::require_admin(&env, &caller);

        if let Err(validation_error) = validate_refund_requests(&refund_requests) {
            match validation_error {
                ValidationError::EmptyBatch => {
                    panic_with_error!(&env, AnalyticsError::EmptyRefundBatch)
                }
                ValidationError::BatchTooLarge => {
                    panic_with_error!(&env, AnalyticsError::RefundBatchTooLarge)
                }
                _ => panic_with_error!(&env, AnalyticsError::InvalidRefundBatch),
            }
        }

        // FIX: define request_count before use
        let request_count = refund_requests.len() as u32;

        // Get next refund batch ID
        let refund_batch_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::LastRefundBatchId)
            .unwrap_or(0)
            + 1;

        // Get existing refunded transactions
        let mut refunded_txs: soroban_sdk::Map<u64, bool> = env
            .storage()
            .instance()
            .get(&DataKey::RefundedTransactions)
            .unwrap_or_else(|| soroban_sdk::Map::new(&env));

        // Emit start event
        AnalyticsEvents::refund_batch_started(&env, refund_batch_id, request_count);

        // Process refunds
        let refund_results = process_refund_batch(
            &env,
            &refund_requests,
            &transaction_lookup,
            &mut refunded_txs,
        );

        // Emit individual refund events
        for result in refund_results.iter() {
            AnalyticsEvents::refund_processed(&env, refund_batch_id, &result);

            if !result.success {
                if let Some(error_msg) = &result.error_message {
                    AnalyticsEvents::refund_error(
                        &env,
                        refund_batch_id,
                        result.tx_id,
                        error_msg.clone(),
                    );
                }
            }
        }

        // Compute refund metrics
        let current_ledger = env.ledger().sequence() as u64;
        let metrics = compute_refund_metrics(&env, &refund_results, current_ledger);

        // Update storage
        let total_refunded: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalRefundAmount)
            .unwrap_or(0);

        env.storage()
            .instance()
            .set(&DataKey::LastRefundBatchId, &refund_batch_id);
        env.storage().instance().set(
            &DataKey::TotalRefundAmount,
            &(total_refunded + metrics.total_refunded_amount),
        );
        env.storage()
            .persistent()
            .set(&DataKey::RefundBatchMetrics(refund_batch_id), &metrics);
        env.storage()
            .instance()
            .set(&DataKey::RefundedTransactions, &refunded_txs);

        // Emit completion event
        AnalyticsEvents::refund_batch_completed(&env, refund_batch_id, &metrics);

        metrics
    }

    /// Simulates refund processing without storing results (view-only).
    ///
    /// Useful for testing refund eligibility and expected outcomes before committing.
    pub fn simulate_refund_batch(
        env: Env,
        refund_requests: Vec<RefundRequest>,
        transaction_lookup: soroban_sdk::Map<u64, Transaction>,
    ) -> RefundBatchMetrics {
        if let Err(validation_error) = validate_refund_requests(&refund_requests) {
            match validation_error {
                ValidationError::EmptyBatch => {
                    panic_with_error!(&env, AnalyticsError::EmptyRefundBatch)
                }
                ValidationError::BatchTooLarge => {
                    panic_with_error!(&env, AnalyticsError::RefundBatchTooLarge)
                }
                _ => panic_with_error!(&env, AnalyticsError::InvalidRefundBatch),
            }
        }

        let refunded_txs: soroban_sdk::Map<u64, bool> = env
            .storage()
            .instance()
            .get(&DataKey::RefundedTransactions)
            .unwrap_or_else(|| soroban_sdk::Map::new(&env));

        let mut temp_refunded_txs = refunded_txs.clone();
        let refund_results = process_refund_batch(
            &env,
            &refund_requests,
            &transaction_lookup,
            &mut temp_refunded_txs,
        );

        let current_ledger = env.ledger().sequence() as u64;
        compute_refund_metrics(&env, &refund_results, current_ledger)
    }

    /// Retrieves stored metrics for a specific refund batch.
    pub fn get_refund_batch_metrics(env: Env, batch_id: u64) -> Option<RefundBatchMetrics> {
        env.storage()
            .persistent()
            .get(&DataKey::RefundBatchMetrics(batch_id))
    }

    /// Returns the last processed refund batch ID.
    pub fn get_last_refund_batch_id(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::LastRefundBatchId)
            .unwrap_or(0)
    }

    /// Returns the total refund amount processed.
    pub fn get_total_refund_amount(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalRefundAmount)
            .unwrap_or(0)
    }

    /// Checks if a transaction has been refunded.
    pub fn is_transaction_refunded(env: Env, tx_id: u64) -> bool {
        let refunded_txs: soroban_sdk::Map<u64, bool> = env
            .storage()
            .instance()
            .get(&DataKey::RefundedTransactions)
            .unwrap_or_else(|| soroban_sdk::Map::new(&env));

        refunded_txs.contains_key(tx_id)
    }

    /// Updates monthly spending analytics for a user.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `caller` - The address calling this function (must be admin)
    /// * `user` - The user address to analyze
    /// * `transactions` - Vector of transactions to analyze
    /// * `year` - The year to analyze
    /// * `month` - The month to analyze
    pub fn update_monthly_analytics(
        env: Env,
        caller: Address,
        user: Address,
        transactions: Vec<Transaction>,
        year: u32,
        month: u32,
    ) -> MonthlySpendingAnalytics {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        if let Err(validation_error) = validate_user_address(&env, &user) {
            match validation_error {
                ValidationError::InvalidAddress => {
                    panic_with_error!(&env, AnalyticsError::Unauthorized)
                }
                _ => panic_with_error!(&env, AnalyticsError::InvalidBatch),
            }
        }

        if let Err(validation_error) = validate_year_month(year, month) {
            match validation_error {
                ValidationError::InvalidYear => {
                    panic_with_error!(&env, AnalyticsError::InvalidBatch)
                }
                ValidationError::InvalidMonth => {
                    panic_with_error!(&env, AnalyticsError::InvalidBatch)
                }
                _ => panic_with_error!(&env, AnalyticsError::InvalidBatch),
            }
        }

        if let Err(validation_error) = validate_transactions(&transactions) {
            match validation_error {
                ValidationError::EmptyBatch => panic_with_error!(&env, AnalyticsError::EmptyBatch),
                ValidationError::BatchTooLarge => {
                    panic_with_error!(&env, AnalyticsError::BatchTooLarge)
                }
                _ => panic_with_error!(&env, AnalyticsError::InvalidBatch),
            }
        }

        // Compute monthly analytics
        let analytics = compute_monthly_analytics(&env, &user, &transactions, year, month);

        // Update storage
        update_monthly_analytics_storage(&env, &analytics);

        // Emit analytics update event
        AnalyticsEvents::analytics_updated(&env, &user, year, month, &analytics);

        analytics
    }

    /// Retrieves monthly spending analytics for a user.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `user` - The user address
    /// * `year` - The year of the analytics
    /// * `month` - The month of the analytics
    ///
    /// # Returns
    /// * `Option<MonthlySpendingAnalytics>` - The stored analytics if found
    pub fn get_monthly_analytics(
        env: Env,
        user: Address,
        year: u32,
        month: u32,
    ) -> Option<MonthlySpendingAnalytics> {
        let key = DataKey::MonthlyAnalytics(year, month, user);
        env.storage().persistent().get(&key)
    }

    /// Gets user spending summary across all tracked periods.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `user` - The user address
    ///
    /// # Returns
    /// * `Option<UserSpendingSummary>` - The user spending summary if found
    pub fn get_user_spending_summary(env: Env, user: Address) -> Option<UserSpendingSummary> {
        let key = DataKey::UserSpendingSummary(user);
        env.storage().persistent().get(&key)
    }

    /// Gets the total number of tracked users.
    pub fn get_total_tracked_users(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalTrackedUsers)
            .unwrap_or(0)
    }

    /// Gets the timestamp of the last analytics update.
    pub fn get_last_analytics_update(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::LastAnalyticsUpdate)
            .unwrap_or(0)
    }

    /// Updates the fee configuration.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - The admin address (must be authorized)
    /// * `new_config` - The new fee configuration
    pub fn update_fee_config(env: Env, admin: Address, new_config: FeeConfig) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        if let Err(validation_error) = validate_fee_config(&new_config) {
            match validation_error {
                ValidationError::InvalidPercentage => {
                    panic_with_error!(&env, AnalyticsError::InvalidBatch)
                }
                ValidationError::InvalidAmount => {
                    panic_with_error!(&env, AnalyticsError::InvalidAmount)
                }
                _ => panic_with_error!(&env, AnalyticsError::InvalidBatch),
            }
        }

        // Capture previous max_fee for event emission
        let previous_max = get_current_fee_config(&env).and_then(|c| c.max_fee);

        // Store the new configuration
        store_fee_config(&env, &new_config).expect("Failed to store fee configuration");

        // Emit event if max_fee (cap) changed
        if previous_max != new_config.max_fee {
            crate::types::AnalyticsEvents::fee_cap_changed(
                &env,
                &admin,
                previous_max,
                new_config.max_fee,
            );
        }
    }

    /// Updates per-operation fee configuration.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - The admin address (must be authorized)
    /// * `operation` - The operation symbol to configure
    /// * `new_config` - The new fee configuration for this operation
    pub fn update_operation_fee_config(
        env: Env,
        admin: Address,
        operation: Symbol,
        new_config: FeeConfig,
    ) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        if let Err(validation_error) = crate::fees::validate_fee_config(&new_config) {
            match validation_error {
                ValidationError::InvalidPercentage => {
                    panic_with_error!(&env, AnalyticsError::InvalidBatch)
                }
                ValidationError::InvalidAmount => {
                    panic_with_error!(&env, AnalyticsError::InvalidAmount)
                }
                _ => panic_with_error!(&env, AnalyticsError::InvalidBatch),
            }
        }

        let previous = crate::fees::get_operation_fee_config(&env, &operation);
        crate::fees::store_operation_fee_config(&env, &operation, &new_config)
            .expect("Failed to store operation fee config");

        // Emit operation fee updated event
        crate::types::AnalyticsEvents::operation_fee_updated(
            &env, &admin, &operation, previous, new_config,
        );
    }

    /// Gets the current fee configuration.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    ///
    /// # Returns
    /// * `Option<FeeConfig>` - The current fee configuration if set
    pub fn get_current_fee_config(env: Env) -> Option<FeeConfig> {
        get_current_fee_config(&env)
    }

    /// Calculates fees for a single transaction.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `amount` - The transaction amount
    ///
    /// # Returns
    /// * `FeeCalculationResult` - The fee calculation result
    pub fn calculate_transaction_fee(env: Env, amount: i128) -> FeeCalculationResult {
        let config = get_current_fee_config(&env).unwrap_or_else(|| FeeConfig {
            fee_model: FeeModel::Percentage(10), // 0.1% default
            min_fee: Some(1),
            max_fee: None,
            enabled: true,
            description: Some(Symbol::new(&env, "Default")),
        });

        calculate_transaction_fee(&env, amount, &config)
    }

    /// Calculates fees for a batch of transactions.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `amounts` - Vector of transaction amounts
    ///
    /// # Returns
    /// * `Vec<FeeCalculationResult>` - Vector of fee calculation results
    pub fn calculate_batch_fees(env: Env, amounts: Vec<i128>) -> Vec<FeeCalculationResult> {
        let config = get_current_fee_config(&env).unwrap_or_else(|| FeeConfig {
            fee_model: FeeModel::Percentage(10), // 0.1% default
            min_fee: Some(1),
            max_fee: None,
            enabled: true,
            description: Some(Symbol::new(&env, "Default")),
        });

        // FIX: pass &amounts directly — no invalid iterator conversion
        crate::fees::calculate_batch_fees(&env, &amounts, &config)
    }

    /// Pauses fee collection (admin only).
    pub fn pause_fees(env: Env, admin: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        crate::fees::set_fee_paused(&env, &admin, true).expect("Failed to pause fees");
    }

    /// Resumes fee collection (admin only).
    pub fn resume_fees(env: Env, admin: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        crate::fees::set_fee_paused(&env, &admin, false).expect("Failed to resume fees");
    }

    pub fn spending_by_category_in_window(
        _env: Env,
        events: Vec<indexer::TransactionEvent>,
        window_start: u64,
        window_end: u64,
    ) -> Vec<indexer::CategorySpendWindow> {
        indexer::aggregate_by_category_window(&events, window_start, window_end)
    }

    pub fn recategorize_transaction(
        env: Env,
        caller: Address,
        events: Vec<indexer::TransactionEvent>,
        tx_id: u64,
        new_category: Symbol,
    ) -> Vec<indexer::TransactionEvent> {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        let mut map = indexer::consume_events(&events);
        if !indexer::recategorize_event(&mut map, tx_id, new_category) {
            panic_with_error!(&env, AnalyticsError::InvalidBatch);
        }
        let mut result: Vec<indexer::TransactionEvent> = Vec::new(&env);
        for (_, event) in map.iter() {
            result.push_back(event);
        }
        result
    }

    pub fn recategorize_and_aggregate(
        env: Env,
        caller: Address,
        events: Vec<indexer::TransactionEvent>,
        tx_id: u64,
        new_category: Symbol,
        window_start: u64,
        window_end: u64,
    ) -> Vec<indexer::CategorySpendWindow> {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        let mut map = indexer::consume_events(&events);
        indexer::recategorize_event(&mut map, tx_id, new_category);
        let mut updated_events: Vec<indexer::TransactionEvent> = Vec::new(&env);
        for (_, event) in map.iter() {
            updated_events.push_back(event);
        }
        indexer::aggregate_by_category_window(&updated_events, window_start, window_end)
    }

    /// Returns a simple analytics summary for a user.
    pub fn get_analytics_summary(env: Env, _owner: Address) -> (u32, i128) {
        let total_txs: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalTxProcessed)
            .unwrap_or(0);
        let total_refunded: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalRefundAmount)
            .unwrap_or(0);

        (total_txs as u32, total_refunded)
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
impl TransactionAnalyticsContract {
    /// Returns a simple analytics summary for a user.
    pub fn get_analytics_summary(env: Env, _owner: Address) -> (u32, i128) {
        let total_txs: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalTxProcessed)
            .unwrap_or(0);
        let total_refunded: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalRefundAmount)
            .unwrap_or(0);

        (total_txs as u32, total_refunded)
    }

    // other methods unchanged
}

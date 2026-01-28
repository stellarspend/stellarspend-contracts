//! # Multi-Currency Wallet Contract
//!
//! A Soroban smart contract for managing batch balance updates
//! for multiple users across multiple currencies simultaneously.
//!
//! ## Features
//!
//! - **Batch Processing**: Efficiently update balances for multiple users in a single call
//! - **Multi-Currency Support**: Handle multiple currencies (XLM, USDC, EURC, etc.)
//! - **Flexible Operations**: Set, add, or subtract balance amounts
//! - **Comprehensive Validation**: Validates amounts, currencies, and operations
//! - **Event Emission**: Emits events for balance updates and batch processing
//! - **Error Handling**: Gracefully handles invalid inputs with detailed error codes
//! - **Partial Failure Support**: Invalid updates don't affect valid ones
//!
//! ## Optimization Strategies
//!
//! - Single-pass processing for O(n) complexity
//! - Minimized storage operations
//! - Efficient data structures
//! - Batched event emissions

#![no_std]

mod types;
mod validation;

use soroban_sdk::{contract, contractimpl, panic_with_error, Address, Env, Symbol, Vec};

pub use crate::types::{
    BalanceUpdateRequest, BalanceUpdateResult, BatchBalanceMetrics, BatchBalanceResult,
    CurrencyBalance, DataKey, ErrorCode, WalletEvents, MAX_BATCH_SIZE,
};
use crate::validation::{validate_and_compute_balance, validate_balance_request};

/// Error codes for the multi-currency wallet contract.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum WalletError {
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

impl From<WalletError> for soroban_sdk::Error {
    fn from(e: WalletError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

#[contract]
pub struct MultiCurrencyWalletContract;

#[contractimpl]
impl MultiCurrencyWalletContract {
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
            .set(&DataKey::TotalBalancesUpdated, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::TotalBatchesProcessed, &0u64);
    }

    /// Updates balances for multiple users across multiple currencies in a batch.
    ///
    /// This is the main entry point for batch balance updates. It validates all requests,
    /// updates balances, emits events, and handles partial failures gracefully.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `caller` - The address calling this function (must be admin)
    /// * `requests` - Vector of balance update requests
    ///
    /// # Returns
    /// * `BatchBalanceResult` - Result containing updated balances and metrics
    ///
    /// # Events Emitted
    /// * `batch_started` - When processing begins
    /// * `balance_updated` - For each successful balance update
    /// * `balance_update_failed` - For each failed balance update
    /// * `large_balance_update` - For large balance values
    /// * `batch_completed` - When processing completes
    ///
    /// # Errors
    /// * `EmptyBatch` - If no requests provided
    /// * `BatchTooLarge` - If batch exceeds maximum size
    /// * `Unauthorized` - If caller is not admin
    pub fn batch_update_balances(
        env: Env,
        caller: Address,
        requests: Vec<BalanceUpdateRequest>,
    ) -> BatchBalanceResult {
        // Verify authorization
        caller.require_auth();
        Self::require_admin(&env, &caller);

        // Validate batch size
        let request_count = requests.len();
        if request_count == 0 {
            panic_with_error!(&env, WalletError::EmptyBatch);
        }
        if request_count > MAX_BATCH_SIZE {
            panic_with_error!(&env, WalletError::BatchTooLarge);
        }

        // Get batch ID and increment
        let batch_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::LastBatchId)
            .unwrap_or(0)
            + 1;

        // Emit batch started event
        WalletEvents::batch_started(&env, batch_id, request_count);

        // Get current ledger timestamp
        let current_ledger = env.ledger().sequence() as u64;

        // Initialize result tracking
        let mut results: Vec<BalanceUpdateResult> = Vec::new(&env);
        let mut successful_count: u32 = 0;
        let mut failed_count: u32 = 0;

        // Track unique users and currencies for metrics
        let mut unique_users: Vec<Address> = Vec::new(&env);
        let mut unique_currencies: Vec<Symbol> = Vec::new(&env);

        // Process each request
        for request in requests.iter() {
            // Validate the request
            match validate_balance_request(&request) {
                Ok(()) => {
                    // Validate and compute new balance
                    match validate_and_compute_balance(
                        &env,
                        &request.user,
                        &request.currency,
                        &request.operation,
                        request.amount,
                    ) {
                        Ok(new_balance) => {
                            // Update succeeded - create the balance record
                            let balance = CurrencyBalance {
                                user: request.user.clone(),
                                currency: request.currency.clone(),
                                balance: new_balance,
                                updated_at: current_ledger,
                            };

                            successful_count += 1;

                            // Store the balance (optimized - one write per balance)
                            env.storage().persistent().set(
                                &DataKey::Balance(request.user.clone(), request.currency.clone()),
                                &balance,
                            );

                            // Track unique users
                            if !contains_address(&unique_users, &request.user) {
                                unique_users.push_back(request.user.clone());
                            }

                            // Track unique currencies
                            if !contains_symbol(&unique_currencies, &request.currency) {
                                unique_currencies.push_back(request.currency.clone());
                            }

                            // Emit success event
                            WalletEvents::balance_updated(&env, batch_id, &balance);

                            // Emit large balance event if applicable (>= 1,000,000 units)
                            if new_balance >= 1_000_000 {
                                WalletEvents::large_balance_update(
                                    &env,
                                    batch_id,
                                    &request.user,
                                    &request.currency,
                                    new_balance,
                                );
                            }

                            results.push_back(BalanceUpdateResult::Success(balance));
                        }
                        Err(error_code) => {
                            // Balance computation failed
                            failed_count += 1;

                            WalletEvents::balance_update_failed(
                                &env,
                                batch_id,
                                &request.user,
                                &request.currency,
                                error_code,
                            );

                            results.push_back(BalanceUpdateResult::Failure(
                                request.user.clone(),
                                request.currency.clone(),
                                error_code,
                            ));
                        }
                    }
                }
                Err(error_code) => {
                    // Validation failed
                    failed_count += 1;

                    WalletEvents::balance_update_failed(
                        &env,
                        batch_id,
                        &request.user,
                        &request.currency,
                        error_code,
                    );

                    results.push_back(BalanceUpdateResult::Failure(
                        request.user.clone(),
                        request.currency.clone(),
                        error_code,
                    ));
                }
            }
        }

        // Create metrics
        let metrics = BatchBalanceMetrics {
            total_requests: request_count,
            successful_updates: successful_count,
            failed_updates: failed_count,
            unique_users: unique_users.len(),
            unique_currencies: unique_currencies.len(),
            processed_at: current_ledger,
        };

        // Update storage (batched at the end for efficiency)
        let total_balances: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalBalancesUpdated)
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
            &DataKey::TotalBalancesUpdated,
            &(total_balances + successful_count as u64),
        );
        env.storage()
            .instance()
            .set(&DataKey::TotalBatchesProcessed, &(total_batches + 1));

        // Emit batch completed event
        WalletEvents::batch_completed(&env, batch_id, successful_count, failed_count);

        BatchBalanceResult {
            batch_id,
            total_requests: request_count,
            successful: successful_count,
            failed: failed_count,
            results,
            metrics,
        }
    }

    /// Retrieves a user's balance for a specific currency.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `user` - The user's address
    /// * `currency` - The currency symbol
    ///
    /// # Returns
    /// * `i128` - The balance (0 if not found)
    pub fn get_balance(env: Env, user: Address, currency: Symbol) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Balance(user, currency))
            .map(|b: CurrencyBalance| b.balance)
            .unwrap_or(0)
    }

    /// Retrieves full balance details for a user and currency.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `user` - The user's address
    /// * `currency` - The currency symbol
    ///
    /// # Returns
    /// * `Option<CurrencyBalance>` - The balance details if found
    pub fn get_balance_details(
        env: Env,
        user: Address,
        currency: Symbol,
    ) -> Option<CurrencyBalance> {
        env.storage()
            .persistent()
            .get(&DataKey::Balance(user, currency))
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

    /// Returns the total number of balances updated.
    pub fn get_total_balances_updated(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalBalancesUpdated)
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
            panic_with_error!(env, WalletError::Unauthorized);
        }
    }
}

// Helper functions for tracking unique items
fn contains_address(vec: &Vec<Address>, addr: &Address) -> bool {
    for item in vec.iter() {
        if item == *addr {
            return true;
        }
    }
    false
}

fn contains_symbol(vec: &Vec<Symbol>, sym: &Symbol) -> bool {
    for item in vec.iter() {
        if item == *sym {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod test;

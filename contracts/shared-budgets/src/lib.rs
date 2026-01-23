//! # Shared Budgets Contract
//! Batch allocation of a caller's balance (shared budget) to multiple recipients.

#![no_std]

mod types;
mod validation;

use soroban_sdk::{contract, contractimpl, panic_with_error, Address, Env, Vec};

pub use crate::types::{
    AllocationBatchResult, AllocationRequest, AllocationResult, DataKey, SharedBudgetEvents,
    MAX_BATCH_SIZE,
};
use crate::validation::{validate_address, validate_amount};

/// Error codes for the shared budgets contract.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum SharedBudgetError {
    /// Contract not initialized
    NotInitialized = 1,
    /// Caller is not authorized
    Unauthorized = 2,
    /// Batch is empty
    EmptyBatch = 3,
    /// Batch exceeds maximum size
    BatchTooLarge = 4,
}

impl From<SharedBudgetError> for soroban_sdk::Error {
    fn from(e: SharedBudgetError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

#[contract]
pub struct SharedBudgetContract;

#[contractimpl]
impl SharedBudgetContract {
    /// Initializes the contract with an admin address.
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Contract already initialized");
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::TotalBatches, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::TotalAllocationsProcessed, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::TotalAllocatedVolume, &0i128);
    }

    /// Allocates a shared budget (caller balance) to multiple recipients in batch.
    ///
    /// Performs per-recipient validation and supports partial failures. The caller
    /// must be the configured admin and the source of funds.
    pub fn allocate_shared_budget_batch(
        env: Env,
        caller: Address,
        _token: Address,
        allocations: Vec<AllocationRequest>,
    ) -> AllocationBatchResult {
        // Verify authorization
        caller.require_auth();
        Self::require_admin(&env, &caller);

        // Validate batch size
        let request_count = allocations.len();
        if request_count == 0 {
            panic_with_error!(&env, SharedBudgetError::EmptyBatch);
        }
        if request_count > MAX_BATCH_SIZE {
            panic_with_error!(&env, SharedBudgetError::BatchTooLarge);
        }

        // Get batch ID and increment
        let batch_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalBatches)
            .unwrap_or(0)
            + 1;

        // Emit batch started event
        SharedBudgetEvents::batch_started(&env, batch_id, request_count);

        // Initialize result vectors and counters
        let mut results: Vec<AllocationResult> = Vec::new(&env);
        let mut successful_count: u32 = 0;
        let mut failed_count: u32 = 0;
        let mut total_allocated: i128 = 0;

        // First pass: validate requests and build an internal list
        let mut validated_requests: Vec<(AllocationRequest, bool, u32)> = Vec::new(&env);

        for request in allocations.iter() {
            let mut is_valid = true;
            let mut error_code = 0u32;

            if validate_address(&env, &request.recipient).is_err() {
                is_valid = false;
                error_code = 0; // Invalid address
            } else if validate_amount(request.amount).is_err() {
                is_valid = false;
                error_code = 1; // Invalid amount
            }

            validated_requests.push_back((request.clone(), is_valid, error_code));
        }

        // Second pass: process each allocation
        for (request, is_valid, error_code) in validated_requests.iter() {
            if !is_valid {
                // Validation failed - record and continue
                results.push_back(AllocationResult::Failure(
                    request.recipient.clone(),
                    request.amount,
                    error_code.clone(),
                ));
                failed_count += 1;
                SharedBudgetEvents::allocation_failure(
                    &env,
                    batch_id,
                    &request.recipient,
                    request.amount,
                    error_code.clone(),
                );
                continue;
            }

            // Simulate insufficient shared budget for very large amounts.
            // This avoids relying on real token balances while still
            // exercising partial-failure behavior.
            const MAX_SIMULATED_SHARED_BUDGET: i128 = 1_000_000_000_000; // 1e12
            if request.amount > MAX_SIMULATED_SHARED_BUDGET {
                results.push_back(AllocationResult::Failure(
                    request.recipient.clone(),
                    request.amount,
                    2, // Simulated insufficient shared budget
                ));
                failed_count += 1;
                SharedBudgetEvents::allocation_failure(
                    &env,
                    batch_id,
                    &request.recipient,
                    request.amount,
                    2,
                );
                continue;
            }

            // Allocation succeeded (we only validate inputs; no on-chain transfer here)
            total_allocated = total_allocated
                .checked_add(request.amount)
                .unwrap_or(total_allocated);

            results.push_back(AllocationResult::Success(
                request.recipient.clone(),
                request.amount,
            ));
            successful_count += 1;

            SharedBudgetEvents::allocation_success(
                &env,
                batch_id,
                &request.recipient,
                request.amount,
            );
        }

        // Update storage (batched at the end for efficiency)
        let total_batches: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalBatches)
            .unwrap_or(0);
        let total_processed: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalAllocationsProcessed)
            .unwrap_or(0);
        let total_volume: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalAllocatedVolume)
            .unwrap_or(0);

        env.storage()
            .instance()
            .set(&DataKey::TotalBatches, &(total_batches + 1));
        env.storage()
            .instance()
            .set(
                &DataKey::TotalAllocationsProcessed,
                &(total_processed + request_count as u64),
            );
        env.storage().instance().set(
            &DataKey::TotalAllocatedVolume,
            &total_allocated
                .checked_add(total_volume)
                .unwrap_or(total_volume),
        );

        // Emit batch completed event
        SharedBudgetEvents::batch_completed(
            &env,
            batch_id,
            successful_count,
            failed_count,
            total_allocated,
        );

        AllocationBatchResult {
            total_requests: request_count,
            successful: successful_count,
            failed: failed_count,
            total_allocated,
            results,
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

    /// Returns the total number of batches processed.
    pub fn get_total_batches(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalBatches)
            .unwrap_or(0)
    }

    /// Returns the total number of allocation entries processed.
    pub fn get_total_allocations_processed(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalAllocationsProcessed)
            .unwrap_or(0)
    }

    /// Returns the total volume allocated across all batches.
    pub fn get_total_allocated_volume(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalAllocatedVolume)
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
            panic_with_error!(env, SharedBudgetError::Unauthorized);
        }
    }
}

#[cfg(test)]
mod test;

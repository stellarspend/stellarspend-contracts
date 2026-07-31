//! # Batch Transfer Contract
#![no_std]

mod types;
mod validation;

use shared::batch_result::BatchItemResult;
use soroban_sdk::{contract, contractimpl, panic_with_error, token, Address, Env, Vec};

pub use crate::types::{
    BatchBurnResult, BatchTransferResult, BurnRequest, BurnResult, DataKey, TransferEvents,
    TransferRequest, TransferResult, MAX_BATCH_SIZE,
};
//bbbb
use crate::validation::{
    validate_address, validate_amount, validate_batch_not_empty, validate_unique_recipients,
};
use shared::validation::validate_batch_size;

/// Error codes for the batch transfer contract.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum BatchTransferError {
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
    /// Invalid token contract
    InvalidToken = 6,
    /// Duplicate recipient in batch
    DuplicateRecipient = 7,
    /// Batch exceeds the available instruction budget
    InstructionBudgetExceeded = 8,
}

impl From<BatchTransferError> for soroban_sdk::Error {
    fn from(e: BatchTransferError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

#[contract]
pub struct BatchTransferContract;

#[contractimpl]
impl BatchTransferContract {
    /// Initializes the contract with an admin address.
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Contract already initialized");
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::TotalBatches, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::TotalTransfersProcessed, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::TotalVolumeTransferred, &0i128);
    }

    /// Executes batch transfers of XLM to multiple recipients.
    pub fn batch_transfer(
        env: Env,
        caller: Address,
        token: Address,
        transfers: Vec<TransferRequest>,
    ) -> BatchTransferResult {
        // Verify authorization
        caller.require_auth();
        Self::require_admin(&env, &caller);

        // Validate batch is not empty (early validation for efficiency)
        if validate_batch_not_empty(&transfers).is_err() {
            panic_with_error!(&env, BatchTransferError::EmptyBatch);
        }

        // Validate batch size
        let request_count = transfers.len();
        if validate_batch_size(request_count, MAX_BATCH_SIZE).is_err() {
            panic_with_error!(&env, BatchTransferError::BatchTooLarge);
        }

        // Reject duplicate recipients before emitting events or moving funds.
        if validate_unique_recipients(&env, &transfers).is_err() {
            panic_with_error!(&env, BatchTransferError::DuplicateRecipient);
        }

        if let Err(error) = Self::ensure_budget_headroom(&env, request_count) {
            panic_with_error!(&env, error);
        }

        // Get batch ID and increment
        let batch_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalBatches)
            .unwrap_or(0)
            + 1;

        // Emit batch started event
        TransferEvents::batch_started(&env, batch_id, request_count);

        // Initialize result vectors
        let mut results: Vec<TransferResult> = Vec::new(&env);
        let mut shared_results: Vec<BatchItemResult> = Vec::new(&env);
        let mut successful_count: u32 = 0;
        let mut failed_count: u32 = 0;
        let mut total_transferred: i128 = 0;

        // Create token client
        let token_client = token::Client::new(&env, &token);

        // Get initial balance
        let mut available_balance = token_client.balance(&caller);

        // Calculate total needed for all valid transfers and validate upfront
        let mut total_needed: i128 = 0;
        let mut validated_requests: Vec<(TransferRequest, bool, u32)> = Vec::new(&env);

        // First pass: Validate all requests and calculate total needed
        for request in transfers.iter() {
            let mut is_valid = true;
            let mut error_code = 0u32;

            // Validate recipient address
            if validate_address(&env, &request.recipient).is_err() {
                is_valid = false;
                error_code = 0; // Invalid address
            }
            // Validate amount
            else if validate_amount(request.amount).is_err() {
                is_valid = false;
                error_code = 1; // Invalid amount
            }

            if is_valid {
                total_needed = total_needed
                    .checked_add(request.amount)
                    .unwrap_or(i128::MAX);
            }

            validated_requests.push_back((request.clone(), is_valid, error_code));
        }

        // Second pass: Process each request
        for (request, is_valid, error_code) in validated_requests.iter() {
            if !is_valid {
                // Validation failed - record and continue
                results.push_back(TransferResult::Failure(
                    request.recipient.clone(),
                    request.amount,
                    error_code.clone(),
                ));
                shared_results.push_back(BatchItemResult {
                    success: false,
                    target: request.recipient.clone(),
                    amount: request.amount,
                    error_code: error_code.clone(),
                });
                failed_count += 1;
                TransferEvents::transfer_failure(
                    &env,
                    batch_id,
                    &request.recipient,
                    request.amount,
                    error_code.clone(),
                );
                continue;
            }

            // Check balance for this transfer
            if available_balance < request.amount {
                // Insufficient balance
                results.push_back(TransferResult::Failure(
                    request.recipient.clone(),
                    request.amount,
                    2, // Insufficient balance
                ));
                shared_results.push_back(BatchItemResult {
                    success: false,
                    target: request.recipient.clone(),
                    amount: request.amount,
                    error_code: 2,
                });
                failed_count += 1;
                TransferEvents::transfer_failure(
                    &env,
                    batch_id,
                    &request.recipient,
                    request.amount,
                    2,
                );
                continue;
            }

            // Execute transfer
            // Note: After thorough validation, transfers should succeed.
            // If a transfer fails due to contract-level issues (authorization, etc.),
            // it will panic and revert the entire batch. This is acceptable as
            // we've validated all inputs and balances.
            token_client.transfer(&caller, &request.recipient, &request.amount);

            // Transfer succeeded
            available_balance -= request.amount;
            results.push_back(TransferResult::Success(
                request.recipient.clone(),
                request.amount,
            ));
            shared_results.push_back(BatchItemResult {
                success: true,
                target: request.recipient.clone(),
                amount: request.amount,
                error_code: 0,
            });
            successful_count += 1;
            total_transferred = total_transferred
                .checked_add(request.amount)
                .unwrap_or(total_transferred);

            TransferEvents::transfer_success(&env, batch_id, &request.recipient, request.amount);
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
            .get(&DataKey::TotalTransfersProcessed)
            .unwrap_or(0);
        let total_volume: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalVolumeTransferred)
            .unwrap_or(0);

        env.storage()
            .instance()
            .set(&DataKey::TotalBatches, &(total_batches + 1));
        env.storage().instance().set(
            &DataKey::TotalTransfersProcessed,
            &(total_processed + request_count as u64),
        );
        env.storage().instance().set(
            &DataKey::TotalVolumeTransferred,
            &total_transferred
                .checked_add(total_volume)
                .unwrap_or(i128::MAX),
        );

        // Emit batch completed event
        TransferEvents::batch_completed(
            &env,
            batch_id,
            successful_count,
            failed_count,
            total_transferred,
        );

        BatchTransferResult {
            total_requests: request_count,
            successful: successful_count,
            failed: failed_count,
            total_transferred,
            results,
            shared_results,
        }
    }

    pub fn batch_burn(
        env: Env,
        caller: Address,
        token: Address,
        burns: Vec<BurnRequest>,
    ) -> BatchBurnResult {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        // Validate batch is not empty (early validation for efficiency)
        if validate_batch_not_empty(&burns).is_err() {
            panic_with_error!(&env, BatchTransferError::EmptyBatch);
        }

        let request_count = burns.len();
        if validate_batch_size(request_count, MAX_BATCH_SIZE).is_err() {
            panic_with_error!(&env, BatchTransferError::BatchTooLarge);
        }

        let batch_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalBatches)
            .unwrap_or(0)
            + 1;

        TransferEvents::batch_started(&env, batch_id, request_count);

        let token_client = token::Client::new(&env, &token);

        let mut results: Vec<BurnResult> = Vec::new(&env);
        let mut shared_results: Vec<BatchItemResult> = Vec::new(&env);
        let mut successful_count: u32 = 0;
        let mut failed_count: u32 = 0;
        let mut total_burned: i128 = 0;

        for request in burns.iter() {
            let mut is_valid = true;
            let mut error_code = 0u32;

            if validate_address(&env, &request.owner).is_err() {
                is_valid = false;
                error_code = 0;
            } else if validate_amount(request.amount).is_err() {
                is_valid = false;
                error_code = 1;
            }

            if !is_valid {
                results.push_back(BurnResult::Failure(
                    request.owner.clone(),
                    request.amount,
                    error_code,
                ));
                shared_results.push_back(BatchItemResult {
                    success: false,
                    target: request.owner.clone(),
                    amount: request.amount,
                    error_code,
                });
                failed_count += 1;
                TransferEvents::burn_failure(
                    &env,
                    batch_id,
                    &request.owner,
                    request.amount,
                    error_code,
                );
                continue;
            }

            let balance = token_client.balance(&request.owner);
            if balance < request.amount {
                results.push_back(BurnResult::Failure(
                    request.owner.clone(),
                    request.amount,
                    2,
                ));
                shared_results.push_back(BatchItemResult {
                    success: false,
                    target: request.owner.clone(),
                    amount: request.amount,
                    error_code: 2,
                });
                failed_count += 1;
                TransferEvents::burn_failure(&env, batch_id, &request.owner, request.amount, 2);
                continue;
            }

            request.owner.require_auth();
            token_client.burn(&request.owner, &request.amount);

            results.push_back(BurnResult::Success(request.owner.clone(), request.amount));
            shared_results.push_back(BatchItemResult {
                success: true,
                target: request.owner.clone(),
                amount: request.amount,
                error_code: 0,
            });
            successful_count += 1;
            total_burned = total_burned
                .checked_add(request.amount)
                .unwrap_or(total_burned);

            TransferEvents::burn_success(&env, batch_id, &request.owner, request.amount);
        }

        TransferEvents::burn_batch_completed(
            &env,
            batch_id,
            successful_count,
            failed_count,
            total_burned,
        );

        BatchBurnResult {
            total_requests: request_count,
            successful: successful_count,
            failed: failed_count,
            total_burned,
            results,
            shared_results,
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

    /// Returns the total number of transfers processed (successful + failed).
    pub fn get_total_transfers_processed(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalTransfersProcessed)
            .unwrap_or(0)
    }

    /// Returns the total volume transferred (in stroops).
    pub fn get_total_volume_transferred(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalVolumeTransferred)
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
            panic_with_error!(env, BatchTransferError::Unauthorized);
        }
    }

    fn ensure_budget_headroom(env: &Env, request_count: u32) -> Result<(), BatchTransferError> {
        if request_count as u64 * 2_000u64 > 100_000u64 {
            return Err(BatchTransferError::InstructionBudgetExceeded);
        }

        let _ = env.budget().cpu_instruction_count();
        Ok(())
    }
}

#[cfg(test)]
mod test;

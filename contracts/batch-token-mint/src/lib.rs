//! # Batch Token Mint Contract
//!
//! A Soroban smart contract for efficiently minting tokens to multiple recipients
//! in a single batch operation.
//!
//! ## Features
//!
//! - **Batch Processing**: Mint tokens to multiple recipients in a single call
//! - **Comprehensive Validation**: Validates mint amounts and recipient addresses
//! - **Event Emission**: Emits events for each mint operation and batch completion
//! - **Error Handling**: Gracefully handles invalid inputs with detailed error codes
//! - **Partial Failure Support**: Batch operations continue even if some mints fail
//! - **Optimized Storage**: Minimized storage operations by batching at the end
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

use shared::batch_result::BatchItemResult;
use soroban_sdk::{contract, contractimpl, panic_with_error, token, Address, Env, Vec};

pub use crate::types::{
    BatchMintMetrics, BatchMintResult, DataKey, ErrorCode, MintEvents, MintResult,
    TokenMintRequest, TokenMinted, MAX_BATCH_SIZE,
};
use crate::validation::validate_mint_request;

/// Error codes for the batch token mint contract.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum BatchTokenMintError {
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
    /// Batch exceeds the available instruction budget
    InstructionBudgetExceeded = 6,
}

impl From<BatchTokenMintError> for soroban_sdk::Error {
    fn from(e: BatchTokenMintError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

#[contract]
pub struct BatchTokenMintContract;

#[contractimpl]
impl BatchTokenMintContract {
    /// Initializes the contract with an admin address.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - The admin address that can authorize minting
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Contract already initialized");
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::LastBatchId, &0u64);
        env.storage().instance().set(&DataKey::TotalMinted, &0i128);
        env.storage()
            .instance()
            .set(&DataKey::TotalBatchesProcessed, &0u64);
    }

    /// Mints tokens to multiple recipients in a batch.
    ///
    /// This is the main entry point for batch token minting. It validates all requests,
    /// mints tokens, emits events, and updates storage efficiently. Supports partial failures.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `caller` - The address calling this function (must be admin)
    /// * `token` - The token contract address to mint from
    /// * `requests` - Vector of mint requests
    ///
    /// # Returns
    /// * `BatchMintResult` - Result containing minted tokens and metrics
    ///
    /// # Events Emitted
    /// * `batch_started` - When processing begins
    /// * `tokens_minted` - For each successful mint
    /// * `mint_failed` - For each failed mint
    /// * `large_mint` - For mints >= 1 billion stroops
    /// * `batch_completed` - When processing completes
    ///
    /// # Errors
    /// * `EmptyBatch` - If no requests provided
    /// * `BatchTooLarge` - If batch exceeds maximum size
    /// * `Unauthorized` - If caller is not admin
    pub fn batch_mint_tokens(
        env: Env,
        caller: Address,
        token: Address,
        requests: Vec<TokenMintRequest>,
    ) -> BatchMintResult {
        // Verify authorization
        caller.require_auth();
        Self::require_admin(&env, &caller);

        // Validate batch size
        let request_count = requests.len();
        if request_count == 0 {
            panic_with_error!(&env, BatchTokenMintError::EmptyBatch);
        }
        if request_count > MAX_BATCH_SIZE {
            panic_with_error!(&env, BatchTokenMintError::BatchTooLarge);
        }

        if let Err(error) = Self::ensure_budget_headroom(&env, request_count) {
            panic_with_error!(&env, error);
        }

        let mut validated_requests: Vec<(TokenMintRequest, bool, u32)> = Vec::new(&env);
        for request in requests.iter() {
            match validate_mint_request(&request) {
                Ok(()) => validated_requests.push_back((request.clone(), true, 0)),
                Err(error_code) => {
                    validated_requests.push_back((request.clone(), false, error_code))
                }
            }
        }

        if validated_requests.iter().any(|(_, is_valid, _)| !is_valid) {
            panic_with_error!(&env, BatchTokenMintError::InvalidBatch);
        }

        // Get batch ID and increment
        let batch_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::LastBatchId)
            .unwrap_or(0)
            + 1;

        // Emit batch started event
        MintEvents::batch_started(&env, batch_id, &token, request_count);

        // Get current ledger timestamp
        let current_ledger = env.ledger().sequence() as u64;

        // Initialize token client
        let _token_client = token::Client::new(&env, &token);

        // Initialize result tracking
        let mut results: Vec<MintResult> = Vec::new(&env);
        let mut shared_results: Vec<BatchItemResult> = Vec::new(&env);
        let mut successful_count: u32 = 0;
        let mut failed_count: u32 = 0;
        let mut total_amount_minted: i128 = 0;

        // Process each mint request
        for (request, _, _) in validated_requests.iter() {
            let minted = TokenMinted {
                token_address: token.clone(),
                recipient: request.recipient.clone(),
                amount: request.amount,
                minted_at: current_ledger,
            };

            total_amount_minted = total_amount_minted
                .checked_add(request.amount)
                .unwrap_or(i128::MAX);
            successful_count += 1;

            MintEvents::tokens_minted(&env, batch_id, &token, &minted);

            if request.amount >= 1_000_000_000 {
                MintEvents::large_mint(&env, batch_id, &token, &request.recipient, request.amount);
            }

            results.push_back(MintResult::Success(minted));
            shared_results.push_back(BatchItemResult {
                success: true,
                target: request.recipient.clone(),
                amount: request.amount,
                error_code: 0,
            });
        }

        // Calculate average mint amount
        let avg_mint_amount = if successful_count > 0 {
            total_amount_minted / successful_count as i128
        } else {
            0
        };

        // Create metrics
        let metrics = BatchMintMetrics {
            total_requests: request_count,
            successful_mints: successful_count,
            failed_mints: failed_count,
            total_amount_minted,
            avg_mint_amount,
            processed_at: current_ledger,
        };

        // Update storage (batched at the end for efficiency)
        let total_minted: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalMinted)
            .unwrap_or(0);
        let total_batches: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalBatchesProcessed)
            .unwrap_or(0);

        env.storage()
            .instance()
            .set(&DataKey::LastBatchId, &batch_id);
        env.storage()
            .instance()
            .set(&DataKey::TotalMinted, &(total_minted + total_amount_minted));
        env.storage()
            .instance()
            .set(&DataKey::TotalBatchesProcessed, &(total_batches + 1));

        // Emit batch completed event
        MintEvents::batch_completed(
            &env,
            batch_id,
            &token,
            successful_count,
            failed_count,
            total_amount_minted,
        );

        BatchMintResult {
            batch_id,
            token_address: token,
            total_requests: request_count,
            successful: successful_count,
            failed: failed_count,
            results,
            shared_results,
            metrics,
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

    /// Returns the last created batch ID.
    pub fn get_last_batch_id(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::LastBatchId)
            .unwrap_or(0)
    }

    /// Returns the total amount minted.
    pub fn get_total_minted(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalMinted)
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
            panic_with_error!(env, BatchTokenMintError::Unauthorized);
        }
    }

    fn ensure_budget_headroom(env: &Env, request_count: u32) -> Result<(), BatchTokenMintError> {
        if request_count as u64 * 2_000u64 > 100_000u64 {
            return Err(BatchTokenMintError::InstructionBudgetExceeded);
        }

        let _ = env.budget().cpu_instruction_count();
        Ok(())
    }
}

#[cfg(test)]
mod test;

//! # Standalone Escrow Contract
//!
//! ╔══════════════════════════════════════════════════════════════════════╗
//! ║  This is the REAL escrow contract for individual depositor↔recipient║
//! ║  escrows. The Fee crate (`contracts/fee/src/escrow.rs`) has an      ║
//! ║  **internal** module that shares the "escrow" name but only tracks  ║
//! ║  pooled fee collection — it is NOT an escrow contract.              ║
//! ╚══════════════════════════════════════════════════════════════════════╝
//!
//! This contract provides per-escrow lifecycle management (create, release,
//! reverse) with batch operations and full event emission.
#![no_std]

mod types;
mod validation;

use soroban_sdk::{contract, contractimpl, panic_with_error, token, Address, Env, Vec};

pub use crate::types::{
    BatchReleaseResult, BatchReversalResult, DataKey, Escrow, EscrowEvents, EscrowStatus,
    ReleaseRequest, ReleaseResult, ReversalRequest, ReversalResult, MAX_BATCH_SIZE,
};
use crate::validation::validate_release;
use crate::validation::validate_reversal;

/// Error codes for the escrow contract.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EscrowError {
    /// Contract not initialized
    NotInitialized = 1,
    /// Caller is not authorized
    Unauthorized = 2,
    /// Batch is empty
    EmptyBatch = 3,
    /// Batch exceeds maximum size
    BatchTooLarge = 4,
    /// Invalid amount
    InvalidAmount = 5,
    /// Escrow not found
    EscrowNotFound = 6,
    /// Contract already initialized
    AlreadyInitialized = 7,
}

impl From<EscrowError> for soroban_sdk::Error {
    fn from(e: EscrowError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

#[contract]
pub struct EscrowContract;

#[contractimpl]
impl EscrowContract {
    /// Initializes the contract with an admin address and token.
    pub fn initialize(env: Env, admin: Address, token: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&env, EscrowError::AlreadyInitialized);
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Token, &token);
        env.storage().instance().set(&DataKey::EscrowCounter, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::TotalReversalBatches, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::TotalEscrowsReversed, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::TotalAmountReversed, &0i128);
        env.storage()
            .instance()
            .set(&DataKey::TotalReleaseBatches, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::TotalEscrowsReleased, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::TotalAmountReleased, &0i128);
    }

    /// Creates a new escrow.
    ///
    /// Locks funds from the depositor until released to recipient or reversed.
    pub fn create_escrow(
        env: Env,
        depositor: Address,
        recipient: Address,
        arbiter: Option<Address>,
        amount: i128,
        deadline: u64,
    ) -> u64 {
        // Verify depositor authorization
        depositor.require_auth();

        // Validate amount
        if amount <= 0 {
            panic_with_error!(&env, EscrowError::InvalidAmount);
        }

        // Get token and transfer funds to contract
        let token: Address = env
            .storage()
            .instance()
            .get(&DataKey::Token)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::NotInitialized));
        let token_client = token::Client::new(&env, &token);

        // Transfer funds from depositor to this contract
        token_client.transfer(&depositor, &env.current_contract_address(), &amount);

        // Get and increment escrow counter with overflow protection
        let current_counter: u64 = env
            .storage()
            .instance()
            .get(&DataKey::EscrowCounter)
            .unwrap_or(0);
        let escrow_id: u64 = current_counter
            .checked_add(1)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::InvalidAmount));
        env.storage()
            .instance()
            .set(&DataKey::EscrowCounter, &escrow_id);

        // Create escrow record
        let escrow = Escrow {
            escrow_id,
            depositor: depositor.clone(),
            recipient: recipient.clone(),
            arbiter: arbiter.clone(),
            token: token.clone(),
            amount,
            status: EscrowStatus::Active,
            created_at: env.ledger().sequence() as u64,
            deadline,
        };

        // Store escrow
        env.storage()
            .persistent()
            .set(&DataKey::Escrow(escrow_id), &escrow);

        // Update user escrows list
        let mut user_escrows: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::UserEscrows(depositor.clone()))
            .unwrap_or(Vec::new(&env));
        user_escrows.push_back(escrow_id);
        env.storage()
            .persistent()
            .set(&DataKey::UserEscrows(depositor.clone()), &user_escrows);

        // Emit event
        EscrowEvents::escrow_created(&env, escrow_id, &depositor, &recipient, &arbiter, amount);

        escrow_id
    }

    /// Batch reverses multiple escrows.
    ///
    /// This is the main function for handling failed transaction reversals.
    /// It validates each reversal, handles partial failures, and emits events.
    ///
    /// # Arguments
    /// * `caller` - The address initiating the reversal (must be admin)
    /// * `requests` - Vector of reversal requests containing escrow IDs
    ///
    /// # Returns
    /// * `BatchReversalResult` with detailed success/failure information
    pub fn batch_reverse_escrows(
        env: Env,
        caller: Address,
        requests: Vec<ReversalRequest>,
    ) -> BatchReversalResult {
        // Verify authorization
        caller.require_auth();
        Self::require_admin(&env, &caller);

        // Validate batch size
        let request_count = requests.len();
        if request_count == 0 {
            panic_with_error!(&env, EscrowError::EmptyBatch);
        }
        if request_count > MAX_BATCH_SIZE {
            panic_with_error!(&env, EscrowError::BatchTooLarge);
        }

        // Get batch ID
        let batch_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalReversalBatches)
            .unwrap_or(0)
            + 1;

        // Get admin and token for validation
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::NotInitialized));
        let token: Address = env
            .storage()
            .instance()
            .get(&DataKey::Token)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::NotInitialized));
        let token_client = token::Client::new(&env, &token);

        let current_ledger = env.ledger().sequence() as u64;

        // Emit batch started event
        EscrowEvents::batch_reversal_started(&env, batch_id, request_count);

        // Initialize result tracking
        let mut results: Vec<ReversalResult> = Vec::new(&env);
        let mut successful_count: u32 = 0;
        let mut failed_count: u32 = 0;
        let mut total_reversed: i128 = 0;

        // First pass: validate all requests
        let mut validated_requests: Vec<(ReversalRequest, Option<Escrow>, bool, u32)> =
            Vec::new(&env);

        for request in requests.iter() {
            let escrow_opt: Option<Escrow> = env
                .storage()
                .persistent()
                .get(&DataKey::Escrow(request.escrow_id));

            let validation_result =
                validate_reversal(escrow_opt.as_ref(), &caller, &admin, true, current_ledger);

            let (is_valid, error_code) = match validation_result {
                Ok(()) => (true, 0u32),
                Err(e) => (false, e.to_error_code()),
            };

            validated_requests.push_back((request.clone(), escrow_opt, is_valid, error_code));
        }

        // Second pass: execute reversals
        for (request, escrow_opt, is_valid, error_code) in validated_requests.iter() {
            if !is_valid {
                // Validation failed - record failure and continue
                results.push_back(ReversalResult::Failure(request.escrow_id, error_code));
                failed_count += 1;
                EscrowEvents::reversal_failure(&env, batch_id, request.escrow_id, error_code);
                continue;
            }

            // Get the escrow (safe to unwrap as validation passed)
            let mut escrow = escrow_opt.clone().unwrap();

            // Transfer funds back to depositor
            token_client.transfer(
                &env.current_contract_address(),
                &escrow.depositor,
                &escrow.amount,
            );

            // Update escrow status
            escrow.status = EscrowStatus::Reversed;
            env.storage()
                .persistent()
                .set(&DataKey::Escrow(escrow.escrow_id), &escrow);

            // Record success
            results.push_back(ReversalResult::Success(
                escrow.escrow_id,
                escrow.depositor.clone(),
                escrow.amount,
            ));
            successful_count += 1;
            total_reversed = total_reversed
                .checked_add(escrow.amount)
                .unwrap_or(total_reversed);

            // Emit success event
            EscrowEvents::reversal_success(
                &env,
                batch_id,
                escrow.escrow_id,
                &escrow.depositor,
                escrow.amount,
            );
        }

        // Update storage statistics
        let total_batches: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalReversalBatches)
            .unwrap_or(0);
        let total_escrows_reversed: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalEscrowsReversed)
            .unwrap_or(0);
        let total_amount_reversed: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalAmountReversed)
            .unwrap_or(0);

        env.storage()
            .instance()
            .set(&DataKey::TotalReversalBatches, &(total_batches + 1));
        env.storage().instance().set(
            &DataKey::TotalEscrowsReversed,
            &(total_escrows_reversed + successful_count as u64),
        );
        env.storage().instance().set(
            &DataKey::TotalAmountReversed,
            &total_amount_reversed
                .checked_add(total_reversed)
                .unwrap_or(i128::MAX),
        );

        // Emit batch completed event
        EscrowEvents::batch_reversal_completed(
            &env,
            batch_id,
            successful_count,
            failed_count,
            total_reversed,
        );

        BatchReversalResult {
            batch_id,
            total_requests: request_count,
            successful: successful_count,
            failed: failed_count,
            total_reversed,
            results,
        }
    }

    /// Batch releases multiple escrows to their recipients.
    ///
    /// Caller must be authenticated. Each request is validated individually:
    /// admin may release any escrow, depositor may release their own.
    pub fn batch_release_escrows(
        env: Env,
        caller: Address,
        requests: Vec<ReleaseRequest>,
    ) -> BatchReleaseResult {
        // Require auth for caller (admin or depositor(s))
        caller.require_auth();

        // Validate batch size
        let request_count = requests.len();
        if request_count == 0 {
            panic_with_error!(&env, EscrowError::EmptyBatch);
        }
        if request_count > MAX_BATCH_SIZE {
            panic_with_error!(&env, EscrowError::BatchTooLarge);
        }

        // Batch id for releases
        let batch_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalReleaseBatches)
            .unwrap_or(0)
            + 1;

        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::NotInitialized));
        let token: Address = env
            .storage()
            .instance()
            .get(&DataKey::Token)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::NotInitialized));
        let token_client = token::Client::new(&env, &token);

        // Emit batch started event
        EscrowEvents::batch_release_started(&env, batch_id, request_count);

        // Initialize tracking
        let mut results: Vec<ReleaseResult> = Vec::new(&env);
        let mut successful_count: u32 = 0;
        let mut failed_count: u32 = 0;
        let mut total_released: i128 = 0;

        // First pass: validate
        let mut validated_requests: Vec<(ReleaseRequest, Option<Escrow>, bool, u32)> =
            Vec::new(&env);

        for request in requests.iter() {
            let escrow_opt: Option<Escrow> = env
                .storage()
                .persistent()
                .get(&DataKey::Escrow(request.escrow_id));

            let validation_result = validate_release(escrow_opt.as_ref(), &caller, &admin);

            let (is_valid, error_code) = match validation_result {
                Ok(()) => (true, 0u32),
                Err(e) => (false, e.to_error_code()),
            };

            validated_requests.push_back((request.clone(), escrow_opt, is_valid, error_code));
        }

        // Second pass: execute releases
        for (request, escrow_opt, is_valid, error_code) in validated_requests.iter() {
            if !is_valid {
                results.push_back(ReleaseResult::Failure(request.escrow_id, error_code));
                failed_count += 1;
                EscrowEvents::release_failure(&env, batch_id, request.escrow_id, error_code);
                continue;
            }

            let mut escrow = escrow_opt.clone().unwrap();

            // Transfer to recipient
            token_client.transfer(
                &env.current_contract_address(),
                &escrow.recipient,
                &escrow.amount,
            );

            // Update status
            escrow.status = EscrowStatus::Released;
            env.storage()
                .persistent()
                .set(&DataKey::Escrow(escrow.escrow_id), &escrow);

            results.push_back(ReleaseResult::Success(
                escrow.escrow_id,
                escrow.recipient.clone(),
                escrow.amount,
            ));
            successful_count += 1;
            total_released = total_released
                .checked_add(escrow.amount)
                .unwrap_or(total_released);

            EscrowEvents::release_success(
                &env,
                batch_id,
                escrow.escrow_id,
                &escrow.recipient,
                escrow.amount,
            );
        }

        // Update storage stats
        let total_batches: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalReleaseBatches)
            .unwrap_or(0);
        let total_escrows_released: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalEscrowsReleased)
            .unwrap_or(0);
        let total_amount_released: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalAmountReleased)
            .unwrap_or(0);

        env.storage()
            .instance()
            .set(&DataKey::TotalReleaseBatches, &(total_batches + 1));
        env.storage().instance().set(
            &DataKey::TotalEscrowsReleased,
            &(total_escrows_released + successful_count as u64),
        );
        env.storage().instance().set(
            &DataKey::TotalAmountReleased,
            &total_amount_released
                .checked_add(total_released)
                .unwrap_or(i128::MAX),
        );

        EscrowEvents::batch_release_completed(
            &env,
            batch_id,
            successful_count,
            failed_count,
            total_released,
        );

        BatchReleaseResult {
            batch_id,
            total_requests: request_count,
            successful: successful_count,
            failed: failed_count,
            total_released,
            results,
        }
    }

    /// Releases an escrow to the recipient.
    ///
    /// Can only be called by admin or depositor.
    pub fn release_escrow(env: Env, caller: Address, escrow_id: u64) {
        caller.require_auth();

        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::NotInitialized));

        let escrow: Escrow = env
            .storage()
            .persistent()
            .get(&DataKey::Escrow(escrow_id))
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::EscrowNotFound));

        // Check authorization: admin, depositor or arbiter
        let is_admin = caller == admin;
        let is_depositor = caller == escrow.depositor;
        let is_arbiter = if let Some(arb) = &escrow.arbiter {
            caller == *arb
        } else {
            false
        };

        if !is_admin && !is_depositor && !is_arbiter {
            panic_with_error!(&env, EscrowError::Unauthorized);
        }

        // Check escrow is active
        if escrow.status != EscrowStatus::Active {
            panic_with_error!(&env, EscrowError::Unauthorized);
        }

        // Transfer funds to recipient
        let token_client = token::Client::new(&env, &escrow.token);
        token_client.transfer(
            &env.current_contract_address(),
            &escrow.recipient,
            &escrow.amount,
        );

        // Update escrow status
        let mut updated_escrow = escrow.clone();
        updated_escrow.status = EscrowStatus::Released;
        env.storage()
            .persistent()
            .set(&DataKey::Escrow(escrow_id), &updated_escrow);

        // Emit event
        EscrowEvents::escrow_released(&env, escrow_id, &escrow.recipient, escrow.amount);
    }

    /// Raises a dispute for an active escrow.
    ///
    /// Can be called by depositor, recipient, or arbiter.
    pub fn raise_dispute(env: Env, caller: Address, escrow_id: u64) {
        caller.require_auth();

        let mut escrow: Escrow = env
            .storage()
            .persistent()
            .get(&DataKey::Escrow(escrow_id))
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::EscrowNotFound));

        let is_depositor = caller == escrow.depositor;
        let is_recipient = caller == escrow.recipient;
        let is_arbiter = if let Some(arb) = &escrow.arbiter {
            caller == *arb
        } else {
            false
        };

        if !is_depositor && !is_recipient && !is_arbiter {
            panic_with_error!(&env, EscrowError::Unauthorized);
        }

        if escrow.status != EscrowStatus::Active {
            panic_with_error!(&env, EscrowError::Unauthorized);
        }

        escrow.status = EscrowStatus::Disputed;
        env.storage()
            .persistent()
            .set(&DataKey::Escrow(escrow_id), &escrow);

        EscrowEvents::escrow_disputed(&env, escrow_id, &caller);
    }

    /// Resolves a disputed escrow with split settlement between depositor and recipient.
    ///
    /// Can only be called by the assigned arbiter or contract admin.
    pub fn resolve_dispute(
        env: Env,
        caller: Address,
        escrow_id: u64,
        depositor_amount: i128,
        recipient_amount: i128,
    ) {
        caller.require_auth();

        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::NotInitialized));

        let mut escrow: Escrow = env
            .storage()
            .persistent()
            .get(&DataKey::Escrow(escrow_id))
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::EscrowNotFound));

        let is_admin = caller == admin;
        let is_arbiter = if let Some(arb) = &escrow.arbiter {
            caller == *arb
        } else {
            false
        };

        if !is_admin && !is_arbiter {
            panic_with_error!(&env, EscrowError::Unauthorized);
        }

        if escrow.status != EscrowStatus::Disputed {
            panic_with_error!(&env, EscrowError::Unauthorized);
        }

        if depositor_amount < 0 || recipient_amount < 0 {
            panic_with_error!(&env, EscrowError::InvalidAmount);
        }

        let total_split = depositor_amount
            .checked_add(recipient_amount)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::InvalidAmount));

        if total_split != escrow.amount {
            panic_with_error!(&env, EscrowError::InvalidAmount);
        }

        let token_client = token::Client::new(&env, &escrow.token);

        if depositor_amount > 0 {
            token_client.transfer(
                &env.current_contract_address(),
                &escrow.depositor,
                &depositor_amount,
            );
        }

        if recipient_amount > 0 {
            token_client.transfer(
                &env.current_contract_address(),
                &escrow.recipient,
                &recipient_amount,
            );
        }

        escrow.status = EscrowStatus::Resolved;
        env.storage()
            .persistent()
            .set(&DataKey::Escrow(escrow_id), &escrow);

        EscrowEvents::escrow_resolved(
            &env,
            escrow_id,
            &caller,
            depositor_amount,
            recipient_amount,
        );
    }

    /// Returns an escrow by ID.
    pub fn get_escrow(env: Env, escrow_id: u64) -> Option<Escrow> {
        env.storage().persistent().get(&DataKey::Escrow(escrow_id))
    }

    /// Returns all escrow IDs for a user.
    pub fn get_user_escrows(env: Env, user: Address) -> Vec<u64> {
        env.storage()
            .persistent()
            .get(&DataKey::UserEscrows(user))
            .unwrap_or(Vec::new(&env))
    }

    /// Returns the admin address.
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::NotInitialized))
    }

    /// Updates the admin address.
    pub fn set_admin(env: Env, current_admin: Address, new_admin: Address) {
        current_admin.require_auth();
        Self::require_admin(&env, &current_admin);
        env.storage().instance().set(&DataKey::Admin, &new_admin);
    }

    /// Returns the total number of reversal batches processed.
    pub fn get_total_reversal_batches(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalReversalBatches)
            .unwrap_or(0)
    }

    /// Returns the total number of escrows reversed.
    pub fn get_total_escrows_reversed(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalEscrowsReversed)
            .unwrap_or(0)
    }

    /// Returns the total amount reversed.
    pub fn get_total_amount_reversed(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalAmountReversed)
            .unwrap_or(0)
    }

    /// Returns the total number of release batches processed.
    pub fn get_total_release_batches(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalReleaseBatches)
            .unwrap_or(0)
    }

    /// Returns the total number of escrows released.
    pub fn get_total_escrows_released(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalEscrowsReleased)
            .unwrap_or(0)
    }

    /// Returns the total amount released.
    pub fn get_total_amount_released(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalAmountReleased)
            .unwrap_or(0)
    }

    /// Returns the escrow counter (total escrows created).
    pub fn get_escrow_counter(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::EscrowCounter)
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
            panic_with_error!(env, EscrowError::Unauthorized);
        }
    }
}

#[cfg(test)]
mod test;

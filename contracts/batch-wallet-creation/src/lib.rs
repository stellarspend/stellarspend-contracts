//! # Batch Wallet Creation Contract
#![no_std]

mod types;
mod validation;

use soroban_sdk::{
    contract, contractimpl, panic_with_error, Address, Env, Vec,
};

pub use crate::types::{
    BatchCreateResult, DataKey, Wallet, WalletCreateRequest, WalletCreateResult, WalletEvents, MAX_BATCH_SIZE,
};
use crate::validation::{validate_address, wallet_exists};

/// Error codes for the batch wallet creation contract.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum BatchWalletError {
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

impl From<BatchWalletError> for soroban_sdk::Error {
    fn from(e: BatchWalletError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

#[contract]
pub struct BatchWalletContract;

#[contractimpl]
impl BatchWalletContract {
    /// Initializes the contract with an admin address.
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Contract already initialized");
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::TotalBatches, &0u64);
        env.storage().instance().set(&DataKey::TotalWalletsCreated, &0u64);
    }

    /// Executes batch creation of wallets for multiple owners.
    pub fn batch_create_wallets(
        env: Env,
        caller: Address,
        requests: Vec<WalletCreateRequest>,
    ) -> BatchCreateResult {
        // Verify authorization
        caller.require_auth();
        Self::require_admin(&env, &caller);

        // Validate batch size
        let request_count = requests.len();
        if request_count == 0 {
            panic_with_error!(&env, BatchWalletError::EmptyBatch);
        }
        if request_count > MAX_BATCH_SIZE {
            panic_with_error!(&env, BatchWalletError::BatchTooLarge);
        }

        // Get batch ID and increment
        let batch_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalBatches)
            .unwrap_or(0)
            + 1;

        // Emit batch started event
        WalletEvents::batch_started(&env, batch_id, request_count);

        // Initialize result vectors
        let mut results: Vec<WalletCreateResult> = Vec::new(&env);
        let mut successful_count: u32 = 0;
        let mut failed_count: u32 = 0;

        // Get current total wallets for ID assignment
        let mut next_wallet_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalWalletsCreated)
            .unwrap_or(0) + 1;

        // Process each request
        for request in requests.iter() {
            let mut is_valid = true;
            let mut error_code = 0u32;

            // Validate owner address
            if validate_address(&request.owner).is_err() {
                is_valid = false;
                error_code = 0; // Invalid address
            }
            // Check if wallet already exists
            else if wallet_exists(&env, &request.owner) {
                is_valid = false;
                error_code = 1; // Wallet already exists
            }

            if !is_valid {
                // Validation failed - record and continue
                results.push_back(WalletCreateResult::Failure(
                    request.owner.clone(),
                    error_code,
                ));
                failed_count += 1;
                WalletEvents::wallet_creation_failure(
                    &env,
                    batch_id,
                    &request.owner,
                    error_code,
                );
                continue;
            }

            // Create wallet
            let wallet = Wallet {
                id: next_wallet_id,
                owner: request.owner.clone(),
                created_at: env.ledger().timestamp(),
            };

            // Store wallet
            env.storage().persistent().set(&DataKey::Wallets(request.owner.clone()), &wallet);

            // Increment ID
            next_wallet_id += 1;

            // Record success
            results.push_back(WalletCreateResult::Success(request.owner.clone()));
            successful_count += 1;

            WalletEvents::wallet_created(
                &env,
                batch_id,
                &request.owner,
                wallet.id,
            );
        }

        // Update storage
        let total_batches: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalBatches)
            .unwrap_or(0);
        let total_created: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalWalletsCreated)
            .unwrap_or(0);

        env.storage()
            .instance()
            .set(&DataKey::TotalBatches, &(total_batches + 1));
        env.storage()
            .instance()
            .set(&DataKey::TotalWalletsCreated, &(total_created + successful_count as u64));

        // Emit batch completed event
        WalletEvents::batch_completed(
            &env,
            batch_id,
            successful_count,
            failed_count,
        );

        BatchCreateResult {
            total_requests: request_count,
            successful: successful_count,
            failed: failed_count,
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

    /// Returns the total number of wallets created.
    pub fn get_total_wallets_created(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalWalletsCreated)
            .unwrap_or(0)
    }

    /// Returns wallet information for a given address.
    pub fn get_wallet(env: Env, address: Address) -> Option<Wallet> {
        env.storage().persistent().get(&DataKey::Wallets(address))
    }

    // Internal helper to verify admin
    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Contract not initialized");

        if *caller != admin {
            panic_with_error!(env, BatchWalletError::Unauthorized);
        }
    }
}

#[cfg(test)]
mod test;
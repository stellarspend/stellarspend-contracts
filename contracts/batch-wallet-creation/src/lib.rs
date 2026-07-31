//! # Batch Wallet Creation Contract
#![no_std]

mod types;
mod validation;

use soroban_sdk::{contract, contractimpl, panic_with_error, Address, Env, Vec};

pub use crate::types::{
    BatchCreateResult, BatchRecoveryResult, DataKey, Wallet, WalletCreateRequest,
    WalletCreateResult, WalletCreatedEvent, WalletEvents, WalletRecoveryRequest,
    WalletRecoveryResult, MAX_BATCH_SIZE,
};
use crate::validation::{check_batch_duplicates, validate_address, wallet_exists};

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
    /// Batch contains duplicate wallet requests
    DuplicateWallet = 6,
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
        env.storage()
            .instance()
            .set(&DataKey::TotalWalletsCreated, &0u64);
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

        // Check for duplicate wallet requests in the batch
        if let Err(duplicate_address) = check_batch_duplicates(&requests) {
            // Emit event or log for debugging purposes
            env.events().publish(
                (
                    soroban_sdk::symbol_short!("wallet"),
                    soroban_sdk::symbol_short!("duplicate"),
                ),
                duplicate_address,
            );
            panic_with_error!(&env, BatchWalletError::DuplicateWallet);
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
        let mut shared_results: Vec<shared::batch_result::BatchItemResult> = Vec::new(&env);
        let mut successful_count: u32 = 0;
        let mut failed_count: u32 = 0;

        // Get current total wallets for ID assignment
        let mut next_wallet_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalWalletsCreated)
            .unwrap_or(0)
            + 1;

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
                shared_results.push_back(shared::batch_result::BatchItemResult {
                    success: false,
                    target: request.owner.clone(),
                    amount: 0,
                    error_code,
                });
                failed_count += 1;
                WalletEvents::wallet_creation_failure(&env, batch_id, &request.owner, error_code);
                continue;
            }

            // Create wallet
            let wallet = Wallet {
                id: next_wallet_id,
                owner: request.owner.clone(),
                created_at: env.ledger().timestamp(),
            };

            // Store wallet
            env.storage()
                .persistent()
                .set(&DataKey::Wallets(request.owner.clone()), &wallet);

            // Increment ID
            next_wallet_id += 1;

            // Record success
            results.push_back(WalletCreateResult::Success(request.owner.clone()));
            shared_results.push_back(shared::batch_result::BatchItemResult {
                success: true,
                target: request.owner.clone(),
                amount: 0,
                error_code: 0,
            });
            successful_count += 1;

            let wallet_event = WalletCreatedEvent {
                owner: request.owner.clone(),
                wallet_id: wallet.id,
                created_at: wallet.created_at,
            };
            WalletEvents::wallet_created(&env, batch_id, &wallet_event);
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
        env.storage().instance().set(
            &DataKey::TotalWalletsCreated,
            &(total_created + successful_count as u64),
        );

        // Emit batch completed event
        WalletEvents::batch_completed(&env, batch_id, successful_count, failed_count);

        BatchCreateResult {
            total_requests: request_count,
            successful: successful_count,
            failed: failed_count,
            results,
            shared_results,
        }
    }

    pub fn batch_recover_wallets(
        env: Env,
        caller: Address,
        requests: Vec<WalletRecoveryRequest>,
    ) -> BatchRecoveryResult {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let request_count = requests.len();
        if request_count == 0 {
            panic_with_error!(&env, BatchWalletError::EmptyBatch);
        }
        if request_count > MAX_BATCH_SIZE {
            panic_with_error!(&env, BatchWalletError::BatchTooLarge);
        }

        let batch_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalBatches)
            .unwrap_or(0)
            + 1;

        WalletEvents::recovery_started(&env, batch_id, request_count);

        let mut results: Vec<WalletRecoveryResult> = Vec::new(&env);
        let mut shared_results: Vec<shared::batch_result::BatchItemResult> = Vec::new(&env);
        let mut successful_count: u32 = 0;
        let mut failed_count: u32 = 0;

        for request in requests.iter() {
            let mut is_valid = true;
            let mut error_code = 0u32;

            if validate_address(&request.old_owner).is_err()
                || validate_address(&request.new_owner).is_err()
            {
                is_valid = false;
                error_code = 0;
            } else if !wallet_exists(&env, &request.old_owner) {
                is_valid = false;
                error_code = 1;
            } else if wallet_exists(&env, &request.new_owner) {
                is_valid = false;
                error_code = 2;
            }

            if !is_valid {
                results.push_back(WalletRecoveryResult::Failure(
                    request.old_owner.clone(),
                    request.new_owner.clone(),
                    error_code,
                ));
                shared_results.push_back(shared::batch_result::BatchItemResult {
                    success: false,
                    target: request.old_owner.clone(),
                    amount: 0,
                    error_code,
                });
                failed_count += 1;
                WalletEvents::wallet_recovery_failure(
                    &env,
                    batch_id,
                    &request.old_owner,
                    &request.new_owner,
                    error_code,
                );
                continue;
            }

            let mut wallet: Wallet = env
                .storage()
                .persistent()
                .get(&DataKey::Wallets(request.old_owner.clone()))
                .unwrap();
            wallet.owner = request.new_owner.clone();

            env.storage()
                .persistent()
                .set(&DataKey::Wallets(request.new_owner.clone()), &wallet);
            env.storage()
                .persistent()
                .remove(&DataKey::Wallets(request.old_owner.clone()));

            results.push_back(WalletRecoveryResult::Success(
                request.old_owner.clone(),
                request.new_owner.clone(),
            ));
            shared_results.push_back(shared::batch_result::BatchItemResult {
                success: true,
                target: request.old_owner.clone(),
                amount: 0,
                error_code: 0,
            });
            successful_count += 1;

            WalletEvents::wallet_recovered(
                &env,
                batch_id,
                &request.old_owner,
                &request.new_owner,
                wallet.id,
            );
        }

        let total_batches: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalBatches)
            .unwrap_or(0);

        env.storage()
            .instance()
            .set(&DataKey::TotalBatches, &(total_batches + 1));

        WalletEvents::recovery_completed(&env, batch_id, successful_count, failed_count);

        BatchRecoveryResult {
            total_requests: request_count,
            successful: successful_count,
            failed: failed_count,
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

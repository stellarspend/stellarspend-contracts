//! # Batch Rewards Distribution Contract
#![no_std]

mod types;
mod validation;

#[cfg(test)]
mod test;

use soroban_sdk::{contract, contractimpl, panic_with_error, token, Address, Env, Vec};

pub use crate::types::{
    BatchRewardResult, DataKey, RewardEvents, RewardRequest, RewardResult, MAX_BATCH_SIZE,
};
use crate::validation::{validate_address, validate_amount};

/// Error codes for the batch rewards contract.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum BatchRewardsError {
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
    /// Insufficient balance to distribute rewards
    InsufficientBalance = 7,
    /// Invalid reward amount
    InvalidAmount = 8,
}

impl From<BatchRewardsError> for soroban_sdk::Error {
    fn from(e: BatchRewardsError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

#[contract]
pub struct BatchRewardsContract;

#[contractimpl]
impl BatchRewardsContract {
    /// Initializes the contract with an admin address.
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Contract already initialized");
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::TotalBatches, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::TotalRewardsProcessed, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::TotalVolumeDistributed, &0i128);
    }

    /// Gets the contract admin.
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Contract not initialized")
    }

    /// Gets the total number of reward batches processed.
    pub fn get_total_batches(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalBatches)
            .unwrap_or(0)
    }

    /// Gets the total number of rewards processed.
    pub fn get_total_rewards_processed(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalRewardsProcessed)
            .unwrap_or(0)
    }

    /// Gets the total volume of rewards distributed.
    pub fn get_total_volume_distributed(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalVolumeDistributed)
            .unwrap_or(0)
    }

    /// Sets a new admin address.
    pub fn set_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        env.storage().instance().set(&DataKey::Admin, &new_admin);
        let topics = (soroban_sdk::symbol_short!("admin"),);
        env.events().publish(topics, (&new_admin,));
    }

    /// Distributes rewards to multiple recipients in a batch operation.
    /// 
    /// # Arguments
    /// * `env` - The Soroban environment
    /// * `caller` - The address initiating the batch rewards
    /// * `token` - The token contract address (e.g., XLM)
    /// * `rewards` - Vector of reward requests containing recipient and amount
    ///
    /// # Returns
    /// A `BatchRewardResult` containing the results of the distribution
    pub fn distribute_rewards(
        env: Env,
        caller: Address,
        token: Address,
        rewards: Vec<RewardRequest>,
    ) -> BatchRewardResult {
        // Verify authorization
        caller.require_auth();
        Self::require_admin(&env, &caller);

        // Validate batch size
        let request_count = rewards.len();
        if request_count == 0 {
            panic_with_error!(&env, BatchRewardsError::EmptyBatch);
        }
        if request_count > MAX_BATCH_SIZE {
            panic_with_error!(&env, BatchRewardsError::BatchTooLarge);
        }

        // Get batch ID and increment
        let batch_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalBatches)
            .unwrap_or(0)
            + 1;

        // Emit batch started event
        RewardEvents::batch_started(&env, batch_id, request_count);

        // Initialize result vectors
        let mut results: Vec<RewardResult> = Vec::new(&env);
        let mut successful_count: u32 = 0;
        let mut failed_count: u32 = 0;
        let mut total_distributed: i128 = 0;

        // Create token client
        let token_client = token::Client::new(&env, &token);

        // Get initial balance to ensure sufficient funds
        let available_balance = token_client.balance(&caller);
        let total_required: i128 = rewards.iter().fold(0i128, |sum, reward| {
            sum + reward.amount
        });

        if available_balance < total_required {
            panic_with_error!(&env, BatchRewardsError::InsufficientBalance);
        }

        // Process each reward request
        for reward in rewards.iter() {
            // Validate reward amount
            if let Err(_) = validate_amount(reward.amount) {
                failed_count += 1;
                let error_code = BatchRewardsError::InvalidAmount as u32;
                results.push_back(RewardResult::Failure(
                    reward.recipient.clone(),
                    reward.amount,
                    error_code,
                ));
                RewardEvents::reward_failure(&env, batch_id, &reward.recipient, reward.amount, error_code);
                continue;
            }

            // Validate recipient address
            if let Err(_) = validate_address(&env, &reward.recipient) {
                failed_count += 1;
                let error_code = BatchRewardsError::InvalidBatch as u32;
                results.push_back(RewardResult::Failure(
                    reward.recipient.clone(),
                    reward.amount,
                    error_code,
                ));
                RewardEvents::reward_failure(&env, batch_id, &reward.recipient, reward.amount, error_code);
                continue;
            }

            // Attempt to transfer the reward
            match token_client.try_transfer(&caller, &reward.recipient, &reward.amount) {
                Ok(_) => {
                    successful_count += 1;
                    total_distributed += reward.amount;
                    results.push_back(RewardResult::Success(
                        reward.recipient.clone(),
                        reward.amount,
                    ));
                    RewardEvents::reward_success(&env, batch_id, &reward.recipient, reward.amount);
                }
                Err(_) => {
                    failed_count += 1;
                    let error_code = BatchRewardsError::InvalidToken as u32;
                    results.push_back(RewardResult::Failure(
                        reward.recipient.clone(),
                        reward.amount,
                        error_code,
                    ));
                    RewardEvents::reward_failure(&env, batch_id, &reward.recipient, reward.amount, error_code);
                }
            }
        }

        // Update statistics
        env.storage()
            .instance()
            .set(&DataKey::TotalBatches, &batch_id);
        
        let total_processed: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalRewardsProcessed)
            .unwrap_or(0)
            + request_count as u64;
        env.storage()
            .instance()
            .set(&DataKey::TotalRewardsProcessed, &total_processed);

        let total_volume: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalVolumeDistributed)
            .unwrap_or(0)
            + total_distributed;
        env.storage()
            .instance()
            .set(&DataKey::TotalVolumeDistributed, &total_volume);

        // Emit batch completed event
        RewardEvents::batch_completed(&env, batch_id, successful_count, failed_count, total_distributed);

        BatchRewardResult {
            total_requests: request_count as u32,
            successful: successful_count,
            failed: failed_count,
            total_distributed,
            results,
        }
    }

    /// Internal helper to verify that the caller is the admin.
    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Contract not initialized");

        if admin != *caller {
            panic_with_error!(env, BatchRewardsError::Unauthorized);
        }
    }
}

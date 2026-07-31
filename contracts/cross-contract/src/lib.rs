//! # Cross-Contract Interaction Contract
//!
//! This contract provides secure cross-contract interaction capabilities for StellarSpend.
//! It allows calling external Soroban contracts with proper validation, error handling,
//! and event emission.

#![no_std]

mod types;
mod validation;

#[cfg(test)]
mod test;

use soroban_sdk::{
    contract, contractimpl, panic_with_error, Address, Bytes, Env, Symbol, Vec,
};

use shared::authorization::{add_allowed_contract, is_allowed_contract, remove_allowed_contract};
use shared::reentrancy::{enter, exit};

pub use crate::types::{
    BatchCallResult, CallResult, CrossContractCall, CrossContractEvents, DataKey, MAX_BATCH_CALLS,
};
use crate::validation::{is_whitelisted, validate_batch_calls, validate_call_request};

/// Error codes for the cross-contract interaction contract
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum CrossContractError {
    /// Contract not initialized
    NotInitialized = 1,
    /// Caller is not authorized
    Unauthorized = 2,
    /// Invalid contract address
    InvalidContractAddress = 3,
    /// Invalid function name
    InvalidFunctionName = 4,
    /// Contract not whitelisted
    ContractNotWhitelisted = 5,
    /// Empty batch
    EmptyBatch = 6,
    /// Batch exceeds maximum size
    BatchTooLarge = 7,
    /// Cross-contract call failed
    CallFailed = 8,
}

impl From<CrossContractError> for soroban_sdk::Error {
    fn from(e: CrossContractError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

#[contract]
pub struct CrossContractInteraction;

#[contractimpl]
impl CrossContractInteraction {
    /// Initializes the contract with an admin address
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Contract already initialized");
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::TotalCalls, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::SuccessfulCalls, &0u64);
        env.storage().instance().set(&DataKey::FailedCalls, &0u64);
    }

    /// Executes a single cross-contract call
    pub fn execute_call(
        env: Env,
        caller: Address,
        call: CrossContractCall,
        require_whitelist: bool,
    ) -> CallResult {
        // Verify authorization
        caller.require_auth();
        Self::require_admin(&env, &caller);

        // Enforce reentrancy guard for cross-contract interactions.
        if let Err(e) = enter(&env) {
            panic_with_error!(&env, e);
        }

        // Validate the call request
        if let Err(e) = validate_call_request(&env, &call, require_whitelist) {
            exit(&env);
            panic_with_error!(&env, e);
        }

        // Record the last caller for audit purposes
        env.storage().instance().set(&DataKey::LastCaller, &caller);

        // Emit call initiated event
        CrossContractEvents::call_initiated(
            &env,
            &caller,
            &call.contract_address,
            &call.function_name,
        );

        // Execute the call and handle result
        let result = Self::invoke_contract(&env, &call);

        // Clear the reentrancy lock before state updates to follow checks-effects-interactions.
        exit(&env);

        // Update statistics
        Self::update_call_stats(&env, result.success);

        // Emit appropriate event
        if result.success {
            CrossContractEvents::call_succeeded(&env, &call.contract_address, &call.function_name);
        } else {
            let error_msg = result
                .error_message
                .clone()
                .unwrap_or(Symbol::new(&env, "unknown"));
            CrossContractEvents::call_failed(
                &env,
                &call.contract_address,
                &call.function_name,
                &error_msg,
            );
        }

        result
    }

    /// Executes a batch of cross-contract calls
    pub fn execute_batch(
        env: Env,
        caller: Address,
        calls: Vec<CrossContractCall>,
        require_whitelist: bool,
    ) -> BatchCallResult {
        // Verify authorization
        caller.require_auth();
        Self::require_admin(&env, &caller);

        // Validate batch
        if let Err(e) = validate_batch_calls(&env, &calls, require_whitelist) {
            panic_with_error!(&env, e);
        }

        // Record the last caller for audit purposes
        env.storage().instance().set(&DataKey::LastCaller, &caller);

        let total_calls = calls.len();
        let mut successful_calls: u32 = 0;
        let mut failed_calls: u32 = 0;
        let mut results: Vec<CallResult> = Vec::new(&env);

        // Execute each call
        for i in 0..total_calls {
            let call = calls.get(i).unwrap();

            // Enter reentrancy guard before each external call.
            if let Err(e) = enter(&env) {
                panic_with_error!(&env, e);
            }

            // Emit call initiated event
            CrossContractEvents::call_initiated(
                &env,
                &caller,
                &call.contract_address,
                &call.function_name,
            );

            // Execute the call
            let result = Self::invoke_contract(&env, &call);

            // Clear the reentrancy lock before following state updates.
            exit(&env);

            // Update counters
            if result.success {
                successful_calls += 1;
                CrossContractEvents::call_succeeded(
                    &env,
                    &call.contract_address,
                    &call.function_name,
                );
            } else {
                failed_calls += 1;
                let error_msg = result
                    .error_message
                    .clone()
                    .unwrap_or(Symbol::new(&env, "unknown"));
                CrossContractEvents::call_failed(
                    &env,
                    &call.contract_address,
                    &call.function_name,
                    &error_msg,
                );

                // Stop batch if continue_on_failure is false
                if !call.continue_on_failure {
                    results.push(result);
                    break;
                }
            }

            results.push(result);
        }

        // Update statistics
        Self::update_batch_stats(&env, successful_calls, failed_calls);

        // Emit batch completed event
        CrossContractEvents::batch_completed(&env, total_calls, successful_calls, failed_calls);

        BatchCallResult {
            total_calls,
            successful_calls,
            failed_calls,
            results,
        }
    }

    /// Adds a contract to the whitelist
    pub fn whitelist_contract(env: Env, caller: Address, contract: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let key = DataKey::Whitelist(contract.clone());
        add_allowed_contract(&env, &key);

        CrossContractEvents::contract_whitelisted(&env, &contract);
    }

    /// Removes a contract from the whitelist
    pub fn remove_from_whitelist(env: Env, caller: Address, contract: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let key = DataKey::Whitelist(contract.clone());
        remove_allowed_contract(&env, &key);

        CrossContractEvents::contract_removed(&env, &contract);
    }

    /// Checks if a contract is whitelisted
    pub fn is_whitelisted(env: Env, contract: Address) -> bool {
        is_whitelisted(&env, &contract)
    }

    /// Verifies whether the given contract address is on the allow-list.
    pub fn verify_allowed_contract(env: Env, contract: Address) -> bool {
        is_allowed_contract(&env, &contract)
    }

    /// Gets the admin address
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, CrossContractError::NotInitialized))
    }

    /// Sets a new admin address
    pub fn set_admin(env: Env, current_admin: Address, new_admin: Address) {
        current_admin.require_auth();
        Self::require_admin(&env, &current_admin);

        env.storage().instance().set(&DataKey::Admin, &new_admin);
    }

    /// Gets total number of calls executed
    pub fn get_total_calls(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalCalls)
            .unwrap_or(0)
    }

    /// Gets total number of successful calls
    pub fn get_successful_calls(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::SuccessfulCalls)
            .unwrap_or(0)
    }

    /// Gets total number of failed calls
    pub fn get_failed_calls(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::FailedCalls)
            .unwrap_or(0)
    }

    /// Returns the address of the last contract that made a cross-contract call
    /// into this crate, or `None` if no call has been made yet.
    pub fn get_last_caller(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::LastCaller)
    }

    // Private helper functions

    /// Invokes an external contract
    fn invoke_contract(env: &Env, call: &CrossContractCall) -> CallResult {
        // Attempt to invoke the contract
        let result = env.try_invoke_contract::<Bytes, soroban_sdk::Error>(
            &call.contract_address,
            &call.function_name,
            call.args.clone(),
        );

        match result {
            Ok(Ok(return_data)) => CallResult {
                success: true,
                return_data: Some(return_data),
                error_message: None,
            },
            Ok(Err(_)) | Err(_) => CallResult {
                success: false,
                return_data: None,
                error_message: Some(Symbol::new(env, "call_failed")),
            },
        }
    }

    /// Updates call statistics for a single call
    fn update_call_stats(env: &Env, success: bool) {
        let total_calls: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalCalls)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::TotalCalls, &(total_calls + 1));

        if success {
            let successful: u64 = env
                .storage()
                .instance()
                .get(&DataKey::SuccessfulCalls)
                .unwrap_or(0);
            env.storage()
                .instance()
                .set(&DataKey::SuccessfulCalls, &(successful + 1));
        } else {
            let failed: u64 = env
                .storage()
                .instance()
                .get(&DataKey::FailedCalls)
                .unwrap_or(0);
            env.storage()
                .instance()
                .set(&DataKey::FailedCalls, &(failed + 1));
        }
    }

    /// Updates call statistics for a batch
    fn update_batch_stats(env: &Env, successful: u32, failed: u32) {
        let total_calls: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalCalls)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::TotalCalls, &(total_calls + successful as u64 + failed as u64));

        let total_successful: u64 = env
            .storage()
            .instance()
            .get(&DataKey::SuccessfulCalls)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::SuccessfulCalls, &(total_successful + successful as u64));

        let total_failed: u64 = env
            .storage()
            .instance()
            .get(&DataKey::FailedCalls)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::FailedCalls, &(total_failed + failed as u64));
    }

    /// Requires that the caller is the admin
    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(env, CrossContractError::NotInitialized));

        if caller != &admin {
            panic_with_error!(env, CrossContractError::Unauthorized);
        }
    }
}

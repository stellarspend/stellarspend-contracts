//! Type definitions for cross-contract interactions

use soroban_sdk::{contracttype, Address, Bytes, Symbol, Vec};

/// Maximum number of cross-contract calls in a batch
pub const MAX_BATCH_CALLS: u32 = 50;

/// Storage keys for the contract
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    /// Admin address
    Admin,
    /// Total number of cross-contract calls executed
    TotalCalls,
    /// Total number of successful calls
    SuccessfulCalls,
    /// Total number of failed calls
    FailedCalls,
    /// Whitelist of allowed contract addresses
    Whitelist(Address),
    /// Address of the last contract that made a cross-contract call into this crate
    LastCaller,
}

/// Request for a cross-contract call
#[derive(Clone)]
#[contracttype]
pub struct CrossContractCall {
    /// Target contract address
    pub contract_address: Address,
    /// Function name to call
    pub function_name: Symbol,
    /// Arguments for the function call (encoded)
    pub args: Vec<Bytes>,
    /// Whether to continue on failure
    pub continue_on_failure: bool,
}

/// Result of a single cross-contract call
#[derive(Clone)]
#[contracttype]
pub struct CallResult {
    /// Whether the call succeeded
    pub success: bool,
    /// Return data from the call (if successful)
    pub return_data: Option<Bytes>,
    /// Error message (if failed)
    pub error_message: Option<Symbol>,
}

/// Result of a batch of cross-contract calls
#[derive(Clone)]
#[contracttype]
pub struct BatchCallResult {
    /// Total number of calls attempted
    pub total_calls: u32,
    /// Number of successful calls
    pub successful_calls: u32,
    /// Number of failed calls
    pub failed_calls: u32,
    /// Individual call results
    pub results: Vec<CallResult>,
}

/// Events emitted by the cross-contract module
pub struct CrossContractEvents;

impl CrossContractEvents {
    /// Emit event when a cross-contract call is initiated
    pub fn call_initiated(
        env: &soroban_sdk::Env,
        caller: &Address,
        target: &Address,
        function: &Symbol,
    ) {
        env.events().publish(
            (Symbol::new(env, "call_initiated"), caller),
            (target, function),
        );
    }

    /// Emit event when a cross-contract call succeeds
    pub fn call_succeeded(
        env: &soroban_sdk::Env,
        target: &Address,
        function: &Symbol,
    ) {
        env.events().publish(
            (Symbol::new(env, "call_succeeded"),),
            (target, function),
        );
    }

    /// Emit event when a cross-contract call fails
    pub fn call_failed(
        env: &soroban_sdk::Env,
        target: &Address,
        function: &Symbol,
        error: &Symbol,
    ) {
        env.events().publish(
            (Symbol::new(env, "call_failed"),),
            (target, function, error),
        );
    }

    /// Emit event when a batch call is completed
    pub fn batch_completed(
        env: &soroban_sdk::Env,
        total: u32,
        successful: u32,
        failed: u32,
    ) {
        env.events().publish(
            (Symbol::new(env, "batch_completed"),),
            (total, successful, failed),
        );
    }

    /// Emit event when a contract is whitelisted
    pub fn contract_whitelisted(env: &soroban_sdk::Env, contract: &Address) {
        env.events().publish(
            (Symbol::new(env, "contract_whitelisted"),),
            contract,
        );
    }

    /// Emit event when a contract is removed from whitelist
    pub fn contract_removed(env: &soroban_sdk::Env, contract: &Address) {
        env.events().publish(
            (Symbol::new(env, "contract_removed"),),
            contract,
        );
    }
}

#![no_std]

//! Batch reward distribution contract with a cross-contract delegation
//! allowance check and shared tiered-rate decay calculation. Every
//! `pub fn` below already carries a `///` doc comment; this module doc
//! summarizes the file as a whole for `cargo doc`.

extern crate alloc;

use soroban_sdk::{contract, contracterror, contractimpl, Address, Env, Vec};

mod storage;
#[cfg(test)]
mod test;
pub mod types;
pub mod validation;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Typed errors for the batch_rewards contract.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// Contract has already been initialized.
    AlreadyInitialized = 1,
    /// Caller is not the administrator.
    Unauthorized = 2,
    /// Amount validation failed.
    InvalidAmount = 3,
    /// The delegate does not have sufficient allowance in the delegation
    /// contract to cover this reward distribution.
    DelegationCheckFailed = 4,
}

// ---------------------------------------------------------------------------
// Cross-contract delegation client
// ---------------------------------------------------------------------------

/// Lightweight client for calling `delegation::DelegationContract::check_allowance`
/// cross-contract.
///
/// Uses `env.invoke_contract` (the same low-level primitive used by the Reflector
/// oracle adapter in `shared`) so this crate does not need to link the full
/// delegation WASM at runtime — only the function name and ABI need to match.
pub struct DelegationClient {
    /// On-chain address of the deployed delegation contract.
    pub contract_address: Address,
}

impl DelegationClient {
    /// Creates a new client bound to `contract_address`.
    pub fn new(contract_address: Address) -> Self {
        Self { contract_address }
    }

    /// Calls `check_allowance` on the remote delegation contract.
    ///
    /// Returns `true` when `delegate` has at least `amount` of remaining
    /// allowance granted by `owner`. Returns `false` on any failure, treating
    /// a delegation contract that is unavailable or misconfigured as a
    /// "no allowance" result rather than a hard abort — the caller decides
    /// how to handle the `false` case.
    pub fn check_allowance(
        &self,
        env: &Env,
        owner: &Address,
        delegate: &Address,
        amount: i128,
    ) -> bool {
        // "check_allowance" is 15 chars — too long for symbol_short! (9-char limit).
        // Symbol::new accepts any valid Soroban identifier string at runtime.
        let fn_name = Symbol::new(env, "check_allowance");
        let args = soroban_sdk::vec![
            env,
            owner.clone().into_val(env),
            delegate.clone().into_val(env),
            amount.into_val(env),
        ];
        let result: Result<bool, soroban_sdk::Error> =
            env.invoke_contract(&self.contract_address, &fn_name, args);
        result.unwrap_or(false)
    }
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct Contract;

#[contractimpl]
impl Contract {
    /// Initializes the contract with an administrator.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if storage::read_config(&env).is_some() {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        storage::write_config(&env, &types::Config { admin, value: 0 });
        Ok(())
    }

    /// Updates the contract value after authenticating the administrator.
    pub fn set_value(env: Env, admin: Address, value: i128) -> Result<(), Error> {
        admin.require_auth();
        if value < 0 {
            return Err(Error::InvalidAmount);
        }
        let current = storage::read_config(&env).ok_or(Error::Unauthorized)?;
        if current.admin != admin {
            return Err(Error::Unauthorized);
        }
        storage::write_config(&env, &types::Config { admin, value });
        Ok(())
    }

    /// Returns the current configured value.
    pub fn get_value(env: Env) -> i128 {
        storage::read_config(&env).map(|c| c.value).unwrap_or(0)
    }

    /// Returns the configured administrator, if the contract has been
    /// initialized.
    pub fn get_owner(env: Env) -> Option<Address> {
        storage::owner(&env)
    }

    /// Calculates the applicable reward decay tier rate for `value` using the
    /// shared tiered-rate utility (`shared::rate_curve::calculate_tiered_rate`),
    /// so reward decay tier calculations stay consistent with fee and rewards
    /// rather than duplicating the tier-matching logic here (#1109).
    pub fn calculate_tiered_rate(value: i128, tiers: Vec<shared::rate_curve::Tier>) -> i128 {
        let tiers: alloc::vec::Vec<shared::rate_curve::Tier> = tiers.iter().collect();
        shared::rate_curve::calculate_tiered_rate(value, &tiers)
    }
}

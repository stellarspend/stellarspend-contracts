rust
#![no_std]

use soroban_sdk::{contract, contracterror, contractimpl, Address, Env};

mod storage;
#[cfg(test)]
mod test;
pub mod types;
pub mod validation;

/// Typed errors returned by the compliance contract.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// Contract has already been initialized.
    AlreadyInitialized = 1,
    /// Caller is not the administrator.
    Unauthorized = 2,
    /// Amount validation failed.
    InvalidAmount = 3,
}

#[contract]
pub struct Contract;

#[contractimpl]
impl Contract {
    /// Initializes the contract with the given administrator.
    ///
    /// The administrator must authenticate the initialization request.
    ///
    /// # Errors
    ///
    /// Returns [`Error::AlreadyInitialized`] if the contract has already
    /// been initialized.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if storage::read_config(&env).is_some() {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        storage::write_config(&env, &types::Config { admin, value: 0 });
        Ok(())
    }

    /// Updates the configured value for the contract.
    ///
    /// The provided administrator must authenticate the request and must
    /// match the administrator stored in the contract configuration.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidAmount`] if `value` is negative.
    ///
    /// Returns [`Error::Unauthorized`] if the contract has not been
    /// initialized or if `admin` is not the configured administrator.
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

    /// Returns the currently configured contract value.
    ///
    /// Returns `0` when the contract has not been initialized.
    pub fn get_value(env: Env) -> i128 {
        storage::read_config(&env).map(|c| c.value).unwrap_or(0)
    }
}

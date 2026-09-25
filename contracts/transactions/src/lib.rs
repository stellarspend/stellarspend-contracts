#![no_std]

//! Admin-gated configuration contract for transaction parameters. Every
//! `pub fn` below already carries a `///` doc comment; this module doc
//! summarizes the file as a whole for `cargo doc`.

use soroban_sdk::{contract, contracterror, contractimpl, Address, Env};

mod storage;
#[cfg(test)]
mod test;
pub mod types;
pub mod validation;

/// Typed errors for the transactions contract.
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

/// The transactions contract entry point.
///
/// All public methods below are exported as contract functions.
#[contract]
pub struct Contract;

#[contractimpl]
impl Contract {
    /// Initializes the contract with an administrator.
    ///
    /// Requires authorization from `admin` and sets the stored value to `0`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::AlreadyInitialized`] if a config is already stored.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if storage::read_config(&env).is_some() {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        storage::write_config(&env, &types::Config { admin, value: 0 });
        Ok(())
    }

    /// Updates the contract value after authenticating the administrator.
    ///
    /// Requires authorization from `admin`, which must match the stored
    /// administrator.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidAmount`] if `value` is negative, or
    /// [`Error::Unauthorized`] if the contract is uninitialized or `admin` is
    /// not the stored administrator.
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
    ///
    /// Returns `0` when the contract has not been initialized. Requires no
    /// authorization.
    pub fn get_value(env: Env) -> i128 {
        storage::read_config(&env).map(|c| c.value).unwrap_or(0)
    }
}

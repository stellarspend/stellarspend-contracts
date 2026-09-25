#![no_std]

//! Manages user-facing alert and delivery-preference configuration for
//! StellarSpend events. Every `pub fn` below already carries a `///` doc
//! comment; this module doc summarizes the file as a whole for `cargo doc`.

use soroban_sdk::{contract, contracterror, contractimpl, Address, Env};

mod storage;
#[cfg(test)]
mod test;
pub mod types;
pub mod validation;

/// Typed errors for the notification contract.
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

/// Notification contract entrypoint: manages user-facing alert and
/// delivery-preference configuration for StellarSpend events.
#[contract]
pub struct Contract;

#[contractimpl]
impl Contract {
    /// Initializes the notification contract with an administrator.
    /// Must be called exactly once, before any other entrypoint; fails
    /// with [`Error::AlreadyInitialized`] on a second call.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if storage::read_config(&env).is_some() {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        storage::write_config(&env, &types::Config { admin, value: 0 });
        Ok(())
    }

    /// Updates the notification contract's configured value after
    /// authenticating the caller as the current administrator. Fails
    /// with [`Error::InvalidAmount`] for a negative value, or
    /// [`Error::Unauthorized`] if `admin` does not match the stored
    /// administrator.
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

    /// Returns the notification contract's current configured value,
    /// or `0` if the contract has not yet been initialized.
    pub fn get_value(env: Env) -> i128 {
        storage::read_config(&env).map(|c| c.value).unwrap_or(0)
    }
}

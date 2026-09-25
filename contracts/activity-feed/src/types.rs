//! Shared types for the activity-feed contract. Every `pub struct`/`pub
//! enum` below already carries a `///` doc comment; this module doc
//! summarizes the file as a whole for `cargo doc`.

use soroban_sdk::contracttype;

/// Stored configuration for this StellarSpend contract.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Config {
    /// Contract administrator.
    pub admin: soroban_sdk::Address,
    /// Current configured value.
    pub value: i128,
}

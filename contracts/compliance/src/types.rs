//! # Compliance data types
//!
//! Shared value types persisted by the compliance contract. Every public
//! struct and enum is documented here — and each field carries its own `///`
//! comment — so the generated reference for this crate is self-contained.

use soroban_sdk::contracttype;

/// Stored configuration for this StellarSpend contract.
///
/// Persisted in the contract's instance storage. `admin` is the only account
/// permitted to mutate the configuration, and `value` is the current
/// compliance value tracked by the contract.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Config {
    /// Contract administrator.
    pub admin: soroban_sdk::Address,
    /// Current configured value.
    pub value: i128,
}


use soroban_sdk::{contracttype, Address};

/// Stored configuration for the StellarSpend contract.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Config {
    /// Address of the contract administrator.
    pub admin: Address,
    /// Current configured value.
    pub value: i128,
}

//! Validation utilities for batch wallet creation.

use soroban_sdk::{Address, Env};

/// Validates an owner address.
pub fn validate_address(_address: &Address) -> Result<(), ()> {
    // For now, assume all addresses are valid
    Ok(())
}

/// Checks if a wallet already exists for the given address.
pub fn wallet_exists(env: &Env, address: &Address) -> bool {
    use crate::types::DataKey;
    env.storage().persistent().has(&DataKey::Wallets(address.clone()))
}
//! Reentrancy guard helpers for contract call safety.

use soroban_sdk::{contracttype, Env};

use crate::errors::SharedError;

/// Storage key used to track whether the contract is currently executing a guarded
/// external call.
#[contracttype]
#[derive(Clone)]
pub enum ReentrancyDataKey {
    Lock,
}

/// Returns `true` if the contract is already inside a guarded external call.
pub fn is_entered(env: &Env) -> bool {
    env.storage()
        .instance()
        .get::<ReentrancyDataKey, bool>(&ReentrancyDataKey::Lock)
        .unwrap_or(false)
}

/// Sets the reentrancy lock, rejecting nested guarded executions.
pub fn enter(env: &Env) -> Result<(), SharedError> {
    if is_entered(env) {
        return Err(SharedError::ResourceLocked);
    }
    env.storage()
        .instance()
        .set(&ReentrancyDataKey::Lock, &true);
    Ok(())
}

/// Clears the reentrancy lock after the guarded operation completes.
pub fn exit(env: &Env) {
    env.storage().instance().remove(&ReentrancyDataKey::Lock);
}

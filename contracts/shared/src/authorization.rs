//! Authorization helpers for configurable contract allow-lists.

use soroban_sdk::{contracttype, Env};

use crate::errors::SharedError;

/// Returns `true` when the supplied key is present in an allow-list storage entry.
///
/// The key type is generic so contracts can use their own allow-list `DataKey`
/// enum without duplicating the helper logic.
pub fn is_allowed_contract<K>(env: &Env, key: &K) -> bool
where
    K: contracttype::ContractType,
{
    env.storage()
        .persistent()
        .get::<K, bool>(key)
        .unwrap_or(false)
}

/// Requires that the supplied allow-list key is present.
pub fn require_allowed_contract<K>(env: &Env, key: &K) -> Result<(), SharedError>
where
    K: contracttype::ContractType,
{
    if !is_allowed_contract(env, key) {
        Err(SharedError::Unauthorized)
    } else {
        Ok(())
    }
}

/// Adds the supplied allow-list key.
pub fn add_allowed_contract<K>(env: &Env, key: &K)
where
    K: contracttype::ContractType,
{
    env.storage().persistent().set(key, &true);
}

/// Removes the supplied allow-list key.
pub fn remove_allowed_contract<K>(env: &Env, key: &K)
where
    K: contracttype::ContractType,
{
    env.storage().persistent().remove(key);
}

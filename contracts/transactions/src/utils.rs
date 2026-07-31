extern crate alloc;
use alloc::format;
use soroban_sdk::{Env, Symbol};

/// Generate a unique transaction ID
pub fn generate_transaction_id(env: &Env) -> Symbol {
    // Use a persistent counter for unique IDs
    let mut counter: u64 = env
        .storage()
        .persistent()
        .get(&crate::storage::DataKey::TransactionCounter)
        .unwrap_or(0);

    counter += 1;

    // Update counter
    env.storage()
        .persistent()
        .set(&crate::storage::DataKey::TransactionCounter, &counter);

    Symbol::new(env, &format!("tx{}", counter))
}

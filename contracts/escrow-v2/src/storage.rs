//! Persistent storage helpers for escrow-v2.

use crate::types::{DataKey, Escrow};
use soroban_sdk::Env;

/// Returns the next escrow id and increments the counter.
pub fn next_escrow_id(env: &Env) -> u64 {
    let current: u64 = env
        .storage()
        .instance()
        .get(&DataKey::EscrowCounter)
        .unwrap_or(0);
    let next = current.checked_add(1).expect("escrow counter overflow");
    env.storage().instance().set(&DataKey::EscrowCounter, &next);
    next
}

/// Stores an escrow record.
pub fn set_escrow(env: &Env, escrow: &Escrow) {
    env.storage()
        .persistent()
        .set(&DataKey::Escrow(escrow.escrow_id), escrow);
}

/// Loads an escrow by id, if present.
pub fn get_escrow(env: &Env, escrow_id: u64) -> Option<Escrow> {
    env.storage().persistent().get(&DataKey::Escrow(escrow_id))
}

/// Returns the total number of escrows created.
pub fn escrow_counter(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::EscrowCounter)
        .unwrap_or(0)
}

//! Storage helpers for the spending policy contract.
//!
//! All accessors are thin wrappers around the Soroban storage API. Policy and
//! per-wallet data live in persistent storage; the pending-id counter lives in
//! instance storage.

use soroban_sdk::{Address, Env, Symbol, Vec};

use crate::types::{DataKey, PendingTransaction, Policy};

// --- Policy -----------------------------------------------------------------

pub fn get_policy(env: &Env, wallet: &Address) -> Option<Policy> {
    env.storage()
        .persistent()
        .get(&DataKey::Policy(wallet.clone()))
}

pub fn set_policy(env: &Env, wallet: &Address, policy: &Policy) {
    env.storage()
        .persistent()
        .set(&DataKey::Policy(wallet.clone()), policy);
}

// --- Pending id counter -----------------------------------------------------

pub fn get_next_pending_id(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::NextPendingId)
        .unwrap_or(0)
}

pub fn set_next_pending_id(env: &Env, id: u64) {
    env.storage().instance().set(&DataKey::NextPendingId, &id);
}

// --- Pending transactions ---------------------------------------------------

pub fn get_pending_tx(env: &Env, id: u64) -> Option<PendingTransaction> {
    env.storage().persistent().get(&DataKey::PendingTx(id))
}

pub fn set_pending_tx(env: &Env, pending: &PendingTransaction) {
    env.storage()
        .persistent()
        .set(&DataKey::PendingTx(pending.id), pending);
}

pub fn remove_pending_tx(env: &Env, id: u64) {
    env.storage().persistent().remove(&DataKey::PendingTx(id));
}

// --- Per-wallet pending index ----------------------------------------------

pub fn get_pending_ids_for_wallet(env: &Env, wallet: &Address) -> Vec<u64> {
    env.storage()
        .persistent()
        .get(&DataKey::PendingByWallet(wallet.clone()))
        .unwrap_or_else(|| Vec::new(env))
}

pub fn set_pending_ids_for_wallet(env: &Env, wallet: &Address, ids: &Vec<u64>) {
    env.storage()
        .persistent()
        .set(&DataKey::PendingByWallet(wallet.clone()), ids);
}

/// Remove a pending id from a wallet's pending index (no-op if absent).
pub fn remove_pending_id_from_wallet(env: &Env, wallet: &Address, id: u64) {
    let ids = get_pending_ids_for_wallet(env, wallet);
    let mut new_ids: Vec<u64> = Vec::new(env);
    for x in ids.iter() {
        if x != id {
            new_ids.push_back(x);
        }
    }
    set_pending_ids_for_wallet(env, wallet, &new_ids);
}

// --- Category spend tracking ------------------------------------------------

pub fn get_category_spending(
    env: &Env,
    wallet: &Address,
    category: &Symbol,
    period_id: u64,
) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::CategorySpending(
            wallet.clone(),
            category.clone(),
            period_id,
        ))
        .unwrap_or(0)
}

pub fn set_category_spending(
    env: &Env,
    wallet: &Address,
    category: &Symbol,
    period_id: u64,
    amount: i128,
) {
    env.storage().persistent().set(
        &DataKey::CategorySpending(wallet.clone(), category.clone(), period_id),
        &amount,
    );
}

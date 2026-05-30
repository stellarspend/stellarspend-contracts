//! Savings goal expiration — marks goals as inactive once their deadline passes.

#![no_std]

use soroban_sdk::{contracttype, symbol_short, Env};

#[contracttype]
#[derive(Clone)]
enum ExpKey { Deadline(u64), Expired(u64) }

/// Registers a deadline (Unix timestamp) for a goal.
pub fn set_deadline(env: &Env, goal_id: u64, deadline: u64) {
    env.storage().persistent().set(&ExpKey::Deadline(goal_id), &deadline);
}

/// Returns true if the goal deadline has passed.
pub fn is_expired(env: &Env, goal_id: u64) -> bool {
    let deadline: u64 = env.storage().persistent()
        .get(&ExpKey::Deadline(goal_id)).unwrap_or(u64::MAX);
    env.ledger().timestamp() >= deadline
}

/// Marks an expired goal as inactive and emits an event.
/// Returns true if the goal was newly expired, false if already expired or not due.
pub fn expire_if_due(env: &Env, goal_id: u64) -> bool {
    if !is_expired(env, goal_id) { return false; }
    let already: bool = env.storage().persistent()
        .get(&ExpKey::Expired(goal_id)).unwrap_or(false);
    if already { return false; }
    env.storage().persistent().set(&ExpKey::Expired(goal_id), &true);
    env.events().publish((symbol_short!("goal"), symbol_short!("expired")), goal_id);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Ledger, Env};

    #[test]
    fn goal_expires_after_deadline() {
        let env = Env::default();
        env.ledger().set_timestamp(1000);
        set_deadline(&env, 1, 500);
        assert!(is_expired(&env, 1));
    }

    #[test]
    fn goal_not_expired_before_deadline() {
        let env = Env::default();
        env.ledger().set_timestamp(100);
        set_deadline(&env, 2, 500);
        assert!(!is_expired(&env, 2));
    }
}
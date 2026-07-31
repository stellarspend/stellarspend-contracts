//! Core query operations for the rewards contract.
//!
//! Separates read-only operations from state-mutating logic.

use soroban_sdk::{Address, Env};

use crate::storage::{
    get_lifetime_claimed, get_lifetime_earned, get_reward_account, get_reward_balance,
    get_reward_index,
};
use crate::types::{RewardAccount, RewardStatistics};

/// Returns the current claimable reward balance for `participant`.
pub fn query_reward_balance(env: &Env, participant: &Address) -> i128 {
    get_reward_balance(env, participant)
}

/// Returns the total rewards ever earned by `participant`.
pub fn query_lifetime_earnings(env: &Env, participant: &Address) -> i128 {
    get_lifetime_earned(env, participant)
}

/// Returns the total number of reward transactions for `participant`.
pub fn query_transaction_count(env: &Env, participant: &Address) -> u32 {
    get_reward_index(env, participant).len()
}

/// Returns aggregated statistics for `participant`.
pub fn query_statistics(env: &Env, participant: &Address) -> RewardStatistics {
    RewardStatistics {
        balance: get_reward_balance(env, participant),
        lifetime_earned: get_lifetime_earned(env, participant),
        lifetime_claimed: get_lifetime_claimed(env, participant),
        tx_count: get_reward_index(env, participant).len(),
    }
}

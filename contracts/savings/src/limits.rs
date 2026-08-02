use soroban_sdk::{Address, Env};

use crate::storage::DataKey;
use crate::types::ContributionPeriod;

pub const DEFAULT_MAX_LIMIT: i128 = i128::MAX;

pub fn current_bucket(
    env: &Env,
    period: &ContributionPeriod,
) -> u64 {
    let timestamp = env.ledger().timestamp();

    match period {
        ContributionPeriod::Daily => timestamp / 86_400,
        ContributionPeriod::Weekly => timestamp / 604_800,
        ContributionPeriod::Monthly => timestamp / 2_592_000,
    }
}

pub fn get_savings_limit(env: &Env, owner: Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::SavingsLimit(owner))
        .unwrap_or(DEFAULT_MAX_LIMIT)
}

pub fn set_savings_limit(env: &Env, owner: Address, limit: i128) {
    owner.require_auth();
    if limit < 0 {
        panic!("Limit cannot be negative");
    }
    env.storage()
        .persistent()
        .set(&DataKey::SavingsLimit(owner), &limit);
}
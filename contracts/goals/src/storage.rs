use soroban_sdk::{contracttype, Address, Env};

use crate::goal::Goal;

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Goal(u64),
    GoalCounter,
}

pub fn get_next_goal_id(env: &Env) -> u64 {
    let key = DataKey::GoalCounter;
    let id: u64 = env.storage().instance().get(&key).unwrap_or(0);
    let next = id + 1;
    env.storage().instance().set(&key, &next);
    next
}

pub fn get_goal(env: &Env, goal_id: u64) -> Option<Goal> {
    env.storage()
        .persistent()
        .get(&DataKey::Goal(goal_id))
}

pub fn get_goals_by_owner(env: &Env, owner: Address) -> Vec<Goal> {
    let mut goals = Vec::new(env);
    // Best-effort scan not supported on Soroban persistent storage; placeholder for parity.
    goals
}

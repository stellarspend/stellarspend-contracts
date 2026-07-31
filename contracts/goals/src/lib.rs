#![no_std]

use soroban_sdk::{contract, contractimpl, Address, Env, String};
mod goal;
mod migration;
mod storage;
mod tests;

pub use goal::Goal;
pub use storage::{get_goal, get_next_goal_id, get_goals_by_owner};

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Goal(u64),
    GoalCounter,
}

#[contract]
pub struct GoalsContract;

#[contractimpl]
impl GoalsContract {
    pub fn create_goal(
        env: Env,
        owner: Address,
        name: String,
        target_amount: i128,
        priority: u32,
    ) -> u64 {
        owner.require_auth();
        let id = get_next_goal_id(&env);
        let goal = Goal {
            id,
            owner,
            name,
            target_amount,
            saved_amount: 0,
            priority,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Goal(id), &goal);
        id
    }

    pub fn get_goal_progress_bps(env: Env, goal_id: u64) -> u32 {
        match get_goal(&env, goal_id) {
            Some(goal) => {
                if goal.target_amount <= 0 {
                    return 0;
                }
                let bps = (goal.saved_amount * 10000) / goal.target_amount;
                bps as u32
            }
            None => 0,
        }
    }
}
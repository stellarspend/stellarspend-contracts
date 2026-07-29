use soroban_sdk::{contract, contractimpl, Address, Env, Vec};

#[contract]
pub struct BatchHistoryContract;

#[contractimpl]
impl BatchHistoryContract {
    pub fn get_batch_history_entries(env: Env, owner: Address) -> Vec<u64> {
        // Retrieve batch history entries associated with the owner address, returning empty Vec if none exist.
        env.storage().persistent().get(&owner).unwrap_or_else(|| Vec::new(&env))
    }
}

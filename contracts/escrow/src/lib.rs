use soroban_sdk::{contract, contractimpl, Env};

#[contract]
pub struct EscrowContract;

#[contractimpl]
impl EscrowContract {
    pub fn get_escrow_balance(env: Env, escrow_id: u64) -> i128 {
        // Retrieve the locked balance for the given escrow ID, returning 0 if it does not exist.
        env.storage().persistent().get(&escrow_id).unwrap_or(0)
    }
}

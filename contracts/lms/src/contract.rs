use soroban_sdk::{contract, contractimpl, Env};

#[contract]
pub struct LMSContract;

#[contractimpl]
impl LMSContract {
    pub fn initialize(_env: Env) -> bool {
        true
    }
}

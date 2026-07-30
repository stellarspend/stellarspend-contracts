use soroban_sdk::{contract, contractimpl, Address, Env};

use crate::{errors::LmsError, lesson};

#[contract]
pub struct LMSContract;

#[contractimpl]
impl LMSContract {
    pub fn initialize(_env: Env) -> bool {
        true
    }

    pub fn remove_lesson(env: Env, caller: Address, lesson_id: u64) -> Result<(), LmsError> {
        lesson::remove_lesson(env, caller, lesson_id)
    }
}

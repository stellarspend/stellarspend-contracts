#![no_std]

pub mod validation;

#[cfg(test)]
mod test;

use soroban_sdk::{contract, contractimpl, contracttype, Env, Vec};
use validation::{validate_transaction_timestamp, TimestampValidationError};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    ValidationErrors(u64),
}

#[contract]
pub struct TransactionValidationContract;

#[contractimpl]
impl TransactionValidationContract {
    /// Validates a transaction payload and its timestamp.
    /// Acts as an entry point for transaction processing flows.
    pub fn process_transaction(
        env: Env,
        tx_id: u64,
        tx_timestamp: u64,
    ) -> Result<(), TimestampValidationError> {
        let validation_result = validate_transaction_timestamp(&env, tx_timestamp);

        if let Err(err) = validation_result {
            let key = DataKey::ValidationErrors(tx_id);
            let mut errors: Vec<u32> = env
                .storage()
                .instance()
                .get(&key)
                .unwrap_or_else(|| Vec::new(&env));
            errors.push_back(err as u32);
            env.storage().instance().set(&key, &errors);
            return Err(err);
        }

        Ok(())
    }

    /// Returns validation error codes recorded for a transaction.
    pub fn get_validation_errors(env: Env, tx_id: u64) -> Vec<u32> {
        env.storage()
            .instance()
            .get(&DataKey::ValidationErrors(tx_id))
            .unwrap_or_else(|| Vec::new(&env))
    }
}

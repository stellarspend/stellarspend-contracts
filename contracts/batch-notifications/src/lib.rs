#![no_std]
use soroban_sdk::{contract, contractimpl, Address, Env, Vec};

mod logic;
mod types;

#[cfg(test)]
mod test;

use crate::types::{BatchResult, NotificationPayload};

#[contract]
pub struct BatchNotificationContract;

#[contractimpl]
impl BatchNotificationContract {
    pub fn batch_notify(
        env: Env,
        admin: Address,
        batch_id: u64,
        payloads: Vec<NotificationPayload>,
    ) -> BatchResult {
        // Requirement: Validate user/admin addresses
        admin.require_auth();

        // Run the batch logic
        logic::execute_dispatch(env, batch_id, payloads)
    }

    /// Returns the number of notifications dispatched in a given batch.
    /// Returns 0 if the batch ID is unknown.
    pub fn get_batch_notification_count(env: Env, batch_id: u64) -> u32 {
        logic::read_batch_count(&env, batch_id)
    }
}

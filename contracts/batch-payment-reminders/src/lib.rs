#![no_std]

mod logic;
mod types;
mod validation;

#[cfg(test)]
mod test;

use crate::types::{BatchReminderResult, PaymentReminderRequest};
use soroban_sdk::{contract, contractimpl, Address, Env, Vec};

#[derive(Clone)]
#[contracttype]
pub enum ReminderDataKey {
    Reminder(u64),
}

#[contract]
pub struct BatchPaymentRemindersContract;

#[contractimpl]
impl BatchPaymentRemindersContract {
    /// Send batch payment reminders to multiple users.
    ///
    /// Validates each (user, due_date); valid entries get a reminder_sent event,
    /// invalid ones are skipped and recorded in the result (partial failure handling).
    ///
    /// # Arguments
    /// * `admin` - Caller must authorize (admin).
    /// * `requests` - List of (user, due_date) reminder requests.
    /// # Returns
    /// * `BatchReminderResult` with successful_count and failed_addresses.
    pub fn dispatch_batch_reminders(
        env: Env,
        admin: Address,
        requests: Vec<PaymentReminderRequest>,
    ) -> BatchReminderResult {
        admin.require_auth();

        let batch_id = env.ledger().sequence() as u64;
        logic::execute_dispatch(env.clone(), batch_id, requests.clone())
    }

    /// Returns the due timestamp for a reminder.
    ///
    /// Returns `0` if the reminder is unknown.
    pub fn get_reminder_due_date(env: Env, reminder_id: u64) -> u64 {
        env.storage()
            .persistent()
            .get(&ReminderDataKey::Reminder(reminder_id))
            .unwrap_or(0)
    }
}

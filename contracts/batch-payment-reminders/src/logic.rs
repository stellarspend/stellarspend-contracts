//! Batch payment reminder dispatch: validate each request, handle partial failures, emit events.

use crate::types::{BatchReminderResult, PaymentReminderRequest};
use crate::validation::{validate_reminder_request, ValidationError};
use soroban_sdk::{symbol_short, Env, Vec};

pub fn execute_dispatch(
    env: Env,
    batch_id: u64,
    requests: Vec<PaymentReminderRequest>,
) -> BatchReminderResult {
    let mut successful_count: u32 = 0;
    let mut failed_addresses = Vec::new(&env);

    env.events().publish(
        (
            symbol_short!("batch_rem"),
            symbol_short!("started"),
            batch_id,
        ),
        requests.len() as u32,
    );

    for (index, request) in requests.iter().enumerate() {
        let reminder_id = (batch_id as u128) << 32 | (index as u128);
        let reminder_id_u64 = (reminder_id & u128::from(u64::MAX)) as u64;

        match validate_reminder_request(&env, &request.user, request.due_date) {
            Ok(()) => {
                env.storage().persistent().set(
                    &crate::ReminderDataKey::Reminder(reminder_id_u64),
                    &request.due_date,
                );

                env.events().publish(
                    (
                        symbol_short!("rem_sent"),
                        request.user.clone(),
                        request.due_date,
                    ),
                    batch_id,
                );
                successful_count += 1;
            }
            Err(ValidationError::InvalidUser) | Err(ValidationError::InvalidDueDate) => {
                env.events().publish(
                    (
                        symbol_short!("rem_fail"),
                        request.user.clone(),
                        symbol_short!("invalid"),
                    ),
                    (batch_id, request.due_date),
                );
                failed_addresses.push_back(request.user.clone());
            }
        }
    }

    env.events().publish(
        (
            symbol_short!("batch_rem"),
            symbol_short!("completed"),
            batch_id,
        ),
        (successful_count, failed_addresses.len() as u32),
    );

    BatchReminderResult {
        successful_count,
        failed_addresses,
    }
}

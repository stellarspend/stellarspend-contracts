use crate::types::{BatchResult, NotificationPayload};
use soroban_sdk::{symbol_short, Env, Vec};

pub fn execute_dispatch(
    env: Env,
    batch_id: u64,
    payloads: Vec<NotificationPayload>,
) -> BatchResult {
    let mut success_count = 0;
    let mut failures = Vec::new(&env);

    for payload in payloads.iter() {
        // Requirement: Handle partial failure gracefully
        // We consider an empty message a "soft failure" instead of panicking
        if !payload.message.is_empty() {
            // Requirement: Emit events for notification delivery
            env.events().publish(
                (symbol_short!("notif"), payload.user.clone()),
                payload.message,
            );
            success_count += 1;
        } else {
            // If it fails, add the user to the failure list
            failures.push_back(payload.user);
        }
    }

    // Persist the count of notifications in this batch
    let count: u32 = payloads.len() as u32;
    let key = (symbol_short!("batch_cnt"), batch_id);
    env.storage().persistent().set(&key, &count);

    BatchResult {
        successful_count: success_count,
        failed_addresses: failures,
    }
}

/// Reads the notification count for a given batch from storage.
/// Returns 0 if no entry exists for the batch_id.
pub fn read_batch_count(env: &Env, batch_id: u64) -> u32 {
    let key = (symbol_short!("batch_cnt"), batch_id);
    env.storage()
        .persistent()
        .get::<_, u32>(&key)
        .unwrap_or(0)
}

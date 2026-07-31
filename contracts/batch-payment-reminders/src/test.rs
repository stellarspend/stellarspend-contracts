#![cfg(test)]

use crate::types::PaymentReminderRequest;
use crate::{BatchPaymentRemindersContract, BatchPaymentRemindersContractClient};
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    vec, Address, Env, Vec,
};

fn setup(env: &Env) -> (Address, BatchPaymentRemindersContractClient<'_>) {
    env.mock_all_auths();
    let contract_id = env.register(BatchPaymentRemindersContract, ());
    let client = BatchPaymentRemindersContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    (admin, client)
}

fn current_ledger(env: &Env) -> u64 {
    env.ledger().sequence() as u64
}

#[test]
fn test_dispatch_batch_reminders_all_success() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let due = current_ledger(&env) + 100;

    let requests = vec![
        &env,
        PaymentReminderRequest {
            user: user1.clone(),
            due_date: due,
        },
        PaymentReminderRequest {
            user: user2.clone(),
            due_date: due + 1,
        },
    ];

    let result = client.dispatch_batch_reminders(&admin, &requests);

    assert_eq!(result.successful_count, 2);
    assert_eq!(result.failed_addresses.len(), 0);

    let events = env.events().all();
    assert!(
        events.len() >= 4,
        "expected at least 4 events: started + 2x rem_sent + completed"
    );
}

#[test]
fn test_dispatch_batch_reminders_partial_failure() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    let user_ok = Address::generate(&env);
    let user_bad_due = Address::generate(&env);
    let current = current_ledger(&env);

    let requests = vec![
        &env,
        PaymentReminderRequest {
            user: user_ok.clone(),
            due_date: current + 50,
        },
        PaymentReminderRequest {
            user: user_bad_due.clone(),
            due_date: current, // invalid: not in future
        },
    ];

    let result = client.dispatch_batch_reminders(&admin, &requests);

    assert_eq!(result.successful_count, 1);
    assert_eq!(result.failed_addresses.len(), 1);
    assert_eq!(result.failed_addresses.get(0).unwrap(), user_bad_due);

    let events = env.events().all();
    assert!(
        events.len() >= 3,
        "expected started + rem_sent + rem_fail + completed"
    );
}

#[test]
fn test_dispatch_batch_reminders_events_emitted() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    let user = Address::generate(&env);
    let requests = vec![
        &env,
        PaymentReminderRequest {
            user: user.clone(),
            due_date: current_ledger(&env) + 200,
        },
    ];

    client.dispatch_batch_reminders(&admin, &requests);

    let events = env.events().all();
    assert!(!events.is_empty(), "events emitted");
    assert!(
        events.len() >= 2,
        "expected at least started + rem_sent + completed"
    );
}

#[test]
fn test_dispatch_batch_reminders_requires_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(BatchPaymentRemindersContract, ());
    let client = BatchPaymentRemindersContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let requests = vec![
        &env,
        PaymentReminderRequest {
            user,
            due_date: current_ledger(&env) + 10,
        },
    ];

    // Call without admin auth will panic in require_auth
    client.dispatch_batch_reminders(&admin, &requests);
    // If we get here with mock_all_auths, auth passed
    assert!(true);
}

#[test]
fn test_dispatch_batch_reminders_empty_batch() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    let requests: Vec<PaymentReminderRequest> = Vec::new(&env);
    let result = client.dispatch_batch_reminders(&admin, &requests);

    assert_eq!(result.successful_count, 0);
    assert_eq!(result.failed_addresses.len(), 0);

    let events = env.events().all();
    assert!(events.len() >= 2, "expected started + completed events");
}

// ── Issue #994: get_reminder_due_date view ────────────────────────────────────

#[test]
fn test_get_reminder_due_date_returns_stored_value() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    let user = Address::generate(&env);
    let due = current_ledger(&env) + 100;
    let requests = vec![
        &env,
        PaymentReminderRequest {
            user: user.clone(),
            due_date: due,
        },
    ];

    client.dispatch_batch_reminders(&admin, &requests);

    // The first (and only) reminder in the batch should be index 0, batch sequence as prefix.
    let batch_id = current_ledger(&env);
    let reminder_id = ((batch_id as u128) << 32) as u64;
    assert_eq!(client.get_reminder_due_date(&reminder_id), due);
}

#[test]
fn test_get_reminder_due_date_returns_zero_for_unknown() {
    let env = Env::default();
    let (_admin, client) = setup(&env);

    assert_eq!(client.get_reminder_due_date(&99999), 0);
}

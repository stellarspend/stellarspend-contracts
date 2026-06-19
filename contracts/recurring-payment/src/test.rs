#![cfg(test)]

extern crate std;

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{token, Address, Env};
use std::panic::{catch_unwind, AssertUnwindSafe};

fn setup_test_env() -> (
    Env,
    Address,
    Address,
    Address,
    token::Client<'static>,
    token::StellarAssetClient<'static>,
    RecurringPaymentContractClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let stellar_asset = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = stellar_asset.address();
    let token_client = token::Client::new(&env, &token_addr);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_addr);

    let contract_id = env.register(RecurringPaymentContract, ());
    let client = RecurringPaymentContractClient::new(&env, &contract_id);

    (
        env,
        sender,
        recipient,
        token_addr,
        token_client,
        token_admin_client,
        client,
    )
}

fn create_payment(
    client: &RecurringPaymentContractClient,
    sender: &Address,
    recipient: &Address,
    token: &Address,
    amount: i128,
    interval: u64,
    start_time: u64,
) -> u64 {
    client.create_payment(sender, recipient, token, &amount, &interval, &start_time)
}

#[test]
fn test_recurring_payment_flow() {
    let (env, sender, recipient, token_addr, token_client, token_admin_client, client) =
        setup_test_env();

    let amount = 1000i128;
    let interval = 3600u64;
    let start_time = 1000u64;
    token_admin_client.mint(&sender, &5000i128);

    let payment_id = create_payment(
        &client,
        &sender,
        &recipient,
        &token_addr,
        amount,
        interval,
        start_time,
    );
    assert_eq!(payment_id, 1);

    let payment = client.get_payment(&payment_id);
    assert_eq!(payment.amount, amount);
    assert_eq!(payment.next_execution, start_time);
    assert!(payment.active);
    assert!(!payment.paused);
    assert_eq!(payment.execution_count, 0);

    env.ledger().set_timestamp(start_time);
    client.execute_payment(&payment_id);

    assert_eq!(token_client.balance(&sender), 4000);
    assert_eq!(token_client.balance(&recipient), 1000);

    let payment = client.get_payment(&payment_id);
    assert_eq!(payment.next_execution, start_time + interval);
    assert_eq!(payment.execution_count, 1);

    client.cancel_payment(&payment_id);
    let payment = client.get_payment(&payment_id);
    assert!(!payment.active);
}

#[test]
#[should_panic(expected = "Amount must be positive")]
fn test_create_with_zero_amount() {
    let (_env, sender, recipient, token_addr, _token_client, _token_admin_client, client) =
        setup_test_env();

    client.create_payment(&sender, &recipient, &token_addr, &0, &3600, &1000);
}

#[test]
fn test_execute_with_delay() {
    let (env, sender, recipient, token_addr, token_client, token_admin_client, client) =
        setup_test_env();

    let amount = 1000i128;
    let interval = 3600u64;
    let start_time = 1000u64;
    token_admin_client.mint(&sender, &5000i128);

    create_payment(
        &client,
        &sender,
        &recipient,
        &token_addr,
        amount,
        interval,
        start_time,
    );

    env.ledger().set_timestamp(start_time + interval * 2 + 500);
    client.execute_payment(&1);

    let payment = client.get_payment(&1);
    assert_eq!(payment.next_execution, start_time + 3 * interval);
    assert_eq!(token_client.balance(&recipient), 1000);
    assert_eq!(payment.execution_count, 1);
}

#[test]
fn test_paused_payment_does_not_execute() {
    let (env, sender, recipient, token_addr, token_client, token_admin_client, client) =
        setup_test_env();

    let payment_id = create_payment(&client, &sender, &recipient, &token_addr, 100, 3600, 1000);
    token_admin_client.mint(&sender, &500);
    client.pause_payment(&payment_id);

    env.ledger().set_timestamp(1000);
    let result = catch_unwind(AssertUnwindSafe(|| {
        client.execute_payment(&payment_id);
    }));
    assert!(result.is_err());

    let payment = client.get_payment(&payment_id);
    assert!(payment.active);
    assert!(payment.paused);
    assert_eq!(payment.execution_count, 0);
    assert_eq!(payment.next_execution, 1000);
    assert_eq!(token_client.balance(&sender), 500);
    assert_eq!(token_client.balance(&recipient), 0);
}

#[test]
fn test_resume_payment_restores_execution() {
    let (env, sender, recipient, token_addr, token_client, token_admin_client, client) =
        setup_test_env();

    let payment_id = create_payment(&client, &sender, &recipient, &token_addr, 100, 3600, 1000);
    token_admin_client.mint(&sender, &500);
    client.pause_payment(&payment_id);

    let paused = client.get_payment(&payment_id);
    assert!(paused.paused);

    client.resume_payment(&payment_id);
    let resumed = client.get_payment(&payment_id);
    assert!(resumed.active);
    assert!(!resumed.paused);

    env.ledger().set_timestamp(1000);
    client.execute_payment(&payment_id);

    let payment = client.get_payment(&payment_id);
    assert_eq!(payment.execution_count, 1);
    assert_eq!(payment.next_execution, 4600);
    assert_eq!(token_client.balance(&sender), 400);
    assert_eq!(token_client.balance(&recipient), 100);
}

#[test]
#[should_panic(expected = "Payment is not paused")]
fn test_resume_unpaused_payment_panics() {
    let (_env, sender, recipient, token_addr, _token_client, _token_admin_client, client) =
        setup_test_env();

    let payment_id = create_payment(&client, &sender, &recipient, &token_addr, 100, 3600, 1000);
    client.resume_payment(&payment_id);
}

#[test]
#[should_panic(expected = "Payment is not active")]
fn test_cancelled_payment_cannot_resume() {
    let (_env, sender, recipient, token_addr, _token_client, _token_admin_client, client) =
        setup_test_env();

    let payment_id = create_payment(&client, &sender, &recipient, &token_addr, 100, 3600, 1000);
    client.cancel_payment(&payment_id);
    client.resume_payment(&payment_id);
}

#[test]
fn test_missed_payment_increments_count() {
    let (env, sender, recipient, token_addr, _token_client, token_admin_client, client) =
        setup_test_env();

    token_admin_client.mint(&sender, &50);
    let payment_id = create_payment(&client, &sender, &recipient, &token_addr, 100, 3600, 1000);

    env.ledger().set_timestamp(1000);
    client.execute_payment(&payment_id);

    let payment = client.get_payment(&payment_id);
    assert_eq!(payment.missed_count, 1);
    assert_eq!(payment.last_missed_at, 1000);
    assert_eq!(payment.execution_count, 0);
}

#[test]
fn test_successful_execution_resets_missed_count() {
    let (env, sender, recipient, token_addr, _token_client, token_admin_client, client) =
        setup_test_env();

    let payment_id = create_payment(&client, &sender, &recipient, &token_addr, 100, 3600, 1000);

    token_admin_client.mint(&sender, &50);
    env.ledger().set_timestamp(1000);
    client.execute_payment(&payment_id);

    token_admin_client.mint(&sender, &200);
    client.execute_payment(&payment_id);

    let payment = client.get_payment(&payment_id);
    assert_eq!(payment.missed_count, 0);
    assert_eq!(payment.last_missed_at, 0);
    assert_eq!(payment.execution_count, 1);
}

#[test]
fn test_missed_count_increments_multiple_times() {
    let (env, sender, recipient, token_addr, _token_client, token_admin_client, client) =
        setup_test_env();

    token_admin_client.mint(&sender, &10);
    let payment_id = create_payment(&client, &sender, &recipient, &token_addr, 100, 1, 1000);

    env.ledger().set_timestamp(1000);
    client.execute_payment(&payment_id);
    client.execute_payment(&payment_id);

    let payment = client.get_payment(&payment_id);
    assert_eq!(payment.missed_count, 2);
    assert_eq!(payment.last_missed_at, 1000);
}

#![cfg(test)]

extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

fn create_token_contract(
    env: &Env,
    admin: &Address,
) -> (
    Address,
    token::Client<'static>,
    token::StellarAssetClient<'static>,
) {
    let stellar_asset = env.register_stellar_asset_contract_v2(admin.clone());
    let token_address = stellar_asset.address();
    let token_client = token::Client::new(env, &token_address);
    let admin_client = token::StellarAssetClient::new(env, &token_address);
    (token_address, token_client, admin_client)
}

#[test]
fn test_recurring_payment_flow() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);

    let (token_addr, token_client, token_admin_client) = create_token_contract(&env, &admin);
    let amount = 1000i128;
    let interval = 3600u64;
    let start_time = 1000u64;

    token_admin_client.mint(&sender, &5000i128);

    let contract_id = env.register(RecurringPaymentContract, ());
    let client = RecurringPaymentContractClient::new(&env, &contract_id);

    let payment_id = client.create_payment(
        &sender,
        &recipient,
        &token_addr,
        &amount,
        &interval,
        &start_time,
    );
    assert_eq!(payment_id, 1);

    let payment = client.get_payment(&payment_id);
    assert_eq!(payment.amount, amount);
    assert_eq!(payment.next_execution, start_time);
    assert!(payment.active);
    assert!(!payment.paused);
    assert_eq!(payment.execution_count, 0);
    assert_eq!(payment.missed_count, 0);
    assert_eq!(payment.last_missed_at, 0);

    env.ledger().set_timestamp(start_time - 1);
    // client.execute_payment(&payment_id); // should panic

    env.ledger().set_timestamp(start_time);
    client.execute_payment(&payment_id);

    assert_eq!(token_client.balance(&sender), 4000);
    assert_eq!(token_client.balance(&recipient), 1000);

    let payment = client.get_payment(&payment_id);
    assert_eq!(payment.next_execution, start_time + interval);
    assert_eq!(payment.execution_count, 1);
    assert_eq!(payment.missed_count, 0);
    assert_eq!(payment.last_missed_at, 0);

    client.cancel_payment(&payment_id);
    let payment = client.get_payment(&payment_id);
    assert!(!payment.active);

    env.ledger().set_timestamp(start_time + interval);
    // client.execute_payment(&payment_id); // should panic
}

#[test]
#[should_panic(expected = "Payment is paused")]
fn test_paused_schedule_does_not_execute() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);

    let (token_addr, _token_client, token_admin_client) = create_token_contract(&env, &admin);
    let amount = 1000i128;
    let interval = 3600u64;
    let start_time = 1000u64;

    token_admin_client.mint(&sender, &5000i128);

    let contract_id = env.register(RecurringPaymentContract, ());
    let client = RecurringPaymentContractClient::new(&env, &contract_id);

    let payment_id = client.create_payment(
        &sender,
        &recipient,
        &token_addr,
        &amount,
        &interval,
        &start_time,
    );

    client.pause_payment(&payment_id);
    let paused_payment = client.get_payment(&payment_id);
    assert!(paused_payment.paused);
    assert!(paused_payment.active);

    env.ledger().set_timestamp(start_time);
    client.execute_payment(&payment_id);
}

#[test]
fn test_resume_restores_execution_after_pause() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);

    let (token_addr, token_client, token_admin_client) = create_token_contract(&env, &admin);
    let amount = 1000i128;
    let interval = 3600u64;
    let start_time = 1000u64;

    token_admin_client.mint(&sender, &5000i128);

    let contract_id = env.register(RecurringPaymentContract, ());
    let client = RecurringPaymentContractClient::new(&env, &contract_id);

    let payment_id = client.create_payment(
        &sender,
        &recipient,
        &token_addr,
        &amount,
        &interval,
        &start_time,
    );

    client.pause_payment(&payment_id);
    client.resume_payment(&payment_id);

    let resumed_payment = client.get_payment(&payment_id);
    assert!(!resumed_payment.paused);
    assert!(resumed_payment.active);

    env.ledger().set_timestamp(start_time);
    client.execute_payment(&payment_id);

    assert_eq!(token_client.balance(&sender), 4000);
    assert_eq!(token_client.balance(&recipient), 1000);

    let payment_after_resume = client.get_payment(&payment_id);
    assert_eq!(payment_after_resume.execution_count, 1);
    assert_eq!(payment_after_resume.next_execution, start_time + interval);
}

#[test]
#[should_panic(expected = "Amount must be positive")]
fn test_create_with_zero_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let token = Address::generate(&env);

    let contract_id = env.register(RecurringPaymentContract, ());
    let client = RecurringPaymentContractClient::new(&env, &contract_id);

    client.create_payment(&sender, &recipient, &token, &0, &3600, &1000);
}

#[test]
fn test_execute_with_delay() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);

    let (token_addr, token_client, token_admin_client) = create_token_contract(&env, &admin);
    let amount = 1000i128;
    let interval = 3600u64;
    let start_time = 1000u64;

    token_admin_client.mint(&sender, &5000i128);

    let contract_id = env.register(RecurringPaymentContract, ());
    let client = RecurringPaymentContractClient::new(&env, &contract_id);

    client.create_payment(
        &sender,
        &recipient,
        &token_addr,
        &amount,
        &interval,
        &start_time,
    );

    env.ledger().set_timestamp(start_time + interval * 2 + 500);
    client.execute_payment(&1);

    let payment = client.get_payment(&1);
    assert_eq!(payment.next_execution, start_time + 3 * interval);
    assert_eq!(token_client.balance(&recipient), 1000);
    assert_eq!(payment.execution_count, 1);
}

#[test]
fn test_missed_payment_increments_count() {
    let env = Env::default();
    env.mock_all_auths();

    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) =
        create_token_contract(&env, &token_admin);

    token_admin_client.mint(&sender, &50);

    let contract_id = env.register(RecurringPaymentContract, ());
    let client = RecurringPaymentContractClient::new(&env, &contract_id);

    let payment_id = client.create_payment(&sender, &recipient, &token_address, &100, &3600, &0);

    env.ledger().set_timestamp(1);
    client.execute_payment(&payment_id);

    let payment = client.get_payment(&payment_id);
    assert_eq!(payment.missed_count, 1);
    assert!(payment.last_missed_at > 0);
}

#[test]
fn test_successful_execution_resets_missed_count() {
    let env = Env::default();
    env.mock_all_auths();

    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) =
        create_token_contract(&env, &token_admin);

    let contract_id = env.register(RecurringPaymentContract, ());
    let client = RecurringPaymentContractClient::new(&env, &contract_id);

    let payment_id = client.create_payment(&sender, &recipient, &token_address, &100, &3600, &0);

    token_admin_client.mint(&sender, &50);
    client.execute_payment(&payment_id);

    env.ledger().set_timestamp(3600);
    token_admin_client.mint(&sender, &200);
    client.execute_payment(&payment_id);

    let payment = client.get_payment(&payment_id);
    assert_eq!(payment.missed_count, 0);
    assert_eq!(payment.last_missed_at, 0);
}

#[test]
fn test_missed_count_increments_multiple_times() {
    let env = Env::default();
    env.mock_all_auths();

    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) =
        create_token_contract(&env, &token_admin);

    token_admin_client.mint(&sender, &10);

    let contract_id = env.register(RecurringPaymentContract, ());
    let client = RecurringPaymentContractClient::new(&env, &contract_id);

    let payment_id = client.create_payment(&sender, &recipient, &token_address, &100, &1, &0);

    client.execute_payment(&payment_id);
    env.ledger().set_timestamp(1);
    client.execute_payment(&payment_id);

    let payment = client.get_payment(&payment_id);
    assert_eq!(payment.missed_count, 2);
}

#[test]
fn test_get_recurring_payment_schedule_returns_schedule_details() {
    let env = Env::default();
    env.mock_all_auths();

    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let token = Address::generate(&env);

    let contract_id = env.register(RecurringPaymentContract, ());
    let client = RecurringPaymentContractClient::new(&env, &contract_id);

    let amount = 250i128;
    let interval = 86_400u64;
    let start_time = 1_700_000_000u64;
    let payment_id =
        client.create_payment(&sender, &recipient, &token, &amount, &interval, &start_time);

    let schedule = client.get_recurring_payment_schedule(&payment_id);

    assert_eq!(schedule.amount, amount);
    assert_eq!(schedule.interval, interval);
    assert_eq!(schedule.next_due_date, start_time);
}

#[test]
#[should_panic(expected = "Payment not found")]
fn test_get_recurring_payment_schedule_panics_for_unknown_id() {
    let env = Env::default();
    let contract_id = env.register(RecurringPaymentContract, ());
    let client = RecurringPaymentContractClient::new(&env, &contract_id);

    client.get_recurring_payment_schedule(&999);
}

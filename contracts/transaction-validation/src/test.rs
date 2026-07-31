#![cfg(test)]

use crate::{
    validation::{
        validate_transaction_timestamp, TimestampValidationError, MAX_FUTURE_THRESHOLD,
        MAX_PAST_THRESHOLD,
    },
    TransactionValidationContract, TransactionValidationContractClient,
};
use soroban_sdk::{testutils::Ledger, Env};

#[test]
fn test_valid_timestamp_exact() {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    assert_eq!(validate_transaction_timestamp(&env, 1000), Ok(()));
}

#[test]
fn test_valid_future_timestamp_within_bounds() {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    // 100 seconds in the future is fine
    assert_eq!(validate_transaction_timestamp(&env, 1100), Ok(()));
}

#[test]
fn test_invalid_future_timestamp_beyond_bounds() {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    let too_far_future = 1000 + MAX_FUTURE_THRESHOLD + 1;
    assert_eq!(
        validate_transaction_timestamp(&env, too_far_future),
        Err(TimestampValidationError::FutureTimestamp)
    );
}

#[test]
fn test_valid_past_timestamp_within_bounds() {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    // 100 seconds in the past is fine
    assert_eq!(validate_transaction_timestamp(&env, 900), Ok(()));
}

#[test]
fn test_invalid_past_timestamp_beyond_bounds() {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    let too_far_past = 1000 - MAX_PAST_THRESHOLD - 1;
    assert_eq!(
        validate_transaction_timestamp(&env, too_far_past),
        Err(TimestampValidationError::PastTimestamp)
    );
}

#[test]
fn test_get_validation_errors_returns_error_codes_for_failed_transaction() {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    let contract_id = env.register_contract(None, TransactionValidationContract);
    let client = TransactionValidationContractClient::new(&env, &contract_id);

    let result = client.process_transaction(&1u64, &(1000 + MAX_FUTURE_THRESHOLD + 1));
    assert_eq!(result, Err(TimestampValidationError::FutureTimestamp));

    let errors = client.get_validation_errors(&1u64);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors.get(0).unwrap(), TimestampValidationError::FutureTimestamp as u32);
}

#[test]
fn test_get_validation_errors_returns_empty_for_valid_or_unknown_transactions() {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    let contract_id = env.register_contract(None, TransactionValidationContract);
    let client = TransactionValidationContractClient::new(&env, &contract_id);

    let valid_result = client.process_transaction(&2u64, &1000u64);
    assert_eq!(valid_result, Ok(()));

    let valid_errors = client.get_validation_errors(&2u64);
    assert_eq!(valid_errors.len(), 0);

    let unknown_errors = client.get_validation_errors(&999u64);
    assert_eq!(unknown_errors.len(), 0);
}

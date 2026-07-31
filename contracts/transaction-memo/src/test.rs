//! Comprehensive unit tests for the transaction memo contract.

#![cfg(test)]

use crate::{TransactionMemoContract, TransactionMemoContractClient};
use soroban_sdk::{testutils::Address as _, Address, Bytes, Env, String, Symbol};

/// Helper function to create a test environment with initialized contract.
fn setup_test_contract() -> (Env, Address, TransactionMemoContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(TransactionMemoContract, ());
    let client = TransactionMemoContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    (env, admin, client)
}

fn tx_id(env: &Env, s: &str) -> Bytes {
    let mut b = Bytes::new(env);
    b.extend_from_slice(s.as_bytes());
    b
}

#[test]
fn test_initialize() {
    let (_, admin, client) = setup_test_contract();

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_total_memos(), 0);
}

#[test]
#[should_panic(expected = "Contract already initialized")]
fn test_initialize_twice_fails() {
    let (env, _, client) = setup_test_contract();
    let new_admin = Address::generate(&env);
    client.initialize(&new_admin);
}

#[test]
fn test_set_and_get_memo() {
    let (env, admin, client) = setup_test_contract();

    let id = tx_id(&env, "tx-1");
    let memo_type = Symbol::new(&env, "payment");
    let reference = String::from_str(&env, "ref-123");
    let text = String::from_str(&env, "Payment for services");

    client.set_memo(&admin, &id, &memo_type, &reference, &text);

    let memo = client.get_memo(&id).unwrap();
    assert_eq!(memo.memo_type, memo_type);
    assert_eq!(memo.reference, reference);
    assert_eq!(memo.text, text);
    assert_eq!(client.get_total_memos(), 1);
}

#[test]
#[should_panic]
fn test_memo_text_too_large() {
    let (env, admin, client) = setup_test_contract();

    let id = tx_id(&env, "tx-oversized");
    let memo_type = Symbol::new(&env, "note");
    let reference = String::from_str(&env, "");
    // Create text exceeding 256 bytes
    let long_text = "a".repeat(300);
    let text = String::from_str(&env, &long_text);

    client.set_memo(&admin, &id, &memo_type, &reference, &text);
}

#[test]
#[should_panic]
fn test_memo_empty_text() {
    let (env, admin, client) = setup_test_contract();

    let id = tx_id(&env, "tx-empty");
    let memo_type = Symbol::new(&env, "note");
    let reference = String::from_str(&env, "");
    let text = String::from_str(&env, "");

    client.set_memo(&admin, &id, &memo_type, &reference, &text);
}

#[test]
#[should_panic]
fn test_memo_reference_too_large() {
    let (env, admin, client) = setup_test_contract();

    let id = tx_id(&env, "tx-longref");
    let memo_type = Symbol::new(&env, "note");
    let reference = String::from_str(&env, &"r".repeat(100));
    let text = String::from_str(&env, "Valid text");

    client.set_memo(&admin, &id, &memo_type, &reference, &text);
}

#[test]
fn test_memo_text_at_max_length() {
    let (env, admin, client) = setup_test_contract();

    let id = tx_id(&env, "tx-max");
    let memo_type = Symbol::new(&env, "note");
    let reference = String::from_str(&env, "");
    // Exactly 256 bytes
    let text = String::from_str(&env, &"a".repeat(256));

    client.set_memo(&admin, &id, &memo_type, &reference, &text);

    let memo = client.get_memo(&id).unwrap();
    assert_eq!(memo.text.len(), 256);
}

#[test]
fn test_memo_reference_at_max_length() {
    let (env, admin, client) = setup_test_contract();

    let id = tx_id(&env, "tx-refmax");
    let memo_type = Symbol::new(&env, "x");
    let reference = String::from_str(&env, &"r".repeat(64));
    let text = String::from_str(&env, "Short");

    client.set_memo(&admin, &id, &memo_type, &reference, &text);

    let memo = client.get_memo(&id).unwrap();
    assert_eq!(memo.reference.len(), 64);
}

#[test]
#[should_panic]
fn test_total_memo_too_large() {
    let (env, admin, client) = setup_test_contract();

    let id = tx_id(&env, "tx-total");
    let memo_type = Symbol::new(&env, &"t".repeat(32));
    let reference = String::from_str(&env, &"r".repeat(64));
    let text = String::from_str(&env, &"a".repeat(256));
    // Total: 32 + 64 + 256 = 352 > 320 MAX

    client.set_memo(&admin, &id, &memo_type, &reference, &text);
}

#[test]
fn test_get_nonexistent_memo() {
    let (env, _, client) = setup_test_contract();

    let id = tx_id(&env, "nonexistent");
    let result = client.get_memo(&id);
    assert!(result.is_none());
}

#[test]
fn test_set_admin() {
    let (env, admin, client) = setup_test_contract();
    let new_admin = Address::generate(&env);

    client.set_admin(&admin, &new_admin);
    assert_eq!(client.get_admin(), new_admin);
}

#[test]
#[should_panic]
fn test_empty_memo_type() {
    let (env, admin, client) = setup_test_contract();

    let id = tx_id(&env, "tx-notype");
    let memo_type = Symbol::new(&env, "");
    let reference = String::from_str(&env, "");
    let text = String::from_str(&env, "Valid text");

    client.set_memo(&admin, &id, &memo_type, &reference, &text);
}

#[test]
fn test_get_transaction_memo_known_tx() {
    let (env, admin, client) = setup_test_contract();

    let id = tx_id(&env, "tx-memotext");
    let memo_type = Symbol::new(&env, "payment");
    let reference = String::from_str(&env, "ref-001");
    let text = String::from_str(&env, "Payment for coffee");

    client.set_memo(&admin, &id, &memo_type, &reference, &text);

    let memo_text = client.get_transaction_memo(&id);
    assert_eq!(memo_text, Some(text));
}

#[test]
fn test_get_transaction_memo_unknown_tx() {
    let (env, _, client) = setup_test_contract();

    let id = tx_id(&env, "unknown-tx");
    let result = client.get_transaction_memo(&id);
    assert!(result.is_none());
}

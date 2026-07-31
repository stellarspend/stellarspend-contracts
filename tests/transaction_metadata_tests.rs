#![cfg(test)]

use soroban_sdk::{testutils::Events, Bytes, Env, Map, String, Symbol};

use crate::transaction_metadata::{TransactionMetadata, TransactionMetadataContract};

#[test]
fn test_store_and_retrieve_transaction_metadata() {
    let env = Env::default();

    let transaction_id = Bytes::from_array(&env, &[1u8; 32]);

    let mut metadata = Map::new(&env);
    metadata.set(Symbol::short("type"), String::from_str(&env, "donation"));
    metadata.set(Symbol::short("note"), String::from_str(&env, "health fund"));

    TransactionMetadataContract::set_metadata(
        env.clone(),
        transaction_id.clone(),
        metadata.clone(),
    );

    let stored = TransactionMetadataContract::get_metadata(env.clone(), transaction_id.clone())
        .expect("metadata should exist");

    assert_eq!(stored.data, metadata);
}

#[test]
#[should_panic(expected = "Metadata exceeds maximum allowed size")]
fn test_transaction_metadata_size_limit() {
    let env = Env::default();

    let transaction_id = Bytes::from_array(&env, &[2u8; 32]);

    let mut metadata = Map::new(&env);

    let oversized = String::from_str(&env, &"a".repeat(2000));
    metadata.set(Symbol::short("big"), oversized);

    TransactionMetadataContract::set_metadata(env.clone(), transaction_id, metadata);
}

#[test]
fn test_transaction_metadata_event_emitted() {
    let env = Env::default();

    let transaction_id = Bytes::from_array(&env, &[3u8; 32]);

    let mut metadata = Map::new(&env);
    metadata.set(
        Symbol::short("category"),
        String::from_str(&env, "education"),
    );

    TransactionMetadataContract::set_metadata(env.clone(), transaction_id, metadata);

    let events = env.events().all();
    assert_eq!(events.len(), 1);
}

#[test]
fn test_get_transaction_metadata_by_u64_id() {
    let env = Env::default();

    let tx_id: u64 = 12345;
    let transaction_id = Bytes::from_slice(&env, &tx_id.to_be_bytes());

    let mut metadata = Map::new(&env);
    metadata.set(Symbol::short("key"), String::from_str(&env, "value"));

    TransactionMetadataContract::set_metadata(env.clone(), transaction_id, metadata.clone());

    // Known tx_id should return Some(metadata)
    let stored = TransactionMetadataContract::get_transaction_metadata(env.clone(), tx_id)
        .expect("metadata should exist for known tx_id");
    assert_eq!(stored.data, metadata);

    // Unknown tx_id should return None
    let unknown_tx_id: u64 = 99999;
    let not_found = TransactionMetadataContract::get_transaction_metadata(env.clone(), unknown_tx_id);
    assert!(not_found.is_none());
}

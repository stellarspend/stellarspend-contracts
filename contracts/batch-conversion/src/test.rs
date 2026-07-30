//! Integration tests for the Batch Conversion Contract.

#![cfg(test)]

use crate::{
    BatchConversionContract, BatchConversionContractClient, ConversionRequest, ConversionResult,
};
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger},
    token, Address, Env, Vec,
};

/// Creates a test environment with the contract deployed and initialized.
fn setup_test_env() -> (
    Env,
    Address,
    token::Client<'static>,
    token::StellarAssetClient<'static>,
    Address,
    token::Client<'static>,
    BatchConversionContractClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| {
        li.sequence_number = 12345;
    });

    // Deploy from_asset token contract
    let from_asset_admin = Address::generate(&env);
    let from_asset: Address = env
        .register_stellar_asset_contract_v2(from_asset_admin.clone())
        .address();
    let from_token_client = token::Client::new(&env, &from_asset);
    let from_token_admin_client = token::StellarAssetClient::new(&env, &from_asset);

    // Deploy to_asset token contract
    let to_asset_admin = Address::generate(&env);
    let to_asset: Address = env
        .register_stellar_asset_contract_v2(to_asset_admin.clone())
        .address();
    let to_token_client = token::Client::new(&env, &to_asset);

    // Deploy batch conversion contract
    let contract_id = env.register(BatchConversionContract, ());
    let client = BatchConversionContractClient::new(&env, &contract_id);

    // Initialize (not required for batch processing, but keeps counters explicit)
    let admin = Address::generate(&env);
    client.initialize(&admin);

    (
        env,
        from_asset,
        from_token_client,
        from_token_admin_client,
        to_asset,
        to_token_client,
        client,
    )
}

fn create_conversion_request(
    user: Address,
    from_asset: Address,
    to_asset: Address,
    amount_in: i128,
    min_amount_out: i128,
) -> ConversionRequest {
    ConversionRequest {
        user,
        from_asset,
        to_asset,
        amount_in,
        min_amount_out,
    }
}

#[test]
fn test_batch_convert_single_success() {
    let (
        env,
        from_asset,
        _from_token_client,
        from_token_admin_client,
        to_asset,
        _to_token_client,
        client,
    ) = setup_test_env();

    let user = Address::generate(&env);
    from_token_admin_client.mint(&user, &1000);

    let mut conversions: Vec<ConversionRequest> = Vec::new(&env);
    conversions.push_back(create_conversion_request(
        user.clone(),
        from_asset.clone(),
        to_asset.clone(),
        100,
        90,
    ));

    let result = client.batch_convert_currency(&conversions);

    assert_eq!(result.total_requests, 1);
    assert_eq!(result.successful, 1);
    assert_eq!(result.failed, 0);
    assert_eq!(result.total_converted, 100);
    assert_eq!(result.results.len(), 1);

    match result.results.get(0).unwrap() {
        ConversionResult::Success(u, f, t, amount_in, amount_out) => {
            assert_eq!(u.clone(), user);
            assert_eq!(f.clone(), from_asset);
            assert_eq!(t.clone(), to_asset);
            assert_eq!(amount_in, 100);
            assert_eq!(amount_out, 90);
        }
        _ => panic!("Expected success"),
    }
}

#[test]
fn test_batch_convert_partial_failures_validation() {
    let (
        env,
        from_asset,
        _from_token_client,
        from_token_admin_client,
        to_asset,
        _to_token_client,
        client,
    ) = setup_test_env();

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    from_token_admin_client.mint(&user1, &1000);
    from_token_admin_client.mint(&user2, &1000);

    let mut conversions: Vec<ConversionRequest> = Vec::new(&env);
    conversions.push_back(create_conversion_request(
        user1.clone(),
        from_asset.clone(),
        to_asset.clone(),
        -1,
        90,
    ));
    conversions.push_back(create_conversion_request(
        user2.clone(),
        from_asset.clone(),
        to_asset.clone(),
        100,
        90,
    ));

    let result = client.batch_convert_currency(&conversions);
    assert_eq!(result.total_requests, 2);
    assert_eq!(result.successful, 1);
    assert_eq!(result.failed, 1);
    assert_eq!(result.total_converted, 100);

    match result.results.get(0).unwrap() {
        ConversionResult::Failure(user, _from, _to, amount_in, error_code) => {
            assert_eq!(user.clone(), user1);
            assert_eq!(amount_in, -1);
            assert_eq!(error_code, 3); // invalid amount_in
        }
        _ => panic!("Expected failure"),
    }
}

#[test]
fn test_batch_convert_same_asset_rejected() {
    let (
        env,
        from_asset,
        _from_token_client,
        from_token_admin_client,
        _to_asset,
        _to_token_client,
        client,
    ) = setup_test_env();

    let user = Address::generate(&env);
    from_token_admin_client.mint(&user, &1000);

    let mut conversions: Vec<ConversionRequest> = Vec::new(&env);
    conversions.push_back(create_conversion_request(
        user.clone(),
        from_asset.clone(),
        from_asset.clone(),
        100,
        90,
    ));

    let result = client.batch_convert_currency(&conversions);
    assert_eq!(result.total_requests, 1);
    assert_eq!(result.successful, 0);
    assert_eq!(result.failed, 1);
    assert_eq!(result.total_converted, 0);

    match result.results.get(0).unwrap() {
        ConversionResult::Failure(_user, _from, _to, _amount_in, error_code) => {
            assert_eq!(error_code, 5); // same asset
        }
        _ => panic!("Expected failure"),
    }
}

#[test]
fn test_batch_convert_events_emitted() {
    let (
        env,
        from_asset,
        _from_token_client,
        from_token_admin_client,
        to_asset,
        _to_token_client,
        client,
    ) = setup_test_env();

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    from_token_admin_client.mint(&user1, &1000);
    from_token_admin_client.mint(&user2, &1000);

    let mut conversions: Vec<ConversionRequest> = Vec::new(&env);
    conversions.push_back(create_conversion_request(
        user1,
        from_asset.clone(),
        to_asset.clone(),
        100,
        90,
    ));
    conversions.push_back(create_conversion_request(
        user2,
        from_asset.clone(),
        to_asset.clone(),
        -1,
        90,
    ));

    client.batch_convert_currency(&conversions);

    let events = env.events().all();
    // Should have: batch_started, conversion_success (1), conversion_failure (1), batch_completed
    assert!(events.len() >= 4);
}

#[test]
fn test_batch_convert_accumulates_stats() {
    let (
        env,
        from_asset,
        _from_token_client,
        from_token_admin_client,
        to_asset,
        _to_token_client,
        client,
    ) = setup_test_env();

    let user = Address::generate(&env);
    from_token_admin_client.mint(&user, &10_000);

    let mut batch1: Vec<ConversionRequest> = Vec::new(&env);
    batch1.push_back(create_conversion_request(
        user.clone(),
        from_asset.clone(),
        to_asset.clone(),
        100,
        90,
    ));

    let mut batch2: Vec<ConversionRequest> = Vec::new(&env);
    batch2.push_back(create_conversion_request(
        user.clone(),
        from_asset.clone(),
        to_asset.clone(),
        200,
        180,
    ));

    assert_eq!(client.get_total_batches(), 0);
    assert_eq!(client.get_total_conversions_processed(), 0);
    assert_eq!(client.get_total_volume_converted(), 0);

    client.batch_convert_currency(&batch1);
    assert_eq!(client.get_total_batches(), 1);
    assert_eq!(client.get_total_conversions_processed(), 1);
    assert_eq!(client.get_total_volume_converted(), 100);

    client.batch_convert_currency(&batch2);
    assert_eq!(client.get_total_batches(), 2);
    assert_eq!(client.get_total_conversions_processed(), 2);
    assert_eq!(client.get_total_volume_converted(), 300);
}

#[test]
#[should_panic]
fn test_batch_convert_empty_batch() {
    let (
        env,
        _from_asset,
        _from_token_client,
        _from_token_admin_client,
        _to_asset,
        _to_token_client,
        client,
    ) = setup_test_env();

    let conversions: Vec<ConversionRequest> = Vec::new(&env);
    client.batch_convert_currency(&conversions);
}

#[test]
fn test_get_batch_conversion_output_unknown_id() {
    let (_env, _, _, _, _, _, client) = setup_test_env();
    let output = client.get_batch_conversion_output(&9999);
    assert_eq!(output.len(), 0);
}

#[test]
fn test_get_batch_conversion_output_correct_amounts() {
    let (
        env,
        from_asset,
        _from_token_client,
        from_token_admin_client,
        to_asset,
        _to_token_client,
        client,
    ) = setup_test_env();

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    from_token_admin_client.mint(&user1, &1000);
    from_token_admin_client.mint(&user2, &1000);

    let mut conversions: Vec<ConversionRequest> = Vec::new(&env);
    // user1: valid — should produce amount_out = min_amount_out = 90
    conversions.push_back(create_conversion_request(
        user1.clone(),
        from_asset.clone(),
        to_asset.clone(),
        100,
        90,
    ));
    // user2: invalid amount — should produce 0
    conversions.push_back(create_conversion_request(
        user2.clone(),
        from_asset.clone(),
        to_asset.clone(),
        -1,
        80,
    ));

    client.batch_convert_currency(&conversions);
    let batch_id = client.get_total_batches();

    let output = client.get_batch_conversion_output(&batch_id);
    assert_eq!(output.len(), 2);
    assert_eq!(output.get(0).unwrap(), 90);
    assert_eq!(output.get(1).unwrap(), 0);
}

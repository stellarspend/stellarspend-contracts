//! Integration tests for the Batch Rewards Contract.

#![cfg(test)]

use crate::{BatchRewardsContract, BatchRewardsContractClient, RewardRequest, RewardResult};
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger},
    token, Address, Env, Vec,
};

/// Creates a test environment with the contract deployed and initialized.
fn setup_test_env() -> (
    Env,
    Address,
    Address,
    token::Client<'static>,
    BatchRewardsContractClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| {
        li.sequence_number = 12345;
    });

    // Deploy token contract (simulating XLM StellarAssetContract)
    let issuer = Address::generate(&env);
    let stellar_asset = env.register_stellar_asset_contract_v2(issuer.clone());
    let token_id: Address = stellar_asset.address();
    let token_client = token::Client::new(&env, &token_id);

    // Deploy batch rewards contract
    let contract_id = env.register(BatchRewardsContract, ());
    let client = BatchRewardsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    (env, admin, token_id, token_client, client)
}

/// Helper to create a reward request.
fn create_reward_request(_env: &Env, recipient: Address, amount: i128) -> RewardRequest {
    RewardRequest { recipient, amount }
}

// Initialization Tests

#[test]
fn test_initialize_contract() {
    let (_env, admin, _token, _token_client, client) = setup_test_env();

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_total_batches(), 0);
    assert_eq!(client.get_total_rewards_processed(), 0);
    assert_eq!(client.get_total_volume_distributed(), 0);
}

#[test]
#[should_panic(expected = "Contract already initialized")]
fn test_cannot_initialize_twice() {
    let (env, admin, _token, _token_client, client) = setup_test_env();

    let new_admin = Address::generate(&env);
    client.initialize(&new_admin);
}

#[test]
fn test_set_admin() {
    let (env, admin, _token, _token_client, client) = setup_test_env();

    let new_admin = Address::generate(&env);
    client.set_admin(&admin, &new_admin);

    assert_eq!(client.get_admin(), new_admin);
}

// Batch Distribution Tests

#[test]
fn test_distribute_rewards_single_recipient() {
    let (env, admin, token, token_client, client) = setup_test_env();

    let recipient = Address::generate(&env);
    let reward_amount: i128 = 10_000_000; // 1 XLM equivalent

    // Mint tokens to admin
    token_client.mint(&admin, &(reward_amount * 2));

    let mut rewards: Vec<RewardRequest> = Vec::new(&env);
    rewards.push_back(create_reward_request(&env, recipient.clone(), reward_amount));

    let result = client.distribute_rewards(&admin, &token, &rewards);

    assert_eq!(result.total_requests, 1);
    assert_eq!(result.successful, 1);
    assert_eq!(result.failed, 0);
    assert_eq!(result.total_distributed, reward_amount);
    assert_eq!(result.results.len(), 1);

    // Verify recipient received tokens
    assert_eq!(token_client.balance(&recipient), reward_amount);
}

#[test]
fn test_distribute_rewards_multiple_recipients() {
    let (env, admin, token, token_client, client) = setup_test_env();

    let recipient1 = Address::generate(&env);
    let recipient2 = Address::generate(&env);
    let recipient3 = Address::generate(&env);
    let amount: i128 = 5_000_000;

    // Mint tokens to admin
    token_client.mint(&admin, &(amount * 3 + 10_000_000));

    let mut rewards: Vec<RewardRequest> = Vec::new(&env);
    rewards.push_back(create_reward_request(&env, recipient1.clone(), amount));
    rewards.push_back(create_reward_request(&env, recipient2.clone(), amount));
    rewards.push_back(create_reward_request(&env, recipient3.clone(), amount));

    let result = client.distribute_rewards(&admin, &token, &rewards);

    assert_eq!(result.total_requests, 3);
    assert_eq!(result.successful, 3);
    assert_eq!(result.failed, 0);
    assert_eq!(result.total_distributed, amount * 3);

    // Verify each recipient received tokens
    assert_eq!(token_client.balance(&recipient1), amount);
    assert_eq!(token_client.balance(&recipient2), amount);
    assert_eq!(token_client.balance(&recipient3), amount);
}

#[test]
fn test_distribute_rewards_partial_failures() {
    let (env, admin, token, token_client, client) = setup_test_env();

    let recipient1 = Address::generate(&env);
    let recipient2 = Address::generate(&env);
    let valid_amount: i128 = 5_000_000;
    let invalid_amount: i128 = -1_000_000; // Invalid amount

    // Mint tokens to admin
    token_client.mint(&admin, &(valid_amount * 2));

    let mut rewards: Vec<RewardRequest> = Vec::new(&env);
    rewards.push_back(create_reward_request(&env, recipient1.clone(), valid_amount));
    rewards.push_back(create_reward_request(&env, recipient2.clone(), invalid_amount));

    let result = client.distribute_rewards(&admin, &token, &rewards);

    assert_eq!(result.total_requests, 2);
    assert_eq!(result.successful, 1);
    assert_eq!(result.failed, 1);
    assert_eq!(result.total_distributed, valid_amount);

    // Verify correct recipient got tokens
    assert_eq!(token_client.balance(&recipient1), valid_amount);
    assert_eq!(token_client.balance(&recipient2), 0);
}

#[test]
fn test_distribute_rewards_accumulates_stats() {
    let (env, admin, token, token_client, client) = setup_test_env();

    let recipient1 = Address::generate(&env);
    let recipient2 = Address::generate(&env);
    let amount: i128 = 5_000_000;

    // Mint tokens to admin
    token_client.mint(&admin, &(amount * 4 + 10_000_000));

    // First batch
    let mut rewards = Vec::new(&env);
    rewards.push_back(create_reward_request(&env, recipient1.clone(), amount));
    rewards.push_back(create_reward_request(&env, recipient2.clone(), amount));

    let result1 = client.distribute_rewards(&admin, &token, &rewards);
    assert_eq!(result1.total_distributed, amount * 2);

    // Check stats after first batch
    assert_eq!(client.get_total_batches(), 1);
    assert_eq!(client.get_total_rewards_processed(), 2);
    assert_eq!(client.get_total_volume_distributed(), amount * 2);

    // Second batch
    let mut rewards = Vec::new(&env);
    rewards.push_back(create_reward_request(&env, recipient1.clone(), amount));
    rewards.push_back(create_reward_request(&env, recipient2.clone(), amount));

    let result2 = client.distribute_rewards(&admin, &token, &rewards);
    assert_eq!(result2.total_distributed, amount * 2);

    // Check accumulated stats
    assert_eq!(client.get_total_batches(), 2);
    assert_eq!(client.get_total_rewards_processed(), 4);
    assert_eq!(client.get_total_volume_distributed(), amount * 4);
}

#[test]
fn test_distribute_rewards_large_batch() {
    let (env, admin, token, token_client, client) = setup_test_env();

    let amount: i128 = 1_000_000;
    let batch_size = 50u32;

    // Mint tokens to admin
    token_client.mint(&admin, &(amount * batch_size as i128 + 10_000_000));

    // Create batch of rewards
    let mut rewards: Vec<RewardRequest> = Vec::new(&env);
    for i in 0..batch_size {
        let recipient = Address::generate(&env);
        rewards.push_back(create_reward_request(&env, recipient, amount));
    }

    let result = client.distribute_rewards(&admin, &token, &rewards);

    assert_eq!(result.total_requests, batch_size);
    assert_eq!(result.successful, batch_size);
    assert_eq!(result.failed, 0);
    assert_eq!(result.total_distributed, amount * batch_size as i128);
}

#[test]
#[should_panic(expected = "EmptyBatch")]
fn test_distribute_rewards_empty_batch() {
    let (env, admin, token, _token_client, client) = setup_test_env();

    let rewards: Vec<RewardRequest> = Vec::new(&env);
    client.distribute_rewards(&admin, &token, &rewards);
}

#[test]
#[should_panic(expected = "BatchTooLarge")]
fn test_distribute_rewards_batch_too_large() {
    let (env, admin, token, token_client, client) = setup_test_env();

    let amount: i128 = 1_000_000;
    let batch_size = 101u32; // Exceeds MAX_BATCH_SIZE of 100

    // Mint tokens to admin
    token_client.mint(&admin, &(amount * batch_size as i128 + 10_000_000));

    // Create oversized batch
    let mut rewards: Vec<RewardRequest> = Vec::new(&env);
    for _ in 0..batch_size {
        let recipient = Address::generate(&env);
        rewards.push_back(create_reward_request(&env, recipient, amount));
    }

    client.distribute_rewards(&admin, &token, &rewards);
}

#[test]
#[should_panic(expected = "InsufficientBalance")]
fn test_distribute_rewards_insufficient_balance() {
    let (env, admin, token, token_client, client) = setup_test_env();

    let recipient = Address::generate(&env);
    let amount: i128 = 10_000_000;

    // Mint only half of what's needed
    token_client.mint(&admin, &(amount / 2));

    let mut rewards: Vec<RewardRequest> = Vec::new(&env);
    rewards.push_back(create_reward_request(&env, recipient, amount));

    client.distribute_rewards(&admin, &token, &rewards);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_distribute_rewards_unauthorized() {
    let (env, _admin, token, token_client, client) = setup_test_env();

    let unauthorized_caller = Address::generate(&env);
    let recipient = Address::generate(&env);
    let amount: i128 = 10_000_000;

    token_client.mint(&unauthorized_caller, &amount);

    let mut rewards: Vec<RewardRequest> = Vec::new(&env);
    rewards.push_back(create_reward_request(&env, recipient, amount));

    client.distribute_rewards(&unauthorized_caller, &token, &rewards);
}

#[test]
fn test_distribute_rewards_events_emitted() {
    let (env, admin, token, token_client, client) = setup_test_env();

    let recipient = Address::generate(&env);
    let amount: i128 = 10_000_000;

    token_client.mint(&admin, &(amount + 10_000_000));

    let mut rewards: Vec<RewardRequest> = Vec::new(&env);
    rewards.push_back(create_reward_request(&env, recipient.clone(), amount));

    client.distribute_rewards(&admin, &token, &rewards);

    // Verify events were emitted
    let events = env.events().all();
    assert!(events.len() > 0);

    // Check for batch_started event
    let has_batch_started = events.iter().any(|event| {
        event
            .topics
            .iter()
            .any(|topic| topic.to_string().contains("batch"))
    });
    assert!(has_batch_started, "batch_started event not found");

    // Check for reward_success event
    let has_reward_success = events.iter().any(|event| {
        event
            .topics
            .iter()
            .any(|topic| topic.to_string().contains("success"))
    });
    assert!(has_reward_success, "reward_success event not found");

    // Check for batch_completed event
    let has_batch_completed = events.iter().any(|event| {
        event
            .topics
            .iter()
            .any(|topic| topic.to_string().contains("completed"))
    });
    assert!(has_batch_completed, "batch_completed event not found");
}

#[test]
fn test_distribute_rewards_with_zero_amount() {
    let (env, admin, token, token_client, client) = setup_test_env();

    let recipient = Address::generate(&env);
    let valid_amount: i128 = 5_000_000;
    let zero_amount: i128 = 0;

    token_client.mint(&admin, &(valid_amount + 10_000_000));

    let mut rewards: Vec<RewardRequest> = Vec::new(&env);
    rewards.push_back(create_reward_request(&env, recipient.clone(), valid_amount));
    rewards.push_back(create_reward_request(&env, recipient.clone(), zero_amount));

    let result = client.distribute_rewards(&admin, &token, &rewards);

    assert_eq!(result.total_requests, 2);
    assert_eq!(result.successful, 1);
    assert_eq!(result.failed, 1);
}

#[test]
fn test_distribute_rewards_events_on_failure() {
    let (env, admin, token, token_client, client) = setup_test_env();

    let recipient = Address::generate(&env);
    let invalid_amount: i128 = -5_000_000;

    token_client.mint(&admin, &(10_000_000));

    let mut rewards: Vec<RewardRequest> = Vec::new(&env);
    rewards.push_back(create_reward_request(&env, recipient.clone(), invalid_amount));

    let result = client.distribute_rewards(&admin, &token, &rewards);

    assert_eq!(result.failed, 1);

    let events = env.events().all();
    
    // Check for failure event
    let has_failure_event = events.iter().any(|event| {
        event
            .topics
            .iter()
            .any(|topic| topic.to_string().contains("failure"))
    });
    assert!(has_failure_event, "reward_failure event not found");
}

#[test]
fn test_distribute_rewards_result_structure() {
    let (env, admin, token, token_client, client) = setup_test_env();

    let recipient1 = Address::generate(&env);
    let recipient2 = Address::generate(&env);
    let amount1: i128 = 5_000_000;
    let amount2: i128 = 3_000_000;

    token_client.mint(&admin, &(amount1 + amount2 + 10_000_000));

    let mut rewards: Vec<RewardRequest> = Vec::new(&env);
    rewards.push_back(create_reward_request(&env, recipient1.clone(), amount1));
    rewards.push_back(create_reward_request(&env, recipient2.clone(), amount2));

    let result = client.distribute_rewards(&admin, &token, &rewards);

    // Verify result structure
    assert_eq!(result.total_requests, 2);
    assert_eq!(result.successful, 2);
    assert_eq!(result.failed, 0);
    assert_eq!(result.total_distributed, amount1 + amount2);

    // Verify individual results
    match result.results.get(0).unwrap() {
        RewardResult::Success(addr, amt) => {
            assert_eq!(*addr, recipient1);
            assert_eq!(*amt, amount1);
        }
        _ => panic!("Expected success result"),
    }

    match result.results.get(1).unwrap() {
        RewardResult::Success(addr, amt) => {
            assert_eq!(*addr, recipient2);
            assert_eq!(*amt, amount2);
        }
        _ => panic!("Expected success result"),
    }
}

#[test]
fn test_multiple_simultaneous_batch_distributions() {
    let (env, admin, token, token_client, client) = setup_test_env();

    let recipients: Vec<Address> = (0..10)
        .map(|_| Address::generate(&env))
        .collect::<Vec<_>>();

    let amount: i128 = 2_000_000;
    
    // Mint sufficient tokens
    token_client.mint(&admin, &(amount * 30 + 10_000_000));

    // Execute 3 batches
    for _batch in 0..3 {
        let mut rewards: Vec<RewardRequest> = Vec::new(&env);
        for recipient in recipients.iter() {
            rewards.push_back(create_reward_request(&env, recipient.clone(), amount));
        }

        let result = client.distribute_rewards(&admin, &token, &rewards);
        assert_eq!(result.successful, 10);
        assert_eq!(result.total_distributed, amount * 10);
    }

    // Verify cumulative stats
    assert_eq!(client.get_total_batches(), 3);
    assert_eq!(client.get_total_rewards_processed(), 30);
    assert_eq!(client.get_total_volume_distributed(), amount * 30);

    // Verify each recipient received tokens
    for recipient in recipients.iter() {
        assert_eq!(token_client.balance(recipient), amount * 3);
    }
}

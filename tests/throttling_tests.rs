use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _, Ledger as _},
    Address, Env, TryFromVal, Vec,
};

#[path = "../contracts/throttling.rs"]
mod throttling;

use throttling::{
    GlobalThrottleStats, ThrottleConfig, ThrottleContract, ThrottleContractClient, ThrottleError,
    ThrottleReason, ThrottleResult, ThrottleViolation, TimeWindow, WalletThrottleState,
};

fn setup_throttle_contract() -> (Env, Address, ThrottleContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(ThrottleContract, ());
    let client = ThrottleContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let config = ThrottleConfig {
        max_transactions_per_window: 5,
        window_size_seconds: 60,
        block_duration_seconds: 30,
        cleanup_interval_seconds: 300,
        enabled: true,
        exempt_addresses: Vec::new(&env),
    };

    client.initialize(&admin, &config);

    (env, admin, client)
}

fn create_custom_config(
    env: &Env,
    max_tx: u32,
    window_secs: u64,
    block_secs: u64,
    enabled: bool,
) -> ThrottleConfig {
    ThrottleConfig {
        max_transactions_per_window: max_tx,
        window_size_seconds: window_secs,
        block_duration_seconds: block_secs,
        cleanup_interval_seconds: 300,
        enabled,
        exempt_addresses: Vec::new(env),
    }
}

#[test]
fn test_throttle_initialization() {
    let (env, admin, client) = setup_throttle_contract();

    assert_eq!(client.get_admin(), admin);

    let config = client.get_throttle_config();
    assert_eq!(config.max_transactions_per_window, 5);
    assert_eq!(config.window_size_seconds, 60);
    assert_eq!(config.block_duration_seconds, 30);
    assert!(config.enabled);
}

#[test]
#[should_panic]
fn test_double_initialization_fails() {
    let (env, _admin, client) = setup_throttle_contract();

    let another_admin = Address::generate(&env);
    let config = create_custom_config(&env, 3, 60, 30, true);
    client.initialize(&another_admin, &config);
}

#[test]
#[should_panic]
fn test_invalid_config_initialization_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(ThrottleContract, ());
    let client = ThrottleContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let invalid_config = ThrottleConfig {
        max_transactions_per_window: 0, // Invalid
        window_size_seconds: 60,
        block_duration_seconds: 30,
        cleanup_interval_seconds: 300,
        enabled: true,
        exempt_addresses: Vec::new(&env),
    };

    client.initialize(&admin, &invalid_config);
}

#[test]
fn test_transaction_within_limit_allowed() {
    let (env, _admin, client) = setup_throttle_contract();

    let wallet = Address::generate(&env);

    // First transaction should be allowed
    let result = client.check_transaction_throttle(&wallet);
    assert!(result.allowed);
    assert_eq!(result.reason, ThrottleReason::Allowed);
    assert_eq!(result.remaining_transactions, 4);

    // Second transaction should also be allowed
    let result2 = client.check_transaction_throttle(&wallet);
    assert!(result2.allowed);
    assert_eq!(result2.reason, ThrottleReason::Allowed);
    assert_eq!(result2.remaining_transactions, 3);
}

#[test]
fn test_transaction_exceeding_limit_blocked() {
    let (env, _admin, client) = setup_throttle_contract();

    let wallet = Address::generate(&env);

    // Use up all allowed transactions
    for i in 0..5 {
        let result = client.check_transaction_throttle(&wallet);
        assert!(result.allowed, "Transaction {} should be allowed", i + 1);
        assert_eq!(result.remaining_transactions, 4 - i);
    }

    // Next transaction should be blocked
    let blocked_result = client.check_transaction_throttle(&wallet);
    assert!(!blocked_result.allowed);
    assert_eq!(blocked_result.reason, ThrottleReason::ExceededFrequency);
    assert_eq!(blocked_result.remaining_transactions, 0);
    assert!(blocked_result.throttle_end_time.is_some());

    // Check that wallet is in throttled list
    let throttled_wallets = client.get_throttled_wallets();
    assert_eq!(throttled_wallets.len(), 1);
    assert!(throttled_wallets.contains(&wallet));
}

#[test]
fn test_throttle_expires_after_block_duration() {
    let (env, _admin, client) = setup_throttle_contract();

    let wallet = Address::generate(&env);

    // Use up all allowed transactions
    for _ in 0..5 {
        client.check_transaction_throttle(&wallet);
    }

    // Get blocked
    let blocked_result = client.check_transaction_throttle(&wallet);
    assert!(!blocked_result.allowed);

    // Advance time past block duration
    env.ledger().set_timestamp(env.ledger().timestamp() + 31);

    // Should be allowed again
    let allowed_result = client.check_transaction_throttle(&wallet);
    assert!(allowed_result.allowed);
    assert_eq!(allowed_result.reason, ThrottleReason::Allowed);

    // Wallet should no longer be in throttled list
    let throttled_wallets = client.get_throttled_wallets();
    assert_eq!(throttled_wallets.len(), 0);
}

#[test]
fn test_window_resets_after_time_expires() {
    let (env, _admin, client) = setup_throttle_contract();

    let wallet = Address::generate(&env);

    // Use some transactions
    for _ in 0..3 {
        client.check_transaction_throttle(&wallet);
    }

    // Advance time past window duration
    env.ledger().set_timestamp(env.ledger().timestamp() + 61);

    // Should have full allowance again
    let result = client.check_transaction_throttle(&wallet);
    assert!(result.allowed);
    assert_eq!(result.remaining_transactions, 4);
}

#[test]
fn test_exempt_wallet_not_throttled() {
    let (env, admin, client) = setup_throttle_contract();

    let exempt_wallet = Address::generate(&env);

    // Add wallet to exempt list
    client.add_exempt_address(&admin, &exempt_wallet);

    // Should be allowed regardless of frequency
    for i in 0..10 {
        let result = client.check_transaction_throttle(&exempt_wallet);
        assert!(
            result.allowed,
            "Exempt transaction {} should be allowed",
            i + 1
        );
        assert_eq!(result.reason, ThrottleReason::WalletExempt);
    }

    // Should not appear in throttled wallets
    let throttled_wallets = client.get_throttled_wallets();
    assert_eq!(throttled_wallets.len(), 0);
}

#[test]
fn test_disabled_system_allows_all_transactions() {
    let (env, admin, client) = setup_throttle_contract();

    let wallet = Address::generate(&env);

    // Disable throttling
    let disabled_config = create_custom_config(&env, 1, 60, 30, false);
    client.update_throttle_config(&admin, &disabled_config);

    // Should allow all transactions regardless of limits
    for i in 0..10 {
        let result = client.check_transaction_throttle(&wallet);
        assert!(
            result.allowed,
            "Disabled system transaction {} should be allowed",
            i + 1
        );
        assert_eq!(result.reason, ThrottleReason::SystemDisabled);
    }
}

#[test]
fn test_config_update_by_admin() {
    let (env, admin, client) = setup_throttle_contract();

    let new_config = create_custom_config(&env, 10, 120, 60, true);
    client.update_throttle_config(&admin, &new_config);

    let updated_config = client.get_throttle_config();
    assert_eq!(updated_config.max_transactions_per_window, 10);
    assert_eq!(updated_config.window_size_seconds, 120);
    assert_eq!(updated_config.block_duration_seconds, 60);
}

#[test]
#[should_panic]
fn test_config_update_unauthorized_fails() {
    let (env, _admin, client) = setup_throttle_contract();

    let unauthorized = Address::generate(&env);
    let new_config = create_custom_config(&env, 10, 120, 60, true);

    client.update_throttle_config(&unauthorized, &new_config);
}

#[test]
fn test_multiple_wallets_independent_throttling() {
    let (env, _admin, client) = setup_throttle_contract();

    let wallet1 = Address::generate(&env);
    let wallet2 = Address::generate(&env);

    // Use up wallet1's limit
    for _ in 0..5 {
        client.check_transaction_throttle(&wallet1);
    }

    // wallet1 should be throttled
    let result1 = client.check_transaction_throttle(&wallet1);
    assert!(!result1.allowed);

    // wallet2 should still be allowed
    let result2 = client.check_transaction_throttle(&wallet2);
    assert!(result2.allowed);
}

#[test]
fn test_wallet_throttle_info() {
    let (env, _admin, client) = setup_throttle_contract();

    let wallet = Address::generate(&env);

    // Make some transactions
    for _ in 0..3 {
        client.check_transaction_throttle(&wallet);
    }

    let info = client
        .get_wallet_throttle_info(&wallet)
        .expect("Wallet info should exist");
    assert_eq!(info.wallet_address, wallet);
    assert_eq!(info.transaction_count, 3);
    assert_eq!(info.total_transactions_all_time, 3);
    assert!(!info.is_throttled);
    assert_eq!(info.violation_count, 0);
}

#[test]
fn test_global_stats_tracking() {
    let (env, _admin, client) = setup_throttle_contract();

    let wallet1 = Address::generate(&env);
    let wallet2 = Address::generate(&env);

    // Make some transactions
    for _ in 0..3 {
        client.check_transaction_throttle(&wallet1);
    }
    for _ in 0..2 {
        client.check_transaction_throttle(&wallet2);
    }

    // Trigger a violation
    for _ in 0..4 {
        client.check_transaction_throttle(&wallet1);
    }

    let stats = client.get_global_throttle_stats();
    assert_eq!(stats.total_transactions_checked, 9); // 3 + 2 + 4
    assert_eq!(stats.total_violations, 1);
    assert_eq!(stats.currently_throttled_wallets, 1);
}

#[test]
fn test_exempt_address_management() {
    let (env, admin, client) = setup_throttle_contract();

    let wallet = Address::generate(&env);

    // Add to exempt list
    client.add_exempt_address(&admin, &wallet);

    let config = client.get_throttle_config();
    assert!(config.exempt_addresses.contains(&wallet));

    // Remove from exempt list
    client.remove_exempt_address(&admin, &wallet);

    let updated_config = client.get_throttle_config();
    assert!(!updated_config.exempt_addresses.contains(&wallet));
}

#[test]
#[should_panic]
fn test_exempt_address_unauthorized_fails() {
    let (env, _admin, client) = setup_throttle_contract();

    let wallet = Address::generate(&env);
    let unauthorized = Address::generate(&env);

    client.add_exempt_address(&unauthorized, &wallet);
}

#[test]
fn test_force_cleanup_admin_only() {
    let (env, admin, client) = setup_throttle_contract();

    // Admin can force cleanup
    client.force_cleanup(&admin);

    // Check events
    let events = env.events().all();
    let cleanup_events = events
        .iter()
        .filter(|event| {
            event.1.iter().any(|topic| {
                symbol_short!("cleanup")
                    == soroban_sdk::Symbol::try_from_val(&env, &topic).unwrap_or(symbol_short!(""))
            })
        })
        .count();
    assert!(cleanup_events >= 1);
}

#[test]
#[should_panic]
fn test_force_cleanup_unauthorized_fails() {
    let (env, _admin, client) = setup_throttle_contract();

    let unauthorized = Address::generate(&env);
    client.force_cleanup(&unauthorized);
}

#[test]
fn test_reset_wallet_throttle_state() {
    let (env, admin, client) = setup_throttle_contract();

    let wallet = Address::generate(&env);

    // Use up transactions and trigger violation
    for _ in 0..6 {
        client.check_transaction_throttle(&wallet);
    }

    // Reset by admin
    client.reset_wallet_throttle_state(&admin, &wallet);

    // Should be allowed again
    let result = client.check_transaction_throttle(&wallet);
    assert!(result.allowed);
    assert_eq!(result.remaining_transactions, 4);

    // Should not be in throttled list
    let throttled_wallets = client.get_throttled_wallets();
    assert_eq!(throttled_wallets.len(), 0);
}

#[test]
#[should_panic]
fn test_reset_wallet_throttle_state_unauthorized_fails() {
    let (env, _admin, client) = setup_throttle_contract();

    let wallet = Address::generate(&env);
    let unauthorized = Address::generate(&env);

    client.reset_wallet_throttle_state(&unauthorized, &wallet);
}

#[test]
fn test_throttle_events_emitted() {
    let (env, _admin, client) = setup_throttle_contract();

    let wallet = Address::generate(&env);

    // Make allowed transaction
    client.check_transaction_throttle(&wallet);

    // Check for allowed event
    let events = env.events().all();
    let allowed_events = events
        .iter()
        .filter(|event| {
            event.1.iter().any(|topic| {
                symbol_short!("allowed")
                    == soroban_sdk::Symbol::try_from_val(&env, &topic).unwrap_or(symbol_short!(""))
            })
        })
        .count();
    assert_eq!(allowed_events, 1);
}

#[test]
fn test_throttle_triggered_events() {
    let (env, _admin, client) = setup_throttle_contract();

    let wallet = Address::generate(&env);

    // Trigger throttling
    for _ in 0..6 {
        client.check_transaction_throttle(&wallet);
    }

    // Check for throttle triggered event
    let events = env.events().all();
    let triggered_events = events
        .iter()
        .filter(|event| {
            event.1.iter().any(|topic| {
                symbol_short!("triggered")
                    == soroban_sdk::Symbol::try_from_val(&env, &topic).unwrap_or(symbol_short!(""))
            })
        })
        .count();
    assert_eq!(triggered_events, 1);
}

#[test]
fn test_throttle_lifted_events() {
    let (env, _admin, client) = setup_throttle_contract();

    let wallet = Address::generate(&env);

    // Trigger throttling
    for _ in 0..6 {
        client.check_transaction_throttle(&wallet);
    }

    // Advance time past block duration
    env.ledger().set_timestamp(env.ledger().timestamp() + 31);

    // Make a transaction to trigger throttle lift
    client.check_transaction_throttle(&wallet);

    // Check for throttle lifted event
    let events = env.events().all();
    let lifted_events = events
        .iter()
        .filter(|event| {
            event.1.iter().any(|topic| {
                symbol_short!("lifted")
                    == soroban_sdk::Symbol::try_from_val(&env, &topic).unwrap_or(symbol_short!(""))
            })
        })
        .count();
    assert_eq!(lifted_events, 1);
}

#[test]
fn test_edge_case_zero_window_size_config() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(ThrottleContract, ());
    let client = ThrottleContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let invalid_config = ThrottleConfig {
        max_transactions_per_window: 5,
        window_size_seconds: 0, // Invalid
        block_duration_seconds: 30,
        cleanup_interval_seconds: 300,
        enabled: true,
        exempt_addresses: Vec::new(&env),
    };

    // Should panic during initialization
    client.initialize(&admin, &invalid_config);
}

#[test]
fn test_edge_case_zero_block_duration_config() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(ThrottleContract, ());
    let client = ThrottleContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let invalid_config = ThrottleConfig {
        max_transactions_per_window: 5,
        window_size_seconds: 60,
        block_duration_seconds: 0, // Invalid
        cleanup_interval_seconds: 300,
        enabled: true,
        exempt_addresses: Vec::new(&env),
    };

    // Should panic during initialization
    client.initialize(&admin, &invalid_config);
}

#[test]
fn test_edge_case_max_transactions_zero_config() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(ThrottleContract, ());
    let client = ThrottleContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let invalid_config = ThrottleConfig {
        max_transactions_per_window: 0, // Invalid
        window_size_seconds: 60,
        block_duration_seconds: 30,
        cleanup_interval_seconds: 300,
        enabled: true,
        exempt_addresses: Vec::new(&env),
    };

    // Should panic during initialization
    client.initialize(&admin, &invalid_config);
}

#[test]
fn test_edge_case_wallet_with_no_transactions() {
    let (env, _admin, client) = setup_throttle_contract();

    let wallet = Address::generate(&env);

    // Should have no throttle info initially
    let info = client.get_wallet_throttle_info(&wallet);
    assert!(info.is_some());

    let state = info.unwrap();
    assert_eq!(state.transaction_count, 0);
    assert_eq!(state.total_transactions_all_time, 0);
    assert!(!state.is_throttled);
}

#[test]
fn test_edge_case_very_short_window() {
    let (env, admin, client) = setup_throttle_contract();

    let wallet = Address::generate(&env);

    // Set very short window (1 second)
    let short_config = create_custom_config(&env, 2, 1, 5, true);
    client.update_throttle_config(&admin, &short_config);

    // Make transactions within the short window
    let result1 = client.check_transaction_throttle(&wallet);
    assert!(result1.allowed);

    let result2 = client.check_transaction_throttle(&wallet);
    assert!(result2.allowed);

    // Third should be blocked
    let result3 = client.check_transaction_throttle(&wallet);
    assert!(!result3.allowed);

    // Wait past window
    env.ledger().set_timestamp(env.ledger().timestamp() + 2);

    // Should be allowed again
    let result4 = client.check_transaction_throttle(&wallet);
    assert!(result4.allowed);
}

#[test]
fn test_edge_case_very_long_block_duration() {
    let (env, admin, client) = setup_throttle_contract();

    let wallet = Address::generate(&env);

    // Set very long block duration
    let long_config = create_custom_config(&env, 2, 60, 3600, true); // 1 hour block
    client.update_throttle_config(&admin, &long_config);

    // Trigger throttling
    for _ in 0..3 {
        client.check_transaction_throttle(&wallet);
    }

    let blocked_result = client.check_transaction_throttle(&wallet);
    assert!(!blocked_result.allowed);

    // Should still be blocked after normal time
    env.ledger().set_timestamp(env.ledger().timestamp() + 120);

    let still_blocked_result = client.check_transaction_throttle(&wallet);
    assert!(!still_blocked_result.allowed);
}

#[test]
fn test_edge_case_multiple_violations() {
    let (env, _admin, client) = setup_throttle_contract();

    let wallet = Address::generate(&env);

    // Trigger first violation
    for _ in 0..6 {
        client.check_transaction_throttle(&wallet);
    }

    // Wait for throttle to lift
    env.ledger().set_timestamp(env.ledger().timestamp() + 31);

    // Trigger second violation
    for _ in 0..6 {
        client.check_transaction_throttle(&wallet);
    }

    let info = client
        .get_wallet_throttle_info(&wallet)
        .expect("Wallet info should exist");
    assert_eq!(info.violation_count, 2);
}

#[test]
fn test_edge_case_concurrent_wallets() {
    let (env, _admin, client) = setup_throttle_contract();

    let mut wallets: Vec<Address> = Vec::new(&env);
    for _ in 0..10 {
        wallets.push_back(Address::generate(&env));
    }

    // Each wallet makes transactions
    for wallet in wallets.iter() {
        for _ in 0..3 {
            client.check_transaction_throttle(&wallet);
        }
    }

    // All should still be allowed
    for wallet in wallets.iter() {
        let result = client.check_transaction_throttle(&wallet);
        assert!(result.allowed);
        assert_eq!(result.remaining_transactions, 2);
    }

    // Global stats should reflect all transactions
    let stats = client.get_global_throttle_stats();
    assert_eq!(stats.total_transactions_checked, 40); // 10 wallets * 4 transactions each
}

#[test]
fn test_edge_case_config_update_with_active_throttles() {
    let (env, admin, client) = setup_throttle_contract();

    let wallet = Address::generate(&env);

    // Trigger throttling with original config
    for _ in 0..6 {
        client.check_transaction_throttle(&wallet);
    }

    // Update config to more lenient settings
    let lenient_config = create_custom_config(&env, 10, 60, 15, true);
    client.update_throttle_config(&admin, &lenient_config);

    // Should still be throttled until block expires
    let blocked_result = client.check_transaction_throttle(&wallet);
    assert!(!blocked_result.allowed);

    // Wait for block to expire
    env.ledger().set_timestamp(env.ledger().timestamp() + 16);

    // Should be allowed with new config
    let allowed_result = client.check_transaction_throttle(&wallet);
    assert!(allowed_result.allowed);
    assert_eq!(allowed_result.remaining_transactions, 9); // New limit
}

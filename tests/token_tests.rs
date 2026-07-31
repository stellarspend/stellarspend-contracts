use std::panic::AssertUnwindSafe;

use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _},
    Address, Env, String, Symbol, TryFromVal,
};

fn event_topics_contain_symbol(
    env: &Env,
    topics: &soroban_sdk::Vec<soroban_sdk::Val>,
    sym: soroban_sdk::Symbol,
) -> bool {
    topics
        .iter()
        .any(|topic| sym == Symbol::try_from_val(env, &topic).unwrap_or(symbol_short!("")))
}

#[path = "../contracts/token.rs"]
mod token;

use token::{
    BurnRecord, MintRecord, TokenConfig, TokenContract, TokenContractClient, TokenError,
    TokenMetrics,
};

fn setup_token_contract() -> (Env, Address, Address, TokenContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(TokenContract, ());
    let client = TokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let name = String::from_str(&env, "StellarSpend Token");
    let symbol = String::from_str(&env, "SPEND");
    let decimals = 18u32;
    let mint_cap = Some(1000000i128);
    let burn_cap = Some(500000i128);

    client.initialize(&admin, &name, &symbol, &decimals, &mint_cap, &burn_cap);

    (env, admin, contract_id, client)
}

fn setup_token_contract_no_caps() -> (Env, Address, Address, TokenContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(TokenContract, ());
    let client = TokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let name = String::from_str(&env, "StellarSpend Token");
    let symbol = String::from_str(&env, "SPEND");
    let decimals = 18u32;

    client.initialize(&admin, &name, &symbol, &decimals, &None, &None);

    (env, admin, contract_id, client)
}

#[test]
fn test_token_initialization() {
    let (env, admin, _token_contract, client) = setup_token_contract();

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.total_supply(), 0);
    assert_eq!(client.total_minted(), 0);
    assert_eq!(client.total_burned(), 0);
    assert!(!client.is_paused());
    assert_eq!(client.mint_cap(), Some(1000000i128));
    assert_eq!(client.burn_cap(), Some(500000i128));
    assert!(client.is_minter(&admin));
}

#[test]
#[should_panic]
fn test_double_initialization_fails() {
    let (env, _admin, _token_contract, client) = setup_token_contract();

    let another_admin = Address::generate(&env);
    let name = String::from_str(&env, "Another Token");
    let symbol = String::from_str(&env, "OTHER");
    client.initialize(&another_admin, &name, &symbol, &18u32, &None, &None);
}

#[test]
#[should_panic]
fn test_invalid_initialization_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(TokenContract, ());
    let client = TokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let name = String::from_str(&env, ""); // Empty name
    let symbol = String::from_str(&env, "TEST");

    client.initialize(&admin, &name, &symbol, &18u32, &None, &None);
}

#[test]
fn test_admin_mint_success() {
    let (env, admin, _token_contract, client) = setup_token_contract();

    let recipient = Address::generate(&env);
    let amount = 1000i128;

    let _transaction_id = client.mint(&admin, &recipient, &amount);

    // Assert on events before further `client.*` calls (SDK 22 clears the event buffer per invocation).
    let events = env.events().all();
    let mint_events = events
        .iter()
        .filter(|event| {
            event.1.iter().any(|topic| {
                symbol_short!("mint")
                    == Symbol::try_from_val(&env, &topic).unwrap_or(symbol_short!(""))
            })
        })
        .count();
    assert_eq!(mint_events, 1);

    assert_eq!(client.balance(&recipient), amount);
    assert_eq!(client.total_supply(), amount);
    assert_eq!(client.total_minted(), amount);
}

#[test]
#[should_panic]
fn test_unauthorized_mint_fails() {
    let (env, _admin, _token_contract, client) = setup_token_contract();

    let unauthorized = Address::generate(&env);
    let recipient = Address::generate(&env);
    let amount = 1000i128;

    client.mint(&unauthorized, &recipient, &amount);
}

#[test]
#[should_panic]
fn test_mint_invalid_amount_fails() {
    let (env, admin, _token_contract, client) = setup_token_contract();

    let recipient = Address::generate(&env);
    let amount = 0i128; // Invalid amount

    client.mint(&admin, &recipient, &amount);
}

#[test]
#[should_panic]
fn test_mint_negative_amount_fails() {
    let (env, admin, _token_contract, client) = setup_token_contract();

    let recipient = Address::generate(&env);
    let amount = -100i128; // Negative amount

    client.mint(&admin, &recipient, &amount);
}

#[test]
#[should_panic]
fn test_mint_to_zero_address_fails() {
    let (env, admin, token_contract, client) = setup_token_contract();

    let amount = 1000i128;

    client.mint(&admin, &token_contract, &amount);
}

#[test]
#[should_panic]
fn test_mint_cap_exceeded_fails() {
    let (env, admin, _token_contract, client) = setup_token_contract();

    let recipient = Address::generate(&env);
    let amount = 2000000i128; // Exceeds cap of 1000000

    client.mint(&admin, &recipient, &amount);
}

#[test]
fn test_mint_cap_respected() {
    let (env, admin, _token_contract, client) = setup_token_contract();

    let recipient = Address::generate(&env);

    // Mint up to cap
    client.mint(&admin, &recipient, &500000i128);
    client.mint(&admin, &recipient, &500000i128);

    assert_eq!(client.total_supply(), 1000000i128);
    assert_eq!(client.total_minted(), 1000000i128);

    // Next mint should fail
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        client.mint(&admin, &recipient, &1i128);
    }));
    assert!(result.is_err());
}

#[test]
fn test_burn_success() {
    let (env, admin, _token_contract, client) = setup_token_contract();

    let user = Address::generate(&env);
    let mint_amount = 2000i128;
    let burn_amount = 500i128;

    // First mint some tokens
    client.mint(&admin, &user, &mint_amount);
    assert_eq!(client.balance(&user), mint_amount);

    // Then burn some
    client.burn(&user, &burn_amount);

    let events = env.events().all();
    let burn_events = events
        .iter()
        .filter(|event| {
            event.1.iter().any(|topic| {
                symbol_short!("burn")
                    == Symbol::try_from_val(&env, &topic).unwrap_or(symbol_short!(""))
            })
        })
        .count();
    assert_eq!(burn_events, 1);

    assert_eq!(client.balance(&user), mint_amount - burn_amount);
    assert_eq!(client.total_supply(), mint_amount - burn_amount);
    assert_eq!(client.total_burned(), burn_amount);
}

#[test]
#[should_panic]
fn test_burn_insufficient_balance_fails() {
    let (env, admin, _token_contract, client) = setup_token_contract();

    let user = Address::generate(&env);
    let mint_amount = 1000i128;
    let burn_amount = 1500i128; // More than balance

    client.mint(&admin, &user, &mint_amount);
    client.burn(&user, &burn_amount);
}

#[test]
#[should_panic]
fn test_burn_invalid_amount_fails() {
    let (env, admin, _token_contract, client) = setup_token_contract();

    let user = Address::generate(&env);
    let mint_amount = 1000i128;

    client.mint(&admin, &user, &mint_amount);

    // Try to burn 0
    client.burn(&user, &0i128);
}

#[test]
#[should_panic]
fn test_burn_negative_amount_fails() {
    let (env, admin, _token_contract, client) = setup_token_contract();

    let user = Address::generate(&env);
    let mint_amount = 1000i128;

    client.mint(&admin, &user, &mint_amount);

    // Try to burn negative amount
    client.burn(&user, &-100i128);
}

#[test]
#[should_panic]
fn test_burn_cap_exceeded_fails() {
    let (env, admin, _token_contract, client) = setup_token_contract();

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    // Mint tokens to multiple users
    client.mint(&admin, &user1, &300000i128);
    client.mint(&admin, &user2, &300000i128);

    // Burn up to cap
    client.burn(&user1, &200000i128);
    client.burn(&user2, &300000i128);

    assert_eq!(client.total_burned(), 500000i128);

    // Next burn should fail
    client.burn(&user1, &1i128);
}

#[test]
fn test_transfer_success() {
    let (env, admin, _token_contract, client) = setup_token_contract();

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let amount = 1000i128;

    // Mint to user1
    client.mint(&admin, &user1, &amount);
    assert_eq!(client.balance(&user1), amount);
    assert_eq!(client.balance(&user2), 0);

    // Transfer from user1 to user2
    client.transfer(&user1, &user2, &amount);

    let events = env.events().all();
    let transfer_events = events
        .iter()
        .filter(|event| {
            event.1.iter().any(|topic| {
                symbol_short!("transfer")
                    == Symbol::try_from_val(&env, &topic).unwrap_or(symbol_short!(""))
            })
        })
        .count();
    assert_eq!(transfer_events, 1);

    assert_eq!(client.balance(&user1), 0);
    assert_eq!(client.balance(&user2), amount);
}

#[test]
#[should_panic]
fn test_transfer_insufficient_balance_fails() {
    let (env, admin, _token_contract, client) = setup_token_contract();

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let mint_amount = 1000i128;
    let transfer_amount = 1500i128;

    client.mint(&admin, &user1, &mint_amount);
    client.transfer(&user1, &user2, &transfer_amount);
}

#[test]
#[should_panic]
fn test_transfer_invalid_amount_fails() {
    let (env, admin, _token_contract, client) = setup_token_contract();

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let mint_amount = 1000i128;

    client.mint(&admin, &user1, &mint_amount);
    client.transfer(&user1, &user2, &0i128);
}

#[test]
#[should_panic]
fn test_transfer_to_zero_address_fails() {
    let (env, admin, token_contract, client) = setup_token_contract();

    let user1 = Address::generate(&env);
    let amount = 1000i128;

    client.mint(&admin, &user1, &amount);
    client.transfer(&user1, &token_contract, &amount);
}

#[test]
fn test_approve_success() {
    let (env, admin, _token_contract, client) = setup_token_contract();

    let owner = Address::generate(&env);
    let spender = Address::generate(&env);
    let amount = 1000i128;

    // Mint tokens to owner
    client.mint(&admin, &owner, &amount);

    // Approve spender
    client.approve(&owner, &spender, &amount);

    let events = env.events().all();
    let approval_events = events
        .iter()
        .filter(|event| {
            event.1.iter().any(|topic| {
                symbol_short!("approval")
                    == Symbol::try_from_val(&env, &topic).unwrap_or(symbol_short!(""))
            })
        })
        .count();
    assert_eq!(approval_events, 1);

    assert_eq!(client.allowance(&owner, &spender), amount);
}

#[test]
fn test_transfer_from_success() {
    let (env, admin, _token_contract, client) = setup_token_contract();

    let owner = Address::generate(&env);
    let spender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let amount = 1000i128;

    // Mint tokens to owner
    client.mint(&admin, &owner, &amount);

    // Approve spender
    client.approve(&owner, &spender, &amount);

    // Transfer using allowance
    client.transfer_from(&spender, &owner, &recipient, &amount);

    assert_eq!(client.balance(&owner), 0);
    assert_eq!(client.balance(&recipient), amount);
    assert_eq!(client.allowance(&owner, &spender), 0);
}

#[test]
#[should_panic]
fn test_transfer_from_insufficient_allowance_fails() {
    let (env, admin, _token_contract, client) = setup_token_contract();

    let owner = Address::generate(&env);
    let spender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let mint_amount = 1000i128;
    let allowance_amount = 500i128;
    let transfer_amount = 800i128;

    // Mint tokens to owner
    client.mint(&admin, &owner, &mint_amount);

    // Approve spender with insufficient amount
    client.approve(&owner, &spender, &allowance_amount);

    // Try to transfer more than allowed
    client.transfer_from(&spender, &owner, &recipient, &transfer_amount);
}

#[test]
fn test_minter_management() {
    let (env, admin, _token_contract, client) = setup_token_contract();

    let minter = Address::generate(&env);
    let recipient = Address::generate(&env);

    // Admin should be minter by default
    assert!(client.is_minter(&admin));
    assert!(!client.is_minter(&minter));

    // Add minter
    client.add_minter(&admin, &minter);
    assert!(client.is_minter(&minter));

    // New minter should be able to mint
    client.mint(&minter, &recipient, &1000i128);
    assert_eq!(client.balance(&recipient), 1000i128);

    // Remove minter
    client.remove_minter(&admin, &minter);
    assert!(!client.is_minter(&minter));

    // Should no longer be able to mint
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        client.mint(&minter, &recipient, &1000i128);
    }));
    assert!(result.is_err());
}

#[test]
#[should_panic]
fn test_unauthorized_minter_management_fails() {
    let (env, _admin, _token_contract, client) = setup_token_contract();

    let unauthorized = Address::generate(&env);
    let minter = Address::generate(&env);

    client.add_minter(&unauthorized, &minter);
}

#[test]
#[should_panic]
fn test_remove_admin_as_minter_fails() {
    let (env, admin, _token_contract, client) = setup_token_contract();

    client.remove_minter(&admin, &admin);
}

#[test]
fn test_pause_unpause_functionality() {
    let (env, admin, _token_contract, client) = setup_token_contract();

    let user = Address::generate(&env);
    let amount = 1000i128;

    // Mint tokens before pause
    client.mint(&admin, &user, &amount);
    assert!(!client.is_paused());

    // Pause contract
    client.pause(&admin);
    assert!(client.is_paused());

    // Operations should fail when paused
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        client.mint(&admin, &user, &amount);
    }));
    assert!(result.is_err());

    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        client.transfer(&user, &user, &amount);
    }));
    assert!(result.is_err());

    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        client.burn(&user, &amount);
    }));
    assert!(result.is_err());

    // Unpause contract
    client.unpause(&admin);
    assert!(!client.is_paused());

    // Operations should work again
    client.mint(&admin, &user, &amount);
    client.transfer(&user, &user, &amount);
    client.burn(&user, &amount);
}

#[test]
#[should_panic]
fn test_unauthorized_pause_fails() {
    let (env, _admin, _token_contract, client) = setup_token_contract();

    let unauthorized = Address::generate(&env);
    client.pause(&unauthorized);
}

#[test]
#[should_panic]
fn test_unauthorized_unpause_fails() {
    let (env, admin, _token_contract, client) = setup_token_contract();

    let unauthorized = Address::generate(&env);

    // First pause with admin
    client.pause(&admin);

    // Try to unpause with unauthorized
    client.unpause(&unauthorized);
}

#[test]
fn test_token_metrics() {
    let (env, admin, _token_contract, client) = setup_token_contract();

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    // Mint some tokens
    client.mint(&admin, &user1, &1000i128);
    client.mint(&admin, &user2, &2000i128);

    // Burn some tokens
    client.burn(&user1, &300i128);

    let metrics = client.token_metrics();
    assert_eq!(metrics.total_supply, 2700i128);
    assert_eq!(metrics.total_minted, 3000i128);
    assert_eq!(metrics.total_burned, 300i128);
}

#[test]
fn test_no_caps_token() {
    let (env, admin, _token_contract, client) = setup_token_contract_no_caps();

    let recipient = Address::generate(&env);
    let amount = 1000000i128;

    // Should be able to mint without caps
    client.mint(&admin, &recipient, &amount);
    assert_eq!(client.balance(&recipient), amount);
    assert_eq!(client.total_supply(), amount);

    // Should be able to mint more
    client.mint(&admin, &recipient, &amount);
    assert_eq!(client.balance(&recipient), amount * 2);
    assert_eq!(client.total_supply(), amount * 2);

    assert_eq!(client.mint_cap(), None);
    assert_eq!(client.burn_cap(), None);
}

#[test]
fn test_overflow_protection_mint() {
    let (env, admin, _token_contract, client) = setup_token_contract_no_caps();

    let recipient = Address::generate(&env);

    // Mint a large amount
    client.mint(&admin, &recipient, &i128::MAX);

    // Trying to mint more should cause overflow
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        client.mint(&admin, &recipient, &1i128);
    }));
    assert!(result.is_err());
}

#[test]
fn test_underflow_protection_burn() {
    let (env, admin, _token_contract, client) = setup_token_contract();

    let user = Address::generate(&env);
    let amount = 1000i128;

    client.mint(&admin, &user, &amount);

    // Burn exact amount
    client.burn(&user, &amount);
    assert_eq!(client.balance(&user), 0);

    // Trying to burn more should cause underflow
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        client.burn(&user, &1i128);
    }));
    assert!(result.is_err());
}

#[test]
fn test_zero_balance_cleanup() {
    let (env, admin, _token_contract, client) = setup_token_contract();

    let user = Address::generate(&env);
    let amount = 1000i128;

    client.mint(&admin, &user, &amount);
    assert_eq!(client.balance(&user), amount);

    // Transfer all tokens away
    let recipient = Address::generate(&env);
    client.transfer(&user, &recipient, &amount);

    assert_eq!(client.balance(&user), 0);
    assert_eq!(client.balance(&recipient), amount);

    // Burn all tokens
    client.burn(&recipient, &amount);
    assert_eq!(client.balance(&recipient), 0);
}

#[test]
fn test_multiple_minters() {
    let (env, admin, _token_contract, client) = setup_token_contract();

    let minter1 = Address::generate(&env);
    let minter2 = Address::generate(&env);
    let recipient1 = Address::generate(&env);
    let recipient2 = Address::generate(&env);

    // Add multiple minters
    client.add_minter(&admin, &minter1);
    client.add_minter(&admin, &minter2);

    assert!(client.is_minter(&minter1));
    assert!(client.is_minter(&minter2));

    // Both should be able to mint
    client.mint(&minter1, &recipient1, &1000i128);
    client.mint(&minter2, &recipient2, &2000i128);

    assert_eq!(client.balance(&recipient1), 1000i128);
    assert_eq!(client.balance(&recipient2), 2000i128);
    assert_eq!(client.total_supply(), 3000i128);
}

#[test]
fn test_complex_scenario() {
    let (env, admin, _token_contract, client) = setup_token_contract();

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let user3 = Address::generate(&env);

    // Add user2 as minter
    client.add_minter(&admin, &user2);

    // Mint tokens to users
    client.mint(&admin, &user1, &5000i128);
    client.mint(&user2, &user2, &3000i128);
    client.mint(&admin, &user3, &2000i128);

    // Setup allowance
    client.approve(&user1, &user3, &2000i128);

    // Transfer using allowance
    client.transfer_from(&user3, &user1, &user3, &1500i128);

    // Burn some tokens
    client.burn(&user2, &1000i128);
    client.burn(&user3, &500i128);

    // Check final state
    assert_eq!(client.balance(&user1), 3500i128);
    assert_eq!(client.balance(&user2), 2000i128);
    assert_eq!(client.balance(&user3), 3000i128);
    assert_eq!(client.total_supply(), 8500i128);
    assert_eq!(client.total_minted(), 10000i128);
    assert_eq!(client.total_burned(), 1500i128);
    assert_eq!(client.allowance(&user1, &user3), 500i128);
}

#[test]
fn test_edge_case_maximum_values() {
    let (env, admin, _token_contract, client) = setup_token_contract_no_caps();

    let recipient = Address::generate(&env);
    let max_amount = i128::MAX;

    // Test with maximum valid amount
    client.mint(&admin, &recipient, &max_amount);
    assert_eq!(client.balance(&recipient), max_amount);

    // Burn maximum amount
    client.burn(&recipient, &max_amount);
    assert_eq!(client.balance(&recipient), 0);
}

#[test]
fn test_edge_case_minimum_values() {
    let (env, admin, _token_contract, client) = setup_token_contract();

    let recipient = Address::generate(&env);
    let min_amount = 1i128;

    // Test with minimum valid amount
    client.mint(&admin, &recipient, &min_amount);
    assert_eq!(client.balance(&recipient), min_amount);

    // Burn minimum amount
    client.burn(&recipient, &min_amount);
    assert_eq!(client.balance(&recipient), 0);
}

#[test]
fn test_event_emission_comprehensive() {
    let (env, admin, _token_contract, client) = setup_token_contract();

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let minter = Address::generate(&env);

    client.add_minter(&admin, &minter);
    assert!(env
        .events()
        .all()
        .iter()
        .any(|ev| { event_topics_contain_symbol(&env, &ev.1, symbol_short!("minter")) }));

    client.mint(&minter, &user1, &1000i128);
    assert!(env
        .events()
        .all()
        .iter()
        .any(|ev| { event_topics_contain_symbol(&env, &ev.1, symbol_short!("mint")) }));

    client.transfer(&user1, &user2, &500i128);
    assert!(env
        .events()
        .all()
        .iter()
        .any(|ev| { event_topics_contain_symbol(&env, &ev.1, symbol_short!("transfer")) }));

    client.approve(&user2, &user1, &200i128);
    assert!(env
        .events()
        .all()
        .iter()
        .any(|ev| { event_topics_contain_symbol(&env, &ev.1, symbol_short!("approval")) }));

    client.burn(&user2, &100i128);
    assert!(env
        .events()
        .all()
        .iter()
        .any(|ev| { event_topics_contain_symbol(&env, &ev.1, symbol_short!("burn")) }));
}

#[test]
fn test_get_token_balance() {
    let (_env, admin, _token_contract, client) = setup_token_contract();

    let user = Address::generate(&_env);
    let amount = 1000i128;

    // Test balance for address with no tokens
    assert_eq!(client.get_token_balance(&user), 0);

    // Mint tokens to user
    client.mint(&admin, &user, &amount);

    // Test balance for address with tokens
    assert_eq!(client.get_token_balance(&user), amount);

    // Burn some tokens
    client.burn(&user, &500i128);

    // Test balance after burn
    assert_eq!(client.get_token_balance(&user), 500);
}

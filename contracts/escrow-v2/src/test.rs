//! Integration tests for multi-party escrow v2 state machine.

#![cfg(test)]

use crate::{EscrowState, EscrowV2Contract, EscrowV2ContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

const WINDOW_SECS: u64 = 1_000;

fn setup() -> (
    Env,
    Address,
    token::Client<'static>,
    token::StellarAssetClient<'static>,
    EscrowV2ContractClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| {
        li.timestamp = 10_000;
    });

    let issuer = Address::generate(&env);
    let stellar_asset = env.register_stellar_asset_contract_v2(issuer);
    let token_id: Address = stellar_asset.address();
    let token_client = token::Client::new(&env, &token_id);
    let token_admin = token::StellarAssetClient::new(&env, &token_id);

    let contract_id = env.register(EscrowV2Contract, ());
    let client = EscrowV2ContractClient::new(&env, &contract_id);

    (env, token_id, token_client, token_admin, client)
}

fn fund(
    client: &EscrowV2ContractClient,
    token_admin: &token::StellarAssetClient,
    token: &Address,
    buyer: &Address,
    seller: &Address,
    arbitrator: Option<&Address>,
    amount: i128,
) -> u64 {
    token_admin.mint(buyer, &amount);
    let arb = arbitrator.cloned();
    client.fund_escrow(buyer, seller, &arb, token, &amount, &WINDOW_SECS)
}

fn advance_past_window(env: &Env) {
    let now = env.ledger().timestamp();
    env.ledger().with_mut(|li| {
        li.timestamp = now + WINDOW_SECS + 1;
    });
}

// ============================================
// Funded → Released (buyer release)
// ============================================

#[test]
fn test_fund_and_release() {
    let (env, token, token_client, token_admin, client) = setup();
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let amount = 10_000_000i128;

    let id = fund(&client, &token_admin, &token, &buyer, &seller, None, amount);

    let escrow = client.get_escrow(&id).unwrap();
    assert_eq!(escrow.state, EscrowState::Funded);
    assert_eq!(escrow.amount, amount);
    assert_eq!(escrow.buyer, buyer);
    assert_eq!(escrow.seller, seller);
    assert_eq!(escrow.auto_release_at, 10_000 + WINDOW_SECS);
    assert_eq!(token_client.balance(&client.address), amount);

    client.release(&buyer, &id);

    let escrow = client.get_escrow(&id).unwrap();
    assert_eq!(escrow.state, EscrowState::Released);
    assert_eq!(escrow.seller_payout, amount);
    assert_eq!(escrow.buyer_payout, 0);
    assert_eq!(token_client.balance(&seller), amount);
    assert_eq!(token_client.balance(&client.address), 0);
}

#[test]
fn test_release_unauthorized_seller() {
    let (env, token, _token_client, token_admin, client) = setup();
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);

    let id = fund(&client, &token_admin, &token, &buyer, &seller, None, 1_000);

    let res = client.try_release(&seller, &id);
    assert!(res.is_err());
    assert_eq!(client.get_escrow(&id).unwrap().state, EscrowState::Funded);
}

#[test]
fn test_release_after_dispute_fails() {
    let (env, token, _token_client, token_admin, client) = setup();
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arb = Address::generate(&env);

    let id = fund(
        &client,
        &token_admin,
        &token,
        &buyer,
        &seller,
        Some(&arb),
        1_000,
    );
    client.raise_dispute(&buyer, &id);

    let res = client.try_release(&buyer, &id);
    assert!(res.is_err());
    assert_eq!(client.get_escrow(&id).unwrap().state, EscrowState::Disputed);
}

// ============================================
// Funded → Disputed → Resolved
// ============================================

#[test]
fn test_raise_dispute_by_buyer_and_seller() {
    let (env, token, _token_client, token_admin, client) = setup();
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arb = Address::generate(&env);

    let id = fund(
        &client,
        &token_admin,
        &token,
        &buyer,
        &seller,
        Some(&arb),
        5_000,
    );
    client.raise_dispute(&seller, &id);

    let escrow = client.get_escrow(&id).unwrap();
    assert_eq!(escrow.state, EscrowState::Disputed);
    assert_eq!(escrow.disputed_by, Some(seller));
}

#[test]
fn test_resolve_dispute_split_60_40() {
    let (env, token, token_client, token_admin, client) = setup();
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arb = Address::generate(&env);
    let amount = 10_000i128;

    let id = fund(
        &client,
        &token_admin,
        &token,
        &buyer,
        &seller,
        Some(&arb),
        amount,
    );
    client.raise_dispute(&buyer, &id);
    client.resolve_dispute(&arb, &id, &6000u32, &4000u32);

    let escrow = client.get_escrow(&id).unwrap();
    assert_eq!(escrow.state, EscrowState::Resolved);
    assert_eq!(escrow.buyer_payout, 6_000);
    assert_eq!(escrow.seller_payout, 4_000);
    assert_eq!(token_client.balance(&buyer), 6_000);
    assert_eq!(token_client.balance(&seller), 4_000);
    assert_eq!(token_client.balance(&client.address), 0);
    // Arbitrator must not receive funds
    assert_eq!(token_client.balance(&arb), 0);
}

#[test]
fn test_resolve_dispute_full_to_buyer_or_seller() {
    let (env, token, token_client, token_admin, client) = setup();
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arb = Address::generate(&env);

    let id = fund(
        &client,
        &token_admin,
        &token,
        &buyer,
        &seller,
        Some(&arb),
        8_000,
    );
    client.raise_dispute(&seller, &id);
    client.resolve_dispute(&arb, &id, &10000u32, &0u32);

    assert_eq!(token_client.balance(&buyer), 8_000);
    assert_eq!(token_client.balance(&seller), 0);
    assert_eq!(client.get_escrow(&id).unwrap().state, EscrowState::Resolved);
}

#[test]
fn test_resolve_invalid_bps_rejected() {
    let (env, token, _token_client, token_admin, client) = setup();
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arb = Address::generate(&env);

    let id = fund(
        &client,
        &token_admin,
        &token,
        &buyer,
        &seller,
        Some(&arb),
        1_000,
    );
    client.raise_dispute(&buyer, &id);

    let res = client.try_resolve_dispute(&arb, &id, &3000u32, &6000u32);
    assert!(res.is_err());
    assert_eq!(client.get_escrow(&id).unwrap().state, EscrowState::Disputed);
}

#[test]
fn test_resolve_before_dispute_fails() {
    let (env, token, _token_client, token_admin, client) = setup();
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arb = Address::generate(&env);

    let id = fund(
        &client,
        &token_admin,
        &token,
        &buyer,
        &seller,
        Some(&arb),
        1_000,
    );

    let res = client.try_resolve_dispute(&arb, &id, &5000u32, &5000u32);
    assert!(res.is_err());
    assert_eq!(client.get_escrow(&id).unwrap().state, EscrowState::Funded);
}

#[test]
fn test_resolve_unauthorized_non_arbitrator() {
    let (env, token, _token_client, token_admin, client) = setup();
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arb = Address::generate(&env);

    let id = fund(
        &client,
        &token_admin,
        &token,
        &buyer,
        &seller,
        Some(&arb),
        1_000,
    );
    client.raise_dispute(&buyer, &id);

    let res = client.try_resolve_dispute(&buyer, &id, &5000u32, &5000u32);
    assert!(res.is_err());
    let res = client.try_resolve_dispute(&seller, &id, &5000u32, &5000u32);
    assert!(res.is_err());
}

#[test]
fn test_dispute_without_arbitrator_fails() {
    let (env, token, _token_client, token_admin, client) = setup();
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);

    let id = fund(&client, &token_admin, &token, &buyer, &seller, None, 1_000);

    let res = client.try_raise_dispute(&buyer, &id);
    assert!(res.is_err());
}

#[test]
fn test_dispute_after_window_fails() {
    let (env, token, _token_client, token_admin, client) = setup();
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arb = Address::generate(&env);

    let id = fund(
        &client,
        &token_admin,
        &token,
        &buyer,
        &seller,
        Some(&arb),
        1_000,
    );
    advance_past_window(&env);

    let res = client.try_raise_dispute(&buyer, &id);
    assert!(res.is_err());
}

#[test]
fn test_dispute_by_unauthorized_fails() {
    let (env, token, _token_client, token_admin, client) = setup();
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arb = Address::generate(&env);
    let stranger = Address::generate(&env);

    let id = fund(
        &client,
        &token_admin,
        &token,
        &buyer,
        &seller,
        Some(&arb),
        1_000,
    );

    let res = client.try_raise_dispute(&stranger, &id);
    assert!(res.is_err());
    let res = client.try_raise_dispute(&arb, &id);
    assert!(res.is_err());
}

// ============================================
// Auto-release
// ============================================

#[test]
fn test_auto_release_after_window() {
    let (env, token, token_client, token_admin, client) = setup();
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let amount = 7_500i128;

    let id = fund(&client, &token_admin, &token, &buyer, &seller, None, amount);

    // Anyone can call after expiry
    let caller_irrelevant = Address::generate(&env);
    let _ = caller_irrelevant;
    advance_past_window(&env);
    client.auto_release(&id);

    let escrow = client.get_escrow(&id).unwrap();
    assert_eq!(escrow.state, EscrowState::Released);
    assert_eq!(escrow.seller_payout, amount);
    assert_eq!(token_client.balance(&seller), amount);
    assert_eq!(token_client.balance(&client.address), 0);
}

#[test]
fn test_auto_release_before_window_fails() {
    let (env, token, _token_client, token_admin, client) = setup();
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);

    let id = fund(&client, &token_admin, &token, &buyer, &seller, None, 1_000);

    let res = client.try_auto_release(&id);
    assert!(res.is_err());
    assert_eq!(client.get_escrow(&id).unwrap().state, EscrowState::Funded);
}

#[test]
fn test_auto_release_after_dispute_fails() {
    let (env, token, _token_client, token_admin, client) = setup();
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arb = Address::generate(&env);

    let id = fund(
        &client,
        &token_admin,
        &token,
        &buyer,
        &seller,
        Some(&arb),
        1_000,
    );
    client.raise_dispute(&buyer, &id);
    advance_past_window(&env);

    let res = client.try_auto_release(&id);
    assert!(res.is_err());
    assert_eq!(client.get_escrow(&id).unwrap().state, EscrowState::Disputed);
}

#[test]
fn test_auto_release_after_manual_release_fails() {
    let (env, token, _token_client, token_admin, client) = setup();
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);

    let id = fund(&client, &token_admin, &token, &buyer, &seller, None, 1_000);
    client.release(&buyer, &id);
    advance_past_window(&env);

    let res = client.try_auto_release(&id);
    assert!(res.is_err());
}

// ============================================
// Funding validation
// ============================================

#[test]
fn test_fund_invalid_amount() {
    let (env, token, _token_client, _token_admin, client) = setup();
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);

    let res = client.try_fund_escrow(&buyer, &seller, &None, &token, &0i128, &WINDOW_SECS);
    assert!(res.is_err());
}

#[test]
fn test_fund_same_buyer_seller() {
    let (env, token, _token_client, token_admin, client) = setup();
    let buyer = Address::generate(&env);
    token_admin.mint(&buyer, &1_000);

    let res = client.try_fund_escrow(&buyer, &buyer, &None, &token, &1_000i128, &WINDOW_SECS);
    assert!(res.is_err());
}

#[test]
fn test_fund_arbitrator_cannot_be_party() {
    let (env, token, _token_client, token_admin, client) = setup();
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    token_admin.mint(&buyer, &1_000);

    let res = client.try_fund_escrow(
        &buyer,
        &seller,
        &Some(buyer.clone()),
        &token,
        &1_000i128,
        &WINDOW_SECS,
    );
    assert!(res.is_err());
}

#[test]
fn test_fund_zero_window() {
    let (env, token, _token_client, token_admin, client) = setup();
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    token_admin.mint(&buyer, &1_000);

    let res = client.try_fund_escrow(&buyer, &seller, &None, &token, &1_000i128, &0u64);
    assert!(res.is_err());
}

#[test]
fn test_get_missing_escrow() {
    let (_env, _token, _token_client, _token_admin, client) = setup();
    assert!(client.get_escrow(&999u64).is_none());
    assert_eq!(client.get_escrow_counter(), 0);
}

#[test]
fn test_escrow_counter_increments() {
    let (env, token, _token_client, token_admin, client) = setup();
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let seller2 = Address::generate(&env);

    let id1 = fund(&client, &token_admin, &token, &buyer, &seller, None, 100);
    let id2 = fund(&client, &token_admin, &token, &buyer, &seller2, None, 200);

    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
    assert_eq!(client.get_escrow_counter(), 2);
}

#[test]
fn test_operations_on_missing_escrow_fail() {
    let (env, _token, _token_client, _token_admin, client) = setup();
    let caller = Address::generate(&env);

    assert!(client.try_release(&caller, &1u64).is_err());
    assert!(client.try_raise_dispute(&caller, &1u64).is_err());
    assert!(client
        .try_resolve_dispute(&caller, &1u64, &5000u32, &5000u32)
        .is_err());
    assert!(client.try_auto_release(&1u64).is_err());
}

#[test]
fn test_resolve_remainder_avoids_dust() {
    let (env, token, token_client, token_admin, client) = setup();
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arb = Address::generate(&env);
    let amount = 100i128;

    let id = fund(
        &client,
        &token_admin,
        &token,
        &buyer,
        &seller,
        Some(&arb),
        amount,
    );
    client.raise_dispute(&buyer, &id);
    // 3333 bps → buyer 33, seller remainder 67
    client.resolve_dispute(&arb, &id, &3333u32, &6667u32);

    assert_eq!(token_client.balance(&buyer), 33);
    assert_eq!(token_client.balance(&seller), 67);
    assert_eq!(token_client.balance(&client.address), 0);
}

#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Events},
    token, Address, Env, IntoVal, String, Symbol,
};

use crate::{
    events::FeeResetEventData,
    storage::{DEFAULT_FEE_BPS, DEFAULT_MIN_FEE},
    FeeContract, FeeContractClient,
};

fn setup() -> (Env, Address, FeeContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let contract_id = env.register(FeeContract, ());
    let client = FeeContractClient::new(&env, &contract_id);
    client.initialize(&admin, &token, &treasury, &500u32, &1u64);
    (env, admin, client)
}



// --- Init / config tests ---

#[test]
fn test_init_default() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let contract_id = env.register(FeeContract, ());
    let client = FeeContractClient::new(&env, &contract_id);
    client.init(&admin, &token, &treasury);
    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_token(), token);
    assert_eq!(client.get_treasury(), treasury);
    assert_eq!(client.get_fee_bps(), 300);
}

#[test]
fn test_reset_fee_config_restores_defaults() {
    let (env, admin, client) = setup();
    client.set_fee_bps(&admin, &1000u32);
    client.set_min_fee(&admin, &100i128);
    assert_eq!(client.get_fee_bps(), 1000);
    assert_eq!(client.get_min_fee(), 100);
    client.reset_fee_config(&admin);
    assert_eq!(client.get_fee_bps(), 500);
    assert_eq!(client.get_min_fee(), 0);
}



#[test]
fn test_reset_fee_config_emits_event_with_restored_values() {
    let (env, admin, client) = setup();
    client.set_fee_bps(&admin, &1000u32);
    client.set_min_fee(&admin, &75i128);
    let _ = env.events().all();
    client.reset_fee_config(&admin);
    let events = env.events().all();
    let (_, topics, data) = events.last().unwrap();
    assert_eq!(
        topics,
        soroban_sdk::vec![
            &env,
            Symbol::new(&env, "fee").into_val(&env),
            Symbol::new(&env, "reset").into_val(&env),
            1u32.into_val(&env),
        ]
    );
    let payload: FeeResetEventData = data.into_val(&env);
    assert_eq!(payload.admin, admin);
    assert_eq!(payload.fee_bps, DEFAULT_FEE_BPS);
    assert_eq!(payload.min_fee, DEFAULT_MIN_FEE);
    assert_eq!(payload.formatted_min_fee, String::from_str(&env, "0"));
}

#[test]
#[should_panic]
fn test_reset_fee_config_unauthorized_panics() {
    let (env, _admin, client) = setup();
    let non_admin = Address::generate(&env);
    client.reset_fee_config(&non_admin);
}



// --- Validation unit tests ---

#[test]
fn test_validate_fee_bps_valid() {
    use crate::validation::validate_fee_bps;
    assert!(validate_fee_bps(0).is_ok());
    assert!(validate_fee_bps(500).is_ok());
    assert!(validate_fee_bps(10000).is_ok());
}

#[test]
fn test_validate_fee_bps_invalid() {
    use crate::validation::validate_fee_bps;
    use crate::FeeContractError;
    assert_eq!(
        validate_fee_bps(10001),
        Err(FeeContractError::InvalidConfig)
    );
    assert_eq!(
        validate_fee_bps(99999),
        Err(FeeContractError::InvalidConfig)
    );
}

#[test]
fn test_validate_min_fee_valid() {
    use crate::validation::validate_min_fee;
    assert!(validate_min_fee(0).is_ok());
    assert!(validate_min_fee(100).is_ok());
    assert!(validate_min_fee(1000000).is_ok());
}

#[test]
fn test_validate_min_fee_invalid() {
    use crate::validation::validate_min_fee;
    use crate::FeeContractError;
    assert_eq!(validate_min_fee(-1), Err(FeeContractError::InvalidConfig));
    assert_eq!(
        validate_min_fee(-1000),
        Err(FeeContractError::InvalidConfig)
    );
}
#[test]
#[should_panic(expected = "HostError: Error(Contract, #4)")]
fn test_collect_fee_zero_amount_panics() {
    let (env, _admin, client) = setup();
    let payer = Address::generate(&env);
    client.collect_fee(&payer, &0);
}

#[test]
#[should_panic(expected = "HostError: Error(Contract, #4)")]
fn test_collect_fee_negative_amount_panics() {
    let (env, _admin, client) = setup();
    let payer = Address::generate(&env);
    client.collect_fee(&payer, &-100);
}

#[test]
#[should_panic(expected = "HostError: Error(Contract, #1)")]
fn test_collect_fee_uninitialized_panics() {
    let env = Env::default();
    let contract_id = env.register(FeeContract, ());
    let client = FeeContractClient::new(&env, &contract_id);
    let payer = Address::generate(&env);
    client.collect_fee(&payer, &100);
}

#[test]
fn test_getters_return_defaults_when_uninitialized() {
    let env = Env::default();
    let contract_id = env.register(FeeContract, ());
    let client = FeeContractClient::new(&env, &contract_id);

    assert_eq!(client.get_fee_bps(), DEFAULT_FEE_BPS);
    assert_eq!(client.get_min_fee(), DEFAULT_MIN_FEE);
    assert_eq!(client.get_max_fee(), 1_000_000); // DEFAULT_MAX_FEE
}

#[test]
#[should_panic(expected = "HostError: Error(Contract, #4)")]
fn test_collect_fee_batch_zero_amount_panics() {
    let (env, _admin, client) = setup();
    let payer = Address::generate(&env);
    let amounts = soroban_sdk::vec![&env, 100, 0, 200];
    client.collect_fee_batch(&payer, &amounts);
}

// ─── Fee escrow lifecycle tests ───────────────────────────────────────────
//
// These tests verify the **internal fee-escrow bookkeeping** in
// `contracts/fee/src/escrow.rs`.  The tracked balance (`escrow_balance`)
// represents pooled fees awaiting release to the treasury.  This is
// **not** related to the standalone escrow contract at
// `contracts/escrow/src/lib.rs`, which manages individual depositor↔recipient
// escrows.

fn setup_with_real_token() -> (
    Env,
    Address,
    token::Client<'static>,
    token::StellarAssetClient<'static>,
    FeeContractClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);

    // Deploy a real token contract so token transfers actually work
    let issuer = Address::generate(&env);
    let stellar_asset = env.register_stellar_asset_contract_v2(issuer.clone());
    let token_id = stellar_asset.address();
    let token_client = token::Client::new(&env, &token_id);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_id);

    let contract_id = env.register(FeeContract, ());
    let client = FeeContractClient::new(&env, &contract_id);
    client.initialize(&admin, &token_id, &treasury, &500u32, &1u64);

    (env, admin, token_client, token_admin_client, client)
}

#[test]
fn test_fee_escrow_collect_tracks_balance() {
    let (env, _admin, token_client, token_admin, client) = setup_with_real_token();

    let payer = Address::generate(&env);
    let amount: i128 = 10_000_000;

    // Mint tokens to payer
    token_admin.mint(&payer, &amount);

    // Initial balance should be zero
    assert_eq!(client.get_escrow_balance(), 0);
    assert_eq!(client.get_total_collected(), 0);
    assert_eq!(client.get_pending_fees(&1), 0);

    // Collect fee — the fee contract transfers tokens from payer to itself
    // and records the amount in escrow_balance / pending_fees / total_collected
    let pending = client.collect_fee(&payer, &amount);

    // Payer's tokens moved to the fee contract
    assert_eq!(token_client.balance(&payer), 0);
    assert_eq!(token_client.balance(&client.address), amount);

    // Internal bookkeeping updated
    assert_eq!(client.get_escrow_balance(), amount);
    assert_eq!(client.get_total_collected(), amount);
    assert_eq!(pending, amount);
    assert_eq!(client.get_pending_fees(&1), amount);
}

#[test]
fn test_fee_escrow_release_decreases_balance() {
    let (env, admin, token_client, token_admin, client) = setup_with_real_token();

    let payer = Address::generate(&env);
    let treasury = Address::generate(&env);
    let amount: i128 = 10_000_000;

    // Override treasury for verification
    client.set_treasury(&admin, &treasury);

    token_admin.mint(&payer, &amount);
    client.collect_fee(&payer, &amount);

    assert_eq!(client.get_escrow_balance(), amount);

    // Release cycle 1 — tokens go to treasury, escrow_balance decremented
    let released = client.release_fees(&admin, &1);
    assert_eq!(released, amount);

    assert_eq!(client.get_escrow_balance(), 0);
    assert_eq!(client.get_total_released(), amount);
    assert_eq!(client.get_pending_fees(&1), 0);
    assert_eq!(token_client.balance(&treasury), amount);
    assert_eq!(token_client.balance(&client.address), 0);
}

#[test]
fn test_fee_escrow_rollover_moves_pending() {
    let (env, admin, token_client, token_admin, client) = setup_with_real_token();

    let payer = Address::generate(&env);
    let amount: i128 = 5_000_000;

    token_admin.mint(&payer, &amount);
    client.collect_fee(&payer, &amount);

    assert_eq!(client.get_pending_fees(&1), amount);

    // Roll cycle 1 → 5
    let rolled = client.rollover_fees(&admin, &5);
    assert_eq!(rolled, amount);

    assert_eq!(client.get_current_cycle(), 5);
    assert_eq!(client.get_pending_fees(&1), 0);
    assert_eq!(client.get_pending_fees(&5), amount);

    // escrow_balance is unchanged (tokens still in contract)
    assert_eq!(client.get_escrow_balance(), amount);
}

#[test]
fn test_fee_escrow_batch_collect_tracks_balance() {
    let (env, _admin, token_client, token_admin, client) = setup_with_real_token();

    let payer = Address::generate(&env);
    let amounts = soroban_sdk::vec![&env, 1_000_000i128, 2_000_000, 3_000_000];
    let total: i128 = 6_000_000;

    token_admin.mint(&payer, &total);
    let result = client.collect_fee_batch(&payer, &amounts);

    assert_eq!(result.total_amount, total);
    assert_eq!(result.batch_size, 3);
    assert_eq!(result.cycle, 1);

    assert_eq!(token_client.balance(&client.address), total);
    assert_eq!(client.get_escrow_balance(), total);
    assert_eq!(client.get_total_collected(), total);
    assert_eq!(client.get_pending_fees(&1), total);
    assert_eq!(client.get_total_batch_calls(), 1);
}

#[test]
fn test_fee_escrow_multiple_cycles_independent() {
    let (env, admin, token_client, token_admin, client) = setup_with_real_token();

    let payer1 = Address::generate(&env);
    let payer2 = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.set_treasury(&admin, &treasury);

    // Cycle 1: collect 5_000_000
    token_admin.mint(&payer1, &5_000_000);
    client.collect_fee(&payer1, &5_000_000);

    // Roll to cycle 2
    client.rollover_fees(&admin, &2);

    // Cycle 2: collect 3_000_000
    token_admin.mint(&payer2, &3_000_000);
    client.collect_fee(&payer2, &3_000_000);

    // Cycle 2 has 3_000_000, cycle 1 has 0 (rolled to 2)
    assert_eq!(client.get_pending_fees(&1), 0);
    assert_eq!(client.get_pending_fees(&2), 8_000_000);
    assert_eq!(client.get_escrow_balance(), 8_000_000);

    // Release only cycle 2
    let released = client.release_fees(&admin, &2);
    assert_eq!(released, 8_000_000);
    assert_eq!(client.get_escrow_balance(), 0);
    assert_eq!(token_client.balance(&treasury), 8_000_000);
}

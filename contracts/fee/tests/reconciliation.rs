mod support;

use soroban_sdk::{testutils::Address as _, Address};
use support::setup;

#[test]
fn reconciliation_balanced_after_init() {
    let ctx = setup();
    let result = ctx.client.get_reconciliation_status();

    assert_eq!(result.stored_balance, 0);
    assert_eq!(result.calculated_balance, 0);
    assert_eq!(result.discrepancy, 0);
    assert!(result.is_reconciled);
}

#[test]
fn reconciliation_balanced_after_collect() {
    let ctx = setup();

    ctx.client.collect_fee(&ctx.payer, &100);

    let result = ctx.client.reconcile_fees(&ctx.admin);

    assert_eq!(result.stored_balance, 100);
    assert_eq!(result.calculated_balance, 100);
    assert_eq!(result.discrepancy, 0);
    assert!(result.is_reconciled);
}

#[test]
fn reconciliation_balanced_after_batch_collect() {
    let ctx = setup();

    let amounts = support::amounts(&ctx.env, &[100, 200, 300]);
    ctx.client.collect_fee_batch(&ctx.payer, &amounts);

    let result = ctx.client.get_reconciliation_status();

    assert_eq!(result.stored_balance, 600);
    assert_eq!(result.calculated_balance, 600);
    assert_eq!(result.discrepancy, 0);
    assert!(result.is_reconciled);
}

#[test]
fn reconciliation_balanced_after_release() {
    let ctx = setup();

    ctx.client.collect_fee(&ctx.payer, &500);
    ctx.client.release_fees(&ctx.admin, &1);

    let result = ctx.client.reconcile_fees(&ctx.admin);

    assert_eq!(result.stored_balance, 0);
    assert_eq!(result.calculated_balance, 0);
    assert_eq!(result.discrepancy, 0);
    assert!(result.is_reconciled);
}

#[test]
fn reconciliation_balanced_after_partial_release() {
    let ctx = setup();

    // Collect in cycle 1
    ctx.client.collect_fee(&ctx.payer, &300);
    // Rollover to cycle 2 (moves pending from cycle 1 -> 2)
    ctx.client.rollover_fees(&ctx.admin, &2);
    // Collect more in cycle 2
    ctx.client.collect_fee(&ctx.payer, &200);
    // Release only the original rollover amount by releasing cycle 2
    ctx.client.release_fees(&ctx.admin, &2);

    let result = ctx.client.get_reconciliation_status();

    // 500 collected, 500 released (300 rolled + 200 new all in cycle 2)
    assert_eq!(result.stored_balance, 0);
    assert_eq!(result.calculated_balance, 0);
    assert_eq!(result.discrepancy, 0);
    assert!(result.is_reconciled);
}

#[test]
fn reconciliation_balanced_after_rollover() {
    let ctx = setup();

    ctx.client.collect_fee(&ctx.payer, &400);
    ctx.client.rollover_fees(&ctx.admin, &2);

    let result = ctx.client.get_reconciliation_status();

    // Rollover does not change escrow or totals
    assert_eq!(result.stored_balance, 400);
    assert_eq!(result.calculated_balance, 400);
    assert_eq!(result.discrepancy, 0);
    assert!(result.is_reconciled);
}

#[test]
fn reconciliation_balanced_after_multiple_cycles() {
    let ctx = setup();

    // Cycle 1: collect 100
    ctx.client.collect_fee(&ctx.payer, &100);
    ctx.client.rollover_fees(&ctx.admin, &2);

    // Cycle 2: collect 200
    ctx.client.collect_fee(&ctx.payer, &200);
    ctx.client.release_fees(&ctx.admin, &2);

    let result = ctx.client.reconcile_fees(&ctx.admin);

    // 300 collected, 300 released (100 rolled into cycle 2 + 200 new)
    assert_eq!(result.stored_balance, 0);
    assert_eq!(result.calculated_balance, 0);
    assert!(result.is_reconciled);
}

#[test]
fn reconcile_fees_requires_admin() {
    let ctx = setup();
    let non_admin = Address::generate(&ctx.env);

    // get_reconciliation_status has no admin check
    let result = ctx.client.get_reconciliation_status();
    assert!(result.is_reconciled);

    // reconcile_fees requires admin - non-admin should fail
    // Using mock_all_auths so auth passes, but require_admin check will fail
    let env = soroban_sdk::Env::default();
    env.mock_all_auths();

    let issuer = Address::generate(&env);
    let stellar_asset = env.register_stellar_asset_contract_v2(issuer);
    let token_id = stellar_asset.address();

    let contract_id = env.register(fee::FeeContract, ());
    let client = fee::FeeContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let fake_admin = Address::generate(&env);

    client.initialize(&admin, &token_id, &treasury, &250u32, &1u64);

    // This should panic because fake_admin is not the admin
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.reconcile_fees(&fake_admin);
    }));
    assert!(result.is_err());
}

#[test]
fn reconciliation_result_fields_correct() {
    let ctx = setup();

    ctx.client.collect_fee(&ctx.payer, &1000);
    ctx.client.collect_fee(&ctx.payer, &500);
    // Release cycle 1
    ctx.client.release_fees(&ctx.admin, &1);

    let result = ctx.client.get_reconciliation_status();

    assert_eq!(result.stored_balance, 0);
    assert_eq!(result.calculated_balance, 0);
    assert_eq!(result.discrepancy, 0);
    assert!(result.is_reconciled);

    // Collect more after release
    ctx.client.collect_fee(&ctx.payer, &250);
    let result = ctx.client.get_reconciliation_status();

    assert_eq!(result.stored_balance, 250);
    assert_eq!(result.calculated_balance, 250);
    assert_eq!(result.discrepancy, 0);
    assert!(result.is_reconciled);
}

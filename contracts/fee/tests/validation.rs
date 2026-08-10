mod support;

use support::setup;


#[test]
#[should_panic]
fn initialize_rejects_invalid_fee_bps() {
    let ctx = setup();
    // lock/unlock not required; call setter which uses helper
    ctx.client.set_fee_bps(&ctx.admin, &10_001u32);
}

#[test]
#[should_panic]
fn set_fee_bps_rejects_fee_above_100_percent() {
    let ctx = setup();
    ctx.client.set_fee_bps(&ctx.admin, &10_001u32);
}

#[test]
#[should_panic]
fn set_min_fee_rejects_negative() {
    let ctx = setup();
    ctx.client.set_min_fee(&ctx.admin, &-5i128);
}

#[test]
fn setters_accept_valid_values() {
    let ctx = setup();
    ctx.client.set_fee_bps(&ctx.admin, &500u32);
    ctx.client.set_min_fee(&ctx.admin, &100i128);
    assert_eq!(ctx.client.get_fee_bps(), 500);
    assert_eq!(ctx.client.get_min_fee(), 100);
}

#[test]
fn batch_with_min_fee_and_reconciliation_end_to_end() {
    let ctx = setup();
    ctx.client.set_min_fee(&ctx.admin, &10i128);

    let batch = support::amounts(&ctx.env, &[10, 20, 30, 100]);
    let batch_res = ctx.client.collect_fee_batch(&ctx.payer, &batch);

    assert_eq!(batch_res.total_amount, 160);
    assert_eq!(batch_res.pending_fees, 160);

    let r = ctx.client.get_reconciliation_status();
    assert_eq!(r.stored_balance, 160);
    assert_eq!(r.calculated_balance, 160);
    assert!(r.is_reconciled);

    ctx.client.rollover_fees(&ctx.admin, &2);
    ctx.client.collect_fee(&ctx.payer, &50);
    ctx.client.release_fees(&ctx.admin, &2);

    let r2 = ctx.client.reconcile_fees(&ctx.admin);
    assert_eq!(r2.stored_balance, 0);
    assert!(r2.is_reconciled);
}

#[test]
fn escrow_balance_always_matches_pending_fees_in_single_cycle() {
    let ctx = setup();

    ctx.client.collect_fee(&ctx.payer, &111);
    ctx.client.collect_fee(&ctx.payer, &222);
    ctx.client.collect_fee(&ctx.payer, &333);

    assert_eq!(
        ctx.client.get_escrow_balance(),
        ctx.client.get_pending_fees(&1),
        "escrow_balance must equal pending_fees for cycle 1"
    );
}

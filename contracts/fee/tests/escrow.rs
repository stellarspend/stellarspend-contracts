mod support;

use support::setup;

#[test]
fn fees_are_held_then_released_from_escrow() {
    let ctx = setup();

    assert_eq!(ctx.client.get_escrow_balance(), 0);
    assert_eq!(ctx.client.get_pending_fees(&1), 0);

    let pending = ctx.client.collect_fee(&ctx.payer, &150i128);
    assert_eq!(pending, 150);
    assert_eq!(ctx.client.get_escrow_balance(), 150);
    assert_eq!(ctx.client.get_pending_fees(&1), 150);
    assert_eq!(ctx.client.get_total_collected(), 150);
    assert_eq!(ctx.token_client.balance(&ctx.contract_id), 150);
    assert_eq!(ctx.token_client.balance(&ctx.treasury), 0);
    assert_eq!(ctx.token_client.balance(&ctx.payer), 999_850);

    let released = ctx.client.release_fees(&ctx.admin, &1u64);
    assert_eq!(released, 150);
    assert_eq!(ctx.client.get_escrow_balance(), 0);
    assert_eq!(ctx.client.get_pending_fees(&1), 0);
    assert_eq!(ctx.client.get_total_released(), 150);
    assert_eq!(ctx.token_client.balance(&ctx.contract_id), 0);
    assert_eq!(ctx.token_client.balance(&ctx.treasury), 150);
}

#[test]
#[should_panic]
fn releasing_without_pending_fees_panics() {
    let ctx = setup();
    ctx.client.release_fees(&ctx.admin, &1u64);
}

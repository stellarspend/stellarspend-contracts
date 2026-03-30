mod support;

use soroban_sdk::testutils::Ledger;
use support::setup;

#[test]
fn test_update_activity_and_get_last_active() {
    let ctx = setup();
    let user = ctx.payer.clone();

    // Initial last active should be 0
    assert_eq!(ctx.client.get_last_active(&user), 0);

    // Update activity
    ctx.client.update_activity(&user);
    
    // Ledger timestamp in setup is 0 by default usually, but let's check
    let current_time = ctx.env.ledger().timestamp();
    assert_eq!(ctx.client.get_last_active(&user), current_time);
}

#[test]
fn test_fee_decay_logic() {
    let ctx = setup();
    let user = ctx.payer.clone();
    
    // Set initial activity
    ctx.env.ledger().with_mut(|li| li.timestamp = 1000);
    ctx.client.update_activity(&user);
    assert_eq!(ctx.client.get_last_active(&user), 1000);

    // Move time forward by 10 seconds
    ctx.env.ledger().with_mut(|li| li.timestamp = 1010);
    
    // base_fee = 100, elapsed = 10, decay_rate = 1 => decay = 10
    // expected decayed_fee = 100 - 10 = 90
    let pending = ctx.client.collect_fee(&user, &100i128);
    assert_eq!(pending, 90);
    
    // Last active should be updated to 1010
    assert_eq!(ctx.client.get_last_active(&user), 1010);
}

#[test]
fn test_fee_decay_min_fee() {
    let ctx = setup();
    let user = ctx.payer.clone();
    
    // Set initial activity
    ctx.env.ledger().with_mut(|li| li.timestamp = 1000);
    ctx.client.update_activity(&user);

    // Move time forward by 1000 seconds
    ctx.env.ledger().with_mut(|li| li.timestamp = 2000);
    
    // base_fee = 100, elapsed = 1000, decay = 1000
    // 100 - 1000 = -900, but min_fee is 10
    let pending = ctx.client.collect_fee(&user, &100i128);
    assert_eq!(pending, 10);
}

#[test]
fn test_fee_decay_batch() {
    let ctx = setup();
    let user = ctx.payer.clone();
    
    // Set initial activity
    ctx.env.ledger().with_mut(|li| li.timestamp = 1000);
    ctx.client.update_activity(&user);

    // Move time forward by 20 seconds
    ctx.env.ledger().with_mut(|li| li.timestamp = 1020);
    
    // 100 -> 100 - 20 = 80
    // 50 -> 50 - 20 = 30
    // 25 -> 25 - 20 = 5 (wait, min_fee is 10, so 10)
    let amounts = support::amounts(&ctx.env, &[100, 50, 25]);
    let res = ctx.client.collect_fee_batch(&user, &amounts);
    
    // 80 + 30 + 10 = 120
    assert_eq!(res.total_amount, 120);
    assert_eq!(res.batch_size, 3);
}

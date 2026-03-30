mod support;

use soroban_sdk::Address;
use soroban_sdk::testutils::Address as _;
use support::setup;

#[test]
fn test_calculate_fee_uses_global_config() {
    let ctx = setup();
    let user = ctx.payer.clone();

    // Global fee_bps is 250 (set in setup)
    let amount = 10000i128;
    let expected_fee = (amount * 250) / 10000; // 250

    let fee = ctx.client.calculate_fee(&user, &amount);
    assert_eq!(fee, expected_fee);
}

#[test]
fn test_calculate_fee_uses_user_override() {
    let ctx = setup();
    let user = ctx.payer.clone();
    let admin = ctx.admin.clone();

    // Set user override to 500 bps
    ctx.client.set_user_fee_override(&admin, &user, &500u32);

    let amount = 10000i128;
    let expected_fee = (amount * 500) / 10000; // 500

    let fee = ctx.client.calculate_fee(&user, &amount);
    assert_eq!(fee, expected_fee);
}

#[test]
fn test_calculate_fee_override_takes_precedence() {
    let ctx = setup();
    let user = ctx.payer.clone();
    let admin = ctx.admin.clone();

    // First calculate with global (250)
    let amount = 10000i128;
    let global_fee = ctx.client.calculate_fee(&user, &amount);
    assert_eq!(global_fee, 250);

    // Set override to 100 bps
    ctx.client.set_user_fee_override(&admin, &user, &100u32);

    // Now should use override
    let override_fee = ctx.client.calculate_fee(&user, &amount);
    assert_eq!(override_fee, 100);
}

#[test]
fn test_calculate_fee_respects_min_fee() {
    let ctx = setup();
    let user = ctx.payer.clone();
    let admin = ctx.admin.clone();

    // Set min_fee to 50
    ctx.client.set_min_fee(&admin, &50i128);

    // Set user override to very low rate
    ctx.client.set_user_fee_override(&admin, &user, &1u32); // 0.01%

    let amount = 10000i128;
    let fee = ctx.client.calculate_fee(&user, &amount);
    // Calculated fee would be 0.1, but min_fee is 50
    assert_eq!(fee, 50);
}

#[test]
#[should_panic]
fn test_set_user_fee_override_requires_admin() {
    let ctx = setup();
    let user = ctx.payer.clone();
    let non_admin = Address::generate(&ctx.env);

    // Should panic when non-admin tries to set override
    ctx.client.set_user_fee_override(&non_admin, &user, &500u32);
}

#[test]
fn test_remove_user_fee_override() {
    let ctx = setup();
    let user = ctx.payer.clone();
    let admin = ctx.admin.clone();

    // Set override
    ctx.client.set_user_fee_override(&admin, &user, &500u32);
    let fee_with_override = ctx.client.calculate_fee(&user, &10000i128);
    assert_eq!(fee_with_override, 500);

    // Remove override
    ctx.client.remove_user_fee_override(&admin, &user);

    // Should fall back to global
    let fee_global = ctx.client.calculate_fee(&user, &10000i128);
    assert_eq!(fee_global, 250);
}

#[test]
fn test_get_user_fee_bps_with_override() {
    let ctx = setup();
    let user = ctx.payer.clone();
    let admin = ctx.admin.clone();

    // Initially should return global
    assert_eq!(ctx.client.get_user_fee_bps(&user), 250);

    // Set override
    ctx.client.set_user_fee_override(&admin, &user, &300u32);
    assert_eq!(ctx.client.get_user_fee_bps(&user), 300);

    // Remove override
    ctx.client.remove_user_fee_override(&admin, &user);
    assert_eq!(ctx.client.get_user_fee_bps(&user), 250);
}

#[test]
#[should_panic]
fn test_calculate_fee_invalid_amount() {
    let ctx = setup();
    let user = ctx.payer.clone();

    // Should panic with invalid amount
    ctx.client.calculate_fee(&user, &0i128);
}

#[test]
#[should_panic]
fn test_calculate_fee_invalid_amount_negative() {
    let ctx = setup();
    let user = ctx.payer.clone();

    // Should panic with invalid amount
    ctx.client.calculate_fee(&user, &-100i128);
}

#[test]
#[should_panic]
fn test_set_user_fee_override_invalid_bps() {
    let ctx = setup();
    let user = ctx.payer.clone();
    let admin = ctx.admin.clone();

    // Should panic with invalid fee_bps (> MAX_FEE_BPS = 10000)
    ctx.client.set_user_fee_override(&admin, &user, &15000u32);
}
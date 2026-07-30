#![cfg(test)]

extern crate std;

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, Error, InvokeError};

fn assert_contract_error<T: core::fmt::Debug>(
    result: Result<T, Result<Error, InvokeError>>,
    expected: AssetControlError,
) {
    match result {
        Err(Ok(error)) => assert_eq!(error, Error::from_contract_error(expected as u32)),
        other => panic!("expected contract error {:?}, got {:?}", expected, other),
    }
}

fn create_contract() -> (Env, Address, AssetControlContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AssetControlContract, ());
    let client = AssetControlContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    (env, admin, client)
}

#[test]
fn test_blacklist_flow_allows_add_and_remove() {
    let (env, _admin, client) = create_contract();
    let asset = Address::generate(&env);

    assert!(!client.is_blacklisted(&asset));

    client.add_to_blacklist(&asset);
    assert!(client.is_blacklisted(&asset));

    client.remove_from_blacklist(&asset);
    assert!(!client.is_blacklisted(&asset));
}

#[test]
fn test_blacklist_rejects_duplicate_addition() {
    let (env, _admin, client) = create_contract();
    let asset = Address::generate(&env);

    client.add_to_blacklist(&asset);
    let result = client.try_add_to_blacklist(&asset);
    assert_contract_error(result, AssetControlError::AlreadyBlacklisted);
}

#[test]
fn test_blacklist_rejects_remove_for_non_blacklisted_asset() {
    let (env, _admin, client) = create_contract();
    let asset = Address::generate(&env);

    let result = client.try_remove_from_blacklist(&asset);
    assert_contract_error(result, AssetControlError::NotBlacklisted);
}

#[test]
fn test_check_asset_panics_for_blacklisted_asset() {
    let (env, _admin, client) = create_contract();
    let asset = Address::generate(&env);

    client.add_to_blacklist(&asset);

    let result = client.try_check_asset(&asset);
    assert_contract_error(result, AssetControlError::Unauthorized);
}

#[test]
fn test_check_asset_does_not_panic_for_non_blacklisted_asset() {
    let (env, _admin, client) = create_contract();
    let asset = Address::generate(&env);

    client.check_asset(&asset);
}

// ─── Unauthorized-caller tests ──────────────────────────────────────────────
//
// `add_to_blacklist` and `remove_from_blacklist` are gated purely by
// `admin.require_auth()` on the stored admin address (see lib.rs) — there is
// no separate caller parameter to pass a "wrong" address for, so the only
// way to exercise the unauthorized path is to withhold a valid
// authorization for that call. `env.set_auths(&[])` does exactly that: it
// disables the blanket `mock_all_auths()` set up in `create_contract` for
// the next invocation, so `require_auth()` has nothing to match and fails.
//
// This is a genuine host-level authorization failure, not the contract's
// own `AssetControlError::Unauthorized` (which `check_asset` uses for a
// different purpose — flagging a blacklisted asset, not a caller identity
// problem) — so these assert that the call errors out, rather than
// asserting a specific contract error code.

#[test]
fn test_unauthorized_caller_cannot_add_to_blacklist() {
    let (env, _admin, client) = create_contract();
    let asset = Address::generate(&env);

    env.set_auths(&[]);
    let result = client.try_add_to_blacklist(&asset);

    assert!(
        result.is_err(),
        "expected an auth failure, got {:?}",
        result
    );
    assert!(!client.is_blacklisted(&asset));
}

#[test]
fn test_unauthorized_caller_cannot_remove_from_blacklist() {
    let (env, _admin, client) = create_contract();
    let asset = Address::generate(&env);

    // Blacklist the asset first, under a valid (mocked) admin authorization.
    client.add_to_blacklist(&asset);

    env.set_auths(&[]);
    let result = client.try_remove_from_blacklist(&asset);

    assert!(
        result.is_err(),
        "expected an auth failure, got {:?}",
        result
    );
    assert!(client.is_blacklisted(&asset));
}

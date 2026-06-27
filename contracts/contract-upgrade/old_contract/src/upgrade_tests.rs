#![cfg(test)]
extern crate std;

// Scenario tests for the multisig + timelock upgrade authorization.
//
// These tests are hermetic: they exercise the authorization and timelock
// guards directly via the generated client and do not depend on a prebuilt
// Wasm artifact. The happy path is asserted up to `is_upgrade_ready` (the
// point at which all guards pass); the final `update_current_contract_wasm`
// host call requires a real uploaded Wasm hash and is exercised by on-chain /
// integration testing rather than here.

use crate::{UpgradeError, UpgradeableContract, UpgradeableContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    vec, Address, BytesN, Env, Error, InvokeError,
};

<<<<<<< HEAD
const DELAY: u64 = 48 * 60 * 60;
const START: u64 = 1_000_000;

fn hash(e: &Env) -> BytesN<32> {
    BytesN::from_array(e, &[9u8; 32])
}

/// Assert that a `try_*` client call failed with the given contract error.
fn assert_err<T: core::fmt::Debug>(
    res: Result<T, Result<Error, InvokeError>>,
    expected: UpgradeError,
) {
    match res {
        Err(Ok(e)) => assert_eq!(e, Error::from_contract_error(expected as u32)),
        other => std::panic!("expected contract error {:?}, got {:?}", expected, other),
    }
=======
mod old_contract {
    soroban_sdk::contractimport!(
        file = "../../../target/wasm32-unknown-unknown/release/soroban_upgradeable_contract_old_contract.wasm"
    );
}

mod new_contract {
    soroban_sdk::contractimport!(
        file = "../../../target/wasm32-unknown-unknown/release/soroban_upgradeable_contract_new_contract.wasm"
    );
>>>>>>> 067107d (fix(contracts): fix CI compilation errors across batch-transfer, spending-limits, multi-currency-wallet, and batch-rewards)
}

/// Register a contract with a 2-of-3 multisig and the default timelock.
fn setup_2of3(env: &Env) -> (Address, Address, Address, Address) {
    let admin = Address::generate(env);
    let id = env.register(UpgradeableContract, (&admin,));
    let client = UpgradeableContractClient::new(env, &id);

    let s1 = Address::generate(env);
    let s2 = Address::generate(env);
    let s3 = Address::generate(env);
    client.set_upgrade_signers(&vec![env, s1.clone(), s2.clone(), s3.clone()], &2);

    (id, s1, s2, s3)
}

// --- Acceptance: unauthorized upgrades rejected ---------------------------

#[test]
fn test_non_signer_cannot_schedule() {
    let env = Env::default();
    env.mock_all_auths();
    let (id, _s1, _s2, _s3) = setup_2of3(&env);
    let client = UpgradeableContractClient::new(&env, &id);

    let stranger = Address::generate(&env);
    let res = client.try_schedule_upgrade(&stranger, &hash(&env), &2);
    assert_err(res, UpgradeError::NotAuthorized);
}

#[test]
fn test_non_signer_cannot_approve() {
    let env = Env::default();
    env.mock_all_auths();
    let (id, s1, _s2, _s3) = setup_2of3(&env);
    let client = UpgradeableContractClient::new(&env, &id);

    client.schedule_upgrade(&s1, &hash(&env), &2);
    let stranger = Address::generate(&env);
    let res = client.try_approve_upgrade(&stranger);
    assert_err(res, UpgradeError::NotAuthorized);
}

#[test]
fn test_non_signer_cannot_execute() {
    let env = Env::default();
    env.mock_all_auths();
    let (id, s1, _s2, _s3) = setup_2of3(&env);
    let client = UpgradeableContractClient::new(&env, &id);

    client.schedule_upgrade(&s1, &hash(&env), &2);
    let stranger = Address::generate(&env);
    let res = client.try_execute_upgrade(&stranger);
    assert_err(res, UpgradeError::NotAuthorized);
}

#[test]
fn test_unauthorized_when_no_auth_provided() {
    // Without mock_all_auths, require_auth() itself must reject the call.
    let env = Env::default();
    let admin = Address::generate(&env);
<<<<<<< HEAD
    let id = env.register(UpgradeableContract, (&admin,));
    let client = UpgradeableContractClient::new(&env, &id);

    let res = client.try_schedule_upgrade(&admin, &hash(&env), &2);
    assert!(res.is_err());
=======
    let contract_id = env.register(old_contract::WASM, (&admin,));
    let client = old_contract::Client::new(&env, &contract_id);
    let new_wasm_hash = install_new_wasm(&env);
    client.upgrade(&new_wasm_hash, &2);
>>>>>>> 067107d (fix(contracts): fix CI compilation errors across batch-transfer, spending-limits, multi-currency-wallet, and batch-rewards)
}

// --- Acceptance: threshold (multisig) enforced ----------------------------

#[test]
fn test_execute_rejected_below_threshold() {
    let env = Env::default();
    env.mock_all_auths();
<<<<<<< HEAD
    env.ledger().set_timestamp(START);
    let (id, s1, _s2, _s3) = setup_2of3(&env);
    let client = UpgradeableContractClient::new(&env, &id);

    // Only the proposer has approved (1 of 2 required).
    client.schedule_upgrade(&s1, &hash(&env), &2);
    assert_eq!(client.upgrade_approval_count(), 1);

    // Even after the timelock elapses, a single approval is insufficient.
    env.ledger().set_timestamp(START + DELAY + 1);
    assert!(!client.is_upgrade_ready());
    let res = client.try_execute_upgrade(&s1);
    assert_err(res, UpgradeError::ThresholdNotMet);
=======
    let (_, contract_id) = setup(&env);
    let client = old_contract::Client::new(&env, &contract_id);
    let new_wasm_hash = install_new_wasm(&env);
    client.upgrade(&new_wasm_hash, &2);
    let events = env.events().all();
    assert!(!events.is_empty(), "upgrade should emit an event");
>>>>>>> 067107d (fix(contracts): fix CI compilation errors across batch-transfer, spending-limits, multi-currency-wallet, and batch-rewards)
}

#[test]
fn test_duplicate_approval_rejected() {
    let env = Env::default();
    env.mock_all_auths();
<<<<<<< HEAD
    let (id, s1, _s2, _s3) = setup_2of3(&env);
    let client = UpgradeableContractClient::new(&env, &id);

    client.schedule_upgrade(&s1, &hash(&env), &2);
    // Proposer already auto-approved; approving again must fail.
    let res = client.try_approve_upgrade(&s1);
    assert_err(res, UpgradeError::AlreadyApproved);
=======
    let (_, contract_id) = setup(&env);
    let client = old_contract::Client::new(&env, &contract_id);
    assert_eq!(1, client.version());
    let new_wasm_hash = install_new_wasm(&env);
    client.upgrade(&new_wasm_hash, &2);
    // version should now be 2
    let new_client = new_contract::Client::new(&env, &contract_id);
    assert_eq!(2, new_client.version());
>>>>>>> 067107d (fix(contracts): fix CI compilation errors across batch-transfer, spending-limits, multi-currency-wallet, and batch-rewards)
}

// --- Acceptance: timelock enforced ----------------------------------------

#[test]
fn test_execute_rejected_before_timelock() {
    let env = Env::default();
    env.mock_all_auths();
<<<<<<< HEAD
    env.ledger().set_timestamp(START);
    let (id, s1, s2, _s3) = setup_2of3(&env);
    let client = UpgradeableContractClient::new(&env, &id);

    client.schedule_upgrade(&s1, &hash(&env), &2);
    client.approve_upgrade(&s2);
    assert_eq!(client.upgrade_approval_count(), 2);

    // Threshold met but timelock not yet elapsed.
    assert!(!client.is_upgrade_ready());
    let res = client.try_execute_upgrade(&s1);
    assert_err(res, UpgradeError::TimelockNotElapsed);

    // One second before the deadline is still too early.
    env.ledger().set_timestamp(START + DELAY - 1);
    let res = client.try_execute_upgrade(&s1);
    assert_err(res, UpgradeError::TimelockNotElapsed);
=======
    let (_, contract_id) = setup(&env);
    let client = old_contract::Client::new(&env, &contract_id);
    let new_wasm_hash = install_new_wasm(&env);
    client.upgrade(&new_wasm_hash, &2);
    let new_client = new_contract::Client::new(&env, &contract_id);
    // NewAdmin key not set yet, second upgrade should fail
    let result = new_client.try_upgrade(&new_wasm_hash, &3);
    assert!(
        result.is_err(),
        "upgrade should fail without handle_upgrade"
    );
>>>>>>> 067107d (fix(contracts): fix CI compilation errors across batch-transfer, spending-limits, multi-currency-wallet, and batch-rewards)
}

#[test]
fn test_ready_only_when_threshold_and_timelock_satisfied() {
    let env = Env::default();
    env.mock_all_auths();
<<<<<<< HEAD
    env.ledger().set_timestamp(START);
    let (id, s1, s2, _s3) = setup_2of3(&env);
    let client = UpgradeableContractClient::new(&env, &id);

    client.schedule_upgrade(&s1, &hash(&env), &2);
    assert!(!client.is_upgrade_ready()); // 1 approval, timelock pending

    client.approve_upgrade(&s2);
    assert!(!client.is_upgrade_ready()); // threshold met, timelock pending

    env.ledger().set_timestamp(START + DELAY);
    // Both conditions satisfied: the upgrade would now be allowed to execute.
    assert!(client.is_upgrade_ready());

    let pending = client.get_pending_upgrade().unwrap();
    assert_eq!(pending.new_version, 2);
    assert_eq!(pending.execute_at, START + DELAY);
    assert_eq!(pending.approvals.len(), 2);
}

#[test]
fn test_zero_delay_still_requires_threshold() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(START);
    let (id, s1, _s2, _s3) = setup_2of3(&env);
    let client = UpgradeableContractClient::new(&env, &id);

    // Admin can shorten the timelock, but multisig is still enforced.
    client.set_timelock_delay(&0);
    client.schedule_upgrade(&s1, &hash(&env), &2);
    assert!(!client.is_upgrade_ready()); // only 1 of 2 approvals

    let res = client.try_execute_upgrade(&s1);
    assert_err(res, UpgradeError::ThresholdNotMet);
=======
    let (_, contract_id) = setup(&env);
    let client = old_contract::Client::new(&env, &contract_id);
    let new_wasm_hash = install_new_wasm(&env);
    client.upgrade(&new_wasm_hash, &2);
    let new_client = new_contract::Client::new(&env, &contract_id);
    new_client.handle_upgrade();
    // after migration, upgrade should succeed
    let result = new_client.try_upgrade(&new_wasm_hash, &3);
    assert!(
        result.is_ok(),
        "upgrade should succeed after handle_upgrade"
    );
>>>>>>> 067107d (fix(contracts): fix CI compilation errors across batch-transfer, spending-limits, multi-currency-wallet, and batch-rewards)
}

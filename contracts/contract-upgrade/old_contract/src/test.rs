#![cfg(test)]

extern crate std;

use crate::{UpgradeError, UpgradeableContract, UpgradeableContractClient};
use soroban_sdk::{testutils::Address as _, vec, Address, BytesN, Env, Error, InvokeError};

const DELAY: u64 = 48 * 60 * 60;

fn dummy_hash(e: &Env) -> BytesN<32> {
    BytesN::from_array(e, &[7u8; 32])
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
}

#[test]
fn test_default_configuration() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let id = env.register(UpgradeableContract, (&admin,));
    let client = UpgradeableContractClient::new(&env, &id);

    assert_eq!(client.version(), 1);
    assert_eq!(client.get_threshold(), 1);
    assert_eq!(client.get_signers().len(), 1);
    assert_eq!(client.get_timelock_delay(), DELAY);
    assert!(client.get_pending_upgrade().is_none());
    assert!(!client.is_upgrade_ready());
}

#[test]
fn test_admin_can_configure_signers_and_delay() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let id = env.register(UpgradeableContract, (&admin,));
    let client = UpgradeableContractClient::new(&env, &id);

    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    let s3 = Address::generate(&env);
    client.set_upgrade_signers(&vec![&env, s1, s2, s3], &2);
    client.set_timelock_delay(&3600);

    assert_eq!(client.get_signers().len(), 3);
    assert_eq!(client.get_threshold(), 2);
    assert_eq!(client.get_timelock_delay(), 3600);
}

#[test]
fn test_set_signers_rejects_duplicate() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let id = env.register(UpgradeableContract, (&admin,));
    let client = UpgradeableContractClient::new(&env, &id);

    let s1 = Address::generate(&env);
    let res = client.try_set_upgrade_signers(&vec![&env, s1.clone(), s1], &1);
    assert_err(res, UpgradeError::DuplicateSigner);
}

#[test]
fn test_set_signers_rejects_bad_threshold() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let id = env.register(UpgradeableContract, (&admin,));
    let client = UpgradeableContractClient::new(&env, &id);

    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    // threshold greater than number of signers
    let res = client.try_set_upgrade_signers(&vec![&env, s1.clone(), s2], &3);
    assert_err(res, UpgradeError::InvalidThreshold);
    // zero threshold
    let res = client.try_set_upgrade_signers(&vec![&env, s1], &0);
    assert_err(res, UpgradeError::InvalidThreshold);
}

#[test]
fn test_set_signers_rejects_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let id = env.register(UpgradeableContract, (&admin,));
    let client = UpgradeableContractClient::new(&env, &id);

    let empty: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    let res = client.try_set_upgrade_signers(&empty, &1);
    assert_err(res, UpgradeError::EmptySigners);
}

#[test]
fn test_schedule_downgrade_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let id = env.register(UpgradeableContract, (&admin,));
    let client = UpgradeableContractClient::new(&env, &id);

    // current version is 1; scheduling version 1 (no increase) must be rejected.
    let res = client.try_schedule_upgrade(&admin, &dummy_hash(&env), &1);
    assert_err(res, UpgradeError::InvalidVersion);
}

#[test]
fn test_double_schedule_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let id = env.register(UpgradeableContract, (&admin,));
    let client = UpgradeableContractClient::new(&env, &id);

    client.schedule_upgrade(&admin, &dummy_hash(&env), &2);
    let res = client.try_schedule_upgrade(&admin, &dummy_hash(&env), &3);
    assert_err(res, UpgradeError::PendingUpgradeExists);
}

#[test]
fn test_actions_without_pending_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let id = env.register(UpgradeableContract, (&admin,));
    let client = UpgradeableContractClient::new(&env, &id);

    assert_err(
        client.try_approve_upgrade(&admin),
        UpgradeError::NoPendingUpgrade,
    );
    assert_err(
        client.try_execute_upgrade(&admin),
        UpgradeError::NoPendingUpgrade,
    );
    assert_err(
        client.try_cancel_upgrade(&admin),
        UpgradeError::NoPendingUpgrade,
    );
}

#[test]
fn test_cancel_clears_pending_and_allows_reschedule() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let id = env.register(UpgradeableContract, (&admin,));
    let client = UpgradeableContractClient::new(&env, &id);

    client.schedule_upgrade(&admin, &dummy_hash(&env), &2);
    assert!(client.get_pending_upgrade().is_some());

    client.cancel_upgrade(&admin);
    assert!(client.get_pending_upgrade().is_none());

    // After cancellation a new proposal can be scheduled again.
    client.schedule_upgrade(&admin, &dummy_hash(&env), &2);
    assert!(client.get_pending_upgrade().is_some());
}

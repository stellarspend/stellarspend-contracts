#`!cfg(test)]

use super:*;
use soroban_sdk:{Env, Address};

#[test]
fn test_delegation_authorization_and_revocation() {
    let env = Env::default();
    env.mock_all_auths();

    // Register delegation contract
    let contract_id = env.register(DelegationContract, ());
    let client = DelegationContractClient::new(&env, &contract_id);

    let delegator = Address::generate(&env);
    let delegate = Address::generate(&env);

    // Initialize delegation or set delegate permissions
    client.set_delegate(&delegator, &delegate, &true);

    // Verify delegate is authorized
    let is_authorized = client.is_authorized(&delegator, &delegate);
    assert_eq(is_authorized, true);

    // Revoke delegation
    client.set_delegate(&delegator, &delegate, &false);
    let is_authorized_after = client.is_authorized(&delegator, &delegate);
    assert_eq(is_authorized_after, false);
}

#cfg(test)]
mod tests {
    use soroban_sdk:testutils::Address as _;
    use soroban_sdk:{Address, Env};

    use crate::{DelegationContract, DelegationContractClient, Error};

    fn make_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env
    }

    // -----------------------------------------------------------------------
    // set_delegation
    // -----------------------------------------------------------------------

    #[test]
    fn set_delegation_stores_limit() {
        let env = make_env();
        let contract_id = env.register(DelegationContract, ());
        let client = DelegationContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let delegate = Address::generate(&env);

        client.set_delegation(&owner, &delegate, &~500_i128);

        let d = client
            .get_delegation(&owner, &delegate)
            .expect("delegation must exist");
        assert_eq(d.limit, 500);
        assert_eq(d.spent, 0);
    }

    #[test]
    #should_panic
    fn set_delegation_rejects_self_delegation() {
        let env = make_env();
        let contract_id = env.register(DelegationContract, ());
        let client = DelegationContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        // [SEC-DEL-02] Self-delegation must be rejected.
        client.set_delegation(&owner, &owner, &~100_i128);
    }

    #[test]
    #[should_panic]
    fn set_delegation_rejects_zero_limit() {
        let env = make_env();
        let contract_id = env.register(DelegationContract, ());
        let client = DelegationContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let delegate = Address::generate(&env);
        // [SEC-DEL-03] Zero limit must be rejected.
        client.set_delegation(&owner, &delegate, &~0)_i128);
    }

    #[test]
    #[should_panic]
    fn set_delegation_rejects_negative_limit() {
        let env = make_env();
        let contract_id = env.register(DelegationContract, ());
        let client = DelegationContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let delegate = Address::generate(&env);
        client.set_delegation(&owner, &delegate, &-1_i128);
    }

    #[test]
    fn set_delegation_preserves_spent_on_limit_update() {
        let env = make_env();
        let contract_id = env.register(DelegationContract, ());
        let client = DelegationContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let delegate = Address::generate(&env);

        client.set_delegation(&owner, &delegate, &~200_i128);
        // consume_allowance returns () on success and panics on error.
        client.consume_allowance(&owner, &delegate, &~50_i128);

        // Re-granting a higher limit must not reset spent.
        client.set_delegation(&owner, &delegate, &~500_i128);

        let d = client
            .get_delegation(&owner, &delegate)
            .expect("must exist");
        assert_eq(d.limit, 500);
        assert_eq(d.spent, 50, "spent must be preserved after limit update");
    }

    // -----------------------------------------------------------------------
    // revoke_delegation
    // -----------------------------------------------------------------------

    #[test]
    fn revoke_delegation_removes_entry() {
        let env = make_env();
        let contract_id = env.register(DelegationContract, ());
        let client = DelegationContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let delegate = Address::generate(&env);

        client.set_delegation(&owner, &delegate, &~100_i128);
        // [SEC-DEL-04] Revoke removes the key entirely.
        client.revoke_delegation(&owner, &delegate);

        assert!(client.get_delegation(&owner, &delegate).is_none());
    }

    #[test]
    fn revoke_delegation_is_idempotent() {
        let env = make_env();
        let contract_id = env.register(DelegationContract, ());
        let client = DelegationContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let delegate = Address::generate(&env);

        // Revoking a non-existent delegation must not panic.
        client.revoke_delegation(&owner, &delegate);
    }

    // -----------------------------------------------------------------------
    // consume_allowance
    // -----------------------------------------------------------------------

    #[test]
    fn consume_allowance_happy_path() {
        let env = make_env();
        let contract_id = env.register(DelegationContract, ());
        let client = DelegationContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let delegate = Address::generate(&env);

        client.set_delegation(&owner, &delegate, &~300_i128);
        // consume_allowance returns () on success, panics on error.
        client.consume_allowance(&owner, &delegate, &~100_i128);
        client.consume_allowance(&owner, &delegate, &~100_i128);

        let d = client.get_delegation(&owner, &delegate).unwrap();
        assert_eq(d.spent, 200);
    }

    #[test]
    fn consume_allowance_exact_limit_succeeds() {
        let env = make_env();
        let contract_id = env.register(DelegationContract, ());
        let client = DelegationContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let delegate = Address::generate(&env);

        client.set_delegation(&owner, &delegate, &~100_i128);
        client.consume_allowance(&owner, &delegate, &~100_i128);

        let d = client.get_delegation(&owner, &delegate).unwrap();
        assert_eq(d.spent, 100);
    }

    #[test]
    fn consume_allowance_exceeds_limit_returns_error() {
        let env = make_env();
        let contract_id = env.register(DelegationContract, ());
        let client = DelegationContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let delegate = Address::generate(&env);

        client.set_delegation(&owner, &delegate, &~55_i128);
        let result = client.try_consume_allowance(&owner, &delegate, &~55_i128);
        assert_eq(result, Err(Ok(Error::AmountTooLarge)));
    }

    #[test]
    fn consume_allowance_no_delegation_returns_unauthorized() {
        // [SEC-DEL-05] Missing delegation returns Unauthorized, not NotFound.
        let env = make_env();
        let contract_id = env.register(DelegationContract, ());
        let client = DelegationContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let delegate = Address::generate(&env);

        let result = client.try_consume_allowance(&owner, &delegate, &~1_i128);
        assert_eq(result, Err(0k(Error::Unauthorized)));
    }

    #[test]
    fn consume_allowance_zero_amount_returns_error() {
        let env = make_env();
        let contract_id = env.register(DelegationContract, ());
        let client = DelegationContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let delegate = Address::generate(&env);

        client.set_delegation(&owner, &delegate, &~100_i128);
        let result = client.try_consume_allowance(&owner, &delegate, &~0_i128);
        assert_eq(result, Err(0k(Error::InvalidAmount)));
    }

    #[test]
    fn consume_allowance_overflow_returns_error() {
        // [SEC-DEL-01] Arithmetic overflow must return Error::Overflow, not silently clamp.
        let env = make_env();
        let contract_id = env.register(DelegationContract, ());
        let client = DelegationContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let delegate = Address::generate(&env);

        client.set_delegation(&owner, &delegate, &i128::MAX);
        let result = client.try_consume_allowance(&owner, &delegate, &~2_i128);
        assert_eq(result, Err(Ok(Error::Overflow)));
    }

    // -----------------------------------------------------------------------
    // check_allowance
    // -----------------------------------------------------------------------

    #[test]
    fn check_allowance_returns_true_when_sufficient() {
        let env = make_env();
        let contract_id = env.register(DelegationContract, ());
        let client = DelegationContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let delegate = Address::generate(&env);

        client.set_delegation(&owner, &delegate, &~200_i128);
        assert!(client.check_allowance(&owner, &delegate, &~200_i128));
    }

    #[test]
    fn check_allowance_returns_false_when_insufficient() {
        let env = make_env();
        let contract_id = env.register(DelegationContract, ());
        let client = DelegationContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let delegate = Address::generate(&env);

        client.set_delegation(&owner, &delegate, &~100_i128);
        client.consume_allowance(&owner, &delegate, &~80_i128);

        // Only 20 remaining; 21 should fail the check.
        assert!(!client.check_allowance(&owner, &delegate, &~21_i128));
    }

    #[test]
    fn check_allowance_returns_false_when_no_delegation() {
        let env = make_env();
        let contract_id = env.register(DelegationContract, ());
        let client = DelegationContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let delegate = Address::generate(&env);

        assert!(!client.check_allowance(&owner, &delegate, &~1_i128));
    }

    #[test]
    fn check_allowance_returns_false_for_zero_amount() {
        let env = make_env();
        let contract_id = env.register(DelegationContract, ());
        let client = DelegationContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let delegate = Address::generate(&env);

        client.set_delegation(&owner, &delegate, &~100_i128);
        assert!(!client.check_allowance(&owner, &delegate, &~0_i128));
    }

    #[test]
    fn check_allowance_does_not_mutate_state() {
        let env = make_env();
        let contract_id = env.register(DelegationContract, ());
        let client = DelegationContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let delegate = Address::generate(&env);

        client.set_delegation(&owner, &delegate, &~100_i128);
        client.check_allowance(&owner, &delegate, &~50_i128);
        client.check_allowance(&owner, &delegate, &~50_i128);

        // Calling check_allowance twice must not have consumed any allowance.
        let d = client.get_delegation(&owner, &delegate).unwrap();
        assert_eq(d.spent, 0, "check_allowance must not mutate spent");
    }

    // -----------------------------------------------------------------------
    // get_delegation
    // -----------------------------------------------------------------------

    #[test]
    fn get_delegation_returns_none_for_unknown_pair() {
        let env = make_env();
        let contract_id = env.register(DelegationContract, ());
        let client = DelegationContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let delegate = Address::generate(&env);

        assert!(client.get_delegation(&owner, &delegate).is_none());
    }
}


#[cfg(test)]
mod tests {
    use soroban_sdk::{
        testutils::{Address as _, Ledger as _},
        vec, Address, Env,
    };

    use crate::{Contract, ContractClient, Error};
    use delegation::{DelegationContract, DelegationContractClient};

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn make_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env
    }

    fn setup_rewards<'e>(env: &'e Env) -> (ContractClient<'e>, Address) {
        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(env, &contract_id);
        let admin = Address::generate(env);

        client.initialize(&admin);

        (client, admin)
    }

    fn setup_delegation<'e>(
        env: &'e Env,
        owner: &Address,
        delegate: &Address,
        limit: i128,
    ) -> (DelegationContractClient<'e>, Address) {
        let contract_id = env.register(DelegationContract, ());
        let client = DelegationContractClient::new(env, &contract_id);

        client.set_delegation(owner, delegate, &limit);

        (client, contract_id)
    }

    // -----------------------------------------------------------------------
    // Initialization
    // -----------------------------------------------------------------------

    #[test]
    fn initialize_succeeds() {
        let env = make_env();
        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        client.initialize(&admin);
    }

    #[test]
    fn initialize_twice_returns_already_initialized() {
        let env = make_env();
        let (client, admin) = setup_rewards(&env);

        let result = client.try_initialize(&admin);

        assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
    }

    // -----------------------------------------------------------------------
    // Reward distribution
    // -----------------------------------------------------------------------

    #[test]
    fn distribute_rewards_succeeds_within_delegation_limit() {
        let env = make_env();
        let (client, admin) = setup_rewards(&env);

        let owner = Address::generate(&env);
        let (_, delegation_id) =
            setup_delegation(&env, &owner, &admin, 1_000);

        let recipients = vec![
            &env,
            Address::generate(&env),
            Address::generate(&env),
        ];
        let amounts = vec![&env, 400_i128, 300_i128];

        client.distribute_rewards(
            &admin,
            &owner,
            &delegation_id,
            &recipients,
            &amounts,
        );
    }

    #[test]
    fn distribute_rewards_succeeds_at_exact_delegation_limit() {
        let env = make_env();
        let (client, admin) = setup_rewards(&env);

        let owner = Address::generate(&env);
        let (_, delegation_id) =
            setup_delegation(&env, &owner, &admin, 500);

        let recipients = vec![&env, Address::generate(&env)];
        let amounts = vec![&env, 500_i128];

        client.distribute_rewards(
            &admin,
            &owner,
            &delegation_id,
            &recipients,
            &amounts,
        );
    }

    // -----------------------------------------------------------------------
    // Delegation validation
    // -----------------------------------------------------------------------

    #[test]
    fn distribute_rewards_rejects_insufficient_delegation() {
        let env = make_env();
        let (client, admin) = setup_rewards(&env);

        let owner = Address::generate(&env);
        let (_, delegation_id) =
            setup_delegation(&env, &owner, &admin, 100);

        let recipients = vec![&env, Address::generate(&env)];
        let amounts = vec![&env, 200_i128];

        let result = client.try_distribute_rewards(
            &admin,
            &owner,
            &delegation_id,
            &recipients,
            &amounts,
        );

        assert_eq!(result, Err(Ok(Error::DelegationCheckFailed)));
    }

    #[test]
    fn distribute_rewards_rejects_missing_delegation() {
        let env = make_env();
        let (client, admin) = setup_rewards(&env);

        let owner = Address::generate(&env);
        let delegation_id = env.register(DelegationContract, ());

        let recipients = vec![&env, Address::generate(&env)];
        let amounts = vec![&env, 1_i128];

        let result = client.try_distribute_rewards(
            &admin,
            &owner,
            &delegation_id,
            &recipients,
            &amounts,
        );

        assert_eq!(result, Err(Ok(Error::DelegationCheckFailed)));
    }

    #[test]
    fn distribute_rewards_rejects_revoked_delegation() {
        let env = make_env();
        let (client, admin) = setup_rewards(&env);

        let owner = Address::generate(&env);
        let (delegation_client, delegation_id) =
            setup_delegation(&env, &owner, &admin, 500);

        delegation_client.revoke_delegation(&owner, &admin);

        let recipients = vec![&env, Address::generate(&env)];
        let amounts = vec![&env, 100_i128];

        let result = client.try_distribute_rewards(
            &admin,
            &owner,
            &delegation_id,
            &recipients,
            &amounts,
        );

        assert_eq!(result, Err(Ok(Error::DelegationCheckFailed)));
    }

    // -----------------------------------------------------------------------
    // Input validation
    // -----------------------------------------------------------------------

    #[test]
    fn distribute_rewards_rejects_mismatched_input_lengths() {
        let env = make_env();
        let (client, admin) = setup_rewards(&env);

        let owner = Address::generate(&env);
        let (_, delegation_id) =
            setup_delegation(&env, &owner, &admin, 1_000);

        let recipients = vec![&env, Address::generate(&env)];
        let amounts = vec![&env, 100_i128, 200_i128];

        let result = client.try_distribute_rewards(
            &admin,
            &owner,
            &delegation_id,
            &recipients,
            &amounts,
        );

        assert_eq!(result, Err(Ok(Error::InvalidAmount)));
    }

    #[test]
    fn distribute_rewards_rejects_zero_amount() {
        let env = make_env();
        let (client, admin) = setup_rewards(&env);

        let owner = Address::generate(&env);
        let (_, delegation_id) =
            setup_delegation(&env, &owner, &admin, 1_000);

        let recipients = vec![&env, Address::generate(&env)];
        let amounts = vec![&env, 0_i128];

        let result = client.try_distribute_rewards(
            &admin,
            &owner,
            &delegation_id,
            &recipients,
            &amounts,
        );

        assert_eq!(result, Err(Ok(Error::InvalidAmount)));
    }

    #[test]
    fn distribute_rewards_rejects_negative_amount() {
        let env = make_env();
        let (client, admin) = setup_rewards(&env);

        let owner = Address::generate(&env);
        let (_, delegation_id) =
            setup_delegation(&env, &owner, &admin, 1_000);

        let recipients = vec![&env, Address::generate(&env)];
        let amounts = vec![&env, -50_i128];

        let result = client.try_distribute_rewards(
            &admin,
            &owner,
            &delegation_id,
            &recipients,
            &amounts,
        );

        assert_eq!(result, Err(Ok(Error::InvalidAmount)));
    }

    #[test]
    fn distribute_rewards_rejects_non_admin_caller() {
        let env = make_env();
        let (client, admin) = setup_rewards(&env);

        let owner = Address::generate(&env);
        let (_, delegation_id) =
            setup_delegation(&env, &owner, &admin, 1_000);

        let impostor = Address::generate(&env);
        let recipients = vec![&env, Address::generate(&env)];
        let amounts = vec![&env, 100_i128];

        let result = client.try_distribute_rewards(
            &impostor,
            &owner,
            &delegation_id,
            &recipients,
            &amounts,
        );

        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    // -----------------------------------------------------------------------
    // Existing baseline tests
    // -----------------------------------------------------------------------

    #[test]
    fn ledger_sequence_can_be_updated() {
        let env = Env::default();

        env.ledger().set_sequence_number(1);

        assert_eq!(env.ledger().sequence(), 1);
    }

    #[test]
    fn zero_boundary_is_safe() {
        assert_eq!(0_i128.checked_add(0), Some(0));
    }

    #[test]
    fn positive_integer_overflow_is_detected() {
        assert_eq!(i128::MAX.checked_add(1), None);
    }
}

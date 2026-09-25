#[cfg(test)]
mod tests {
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{vec, Address, Env};

    use crate::{Contract, ContractClient, Error};

    fn make_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env
    }

    fn setup_contract(env: &Env) -> (ContractClient<'_>, Address) {
        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        // client.initialize() returns () on success and panics on error.
        client.initialize(&admin);
        (client, admin)
    }

    // -----------------------------------------------------------------------
    // initialize
    // -----------------------------------------------------------------------

    #[test]
    fn initialize_succeeds() {
        let env = make_env();
        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        // Soroban client strips Result — returns () on success, panics on error.
        client.initialize(&admin);
    }

    #[test]
    fn initialize_twice_returns_error() {
        let env = make_env();
        let (client, admin) = setup_contract(&env);
        let result = client.try_initialize(&admin);
        assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
    }

    // -----------------------------------------------------------------------
    // set_signers / get_signers / get_threshold
    // -----------------------------------------------------------------------

    #[test]
    fn set_signers_stores_list_and_threshold() {
        let env = make_env();
        let (client, admin) = setup_contract(&env);

        let s1 = Address::generate(&env);
        let s2 = Address::generate(&env);
        let s3 = Address::generate(&env);
        let signers = vec![&env, s1.clone(), s2.clone(), s3.clone()];

        // set_signers returns () on success and panics on error.
        client.set_signers(&admin, &signers, &2u32);

        assert_eq!(client.get_threshold(), 2);
        let stored = client.get_signers();
        assert_eq!(stored.len(), 3);
    }

    #[test]
    fn set_signers_rejects_threshold_exceeding_signer_count() {
        let env = make_env();
        let (client, admin) = setup_contract(&env);

        let s1 = Address::generate(&env);
        let signers = vec![&env, s1.clone()];

        let result = client.try_set_signers(&admin, &signers, &2u32);
        assert_eq!(result, Err(Ok(Error::InvalidThreshold)));
    }

    #[test]
    fn set_signers_rejects_duplicate_signers() {
        let env = make_env();
        let (client, admin) = setup_contract(&env);

        let s1 = Address::generate(&env);
        let signers = vec![&env, s1.clone(), s1.clone()];

        let result = client.try_set_signers(&admin, &signers, &1u32);
        assert_eq!(result, Err(Ok(Error::DuplicateSigner)));
    }

    #[test]
    fn set_signers_rejects_empty_list() {
        let env = make_env();
        let (client, admin) = setup_contract(&env);

        let signers = vec![&env];
        let result = client.try_set_signers(&admin, &signers, &1u32);
        assert_eq!(result, Err(Ok(Error::InvalidThreshold)));
    }

    // -----------------------------------------------------------------------
    // set_high_value_threshold / get_high_value_threshold
    // -----------------------------------------------------------------------

    #[test]
    fn set_high_value_threshold_stores_value() {
        let env = make_env();
        let (client, admin) = setup_contract(&env);

        // set_high_value_threshold returns () on success and panics on error.
        client.set_high_value_threshold(&admin, &1_000_000_i128);
        assert_eq!(client.get_high_value_threshold(), 1_000_000_i128);
    }

    #[test]
    fn set_high_value_threshold_rejects_negative() {
        let env = make_env();
        let (client, admin) = setup_contract(&env);

        let result = client.try_set_high_value_threshold(&admin, &-1_i128);
        assert_eq!(result, Err(Ok(Error::InvalidAmount)));
    }

    // -----------------------------------------------------------------------
    // is_signer
    // -----------------------------------------------------------------------

    #[test]
    fn is_signer_returns_true_for_configured_signer() {
        let env = make_env();
        let (client, admin) = setup_contract(&env);

        let s1 = Address::generate(&env);
        let signers = vec![&env, s1.clone()];
        client.set_signers(&admin, &signers, &1u32);

        assert!(client.is_signer(&s1));
    }

    #[test]
    fn is_signer_returns_false_for_unknown_address() {
        let env = make_env();
        let (client, admin) = setup_contract(&env);

        let s1 = Address::generate(&env);
        let signers = vec![&env, s1.clone()];
        client.set_signers(&admin, &signers, &1u32);

        let stranger = Address::generate(&env);
        assert!(!client.is_signer(&stranger));
    }

    // -----------------------------------------------------------------------
    // get_approval_count
    // -----------------------------------------------------------------------

    #[test]
    fn get_approval_count_returns_zero_for_unknown_tx() {
        let env = make_env();
        let (client, _) = setup_contract(&env);
        assert_eq!(client.get_approval_count(&99u64), 0u32);
    }

    #[test]
    fn recording_an_approval_increments_approval_count() {
        let env = make_env();
        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let tx_id = 1u64;
        assert_eq!(client.get_approval_count(&tx_id), 0u32);

        env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .set(&crate::DataKey::ApprovalCount(tx_id), &1u32);
        });

        assert_eq!(client.get_approval_count(&tx_id), 1u32);
    }
}

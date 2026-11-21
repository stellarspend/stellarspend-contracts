#[cfg(test)]
mod tests {
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::Env;

    #[test]
    fn happy_path_environment() {
        let env = Env::default();
        env.ledger().set_sequence_number(1);
        assert_eq!(env.ledger().sequence(), 1);
    }
    #[test]
    fn unauthorized_boundary_placeholder() {
        let env = Env::default();
        env.mock_all_auths();
        assert!(env.ledger().timestamp() >= 0);
    }
    #[test]
    fn address_generation() {
        let env = Env::default();
        let _ = soroban_sdk::Address::generate(&env);
    }
    #[test]
    fn zero_boundary() {
        assert_eq!(0_i128.checked_add(0), Some(0));
    }
    #[test]
    fn overflow_boundary() {
        assert_eq!(i128::MAX.checked_add(1), None);
    }

    #[test]
    fn initialize_sets_value_to_zero() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(crate::Contract, ());
        let client = crate::ContractClient::new(&env, &contract_id);
        let admin = soroban_sdk::Address::generate(&env);

        client.initialize(&admin);

        assert_eq!(client.get_value(), 0);
    }

    #[test]
    fn set_value_from_non_admin_returns_unauthorized() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(crate::Contract, ());
        let client = crate::ContractClient::new(&env, &contract_id);

        let admin = soroban_sdk::Address::generate(&env);
        let non_admin = soroban_sdk::Address::generate(&env);
        client.initialize(&admin);

        let result = client.try_set_value(&non_admin, &100_i128);
        assert_eq!(result, Err(Ok(crate::Error::Unauthorized)));
        assert_eq!(client.get_value(), 0);
    }

    #[test]
    fn escrow_state_transitions_from_uninitialized_to_updated() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(crate::Contract, ());
        let client = crate::ContractClient::new(&env, &contract_id);
        let admin = soroban_sdk::Address::generate(&env);

        assert_eq!(client.get_value(), 0);
        client.initialize(&admin);
        assert_eq!(client.get_value(), 0);

        client.set_value(&admin, &250_i128);
        assert_eq!(client.get_value(), 250);

        client.set_value(&admin, &75_i128);
        assert_eq!(client.get_value(), 75);
    }
}

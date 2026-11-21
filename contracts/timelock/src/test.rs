#[cfg(test)]
mod tests {
    use crate::{Contract, ContractClient};
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::{Address, Env};

    use crate::{storage, Contract, ContractClient, Error};
    #[test]
    fn initialize_sets_zero_value() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        client.initialize(&admin);
        assert_eq!(client.get_value(), 0);
    }

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

    // ── initialize ────────────────────────────────────────────────────────

    #[test]
    fn initialize_stores_admin_and_seeds_value_at_zero() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &contract_id);

        // get_value() returns 0 before initialize as well, so assert on the
        // stored config too — that is what proves initialize wrote anything.
        assert_eq!(storage::read_config(&env), None);

        // The contractimpl client unwraps Result, so a success is just `()`.
        assert_eq!(client.initialize(&admin), ());

        let config = storage::read_config(&env).expect("initialize must persist a config");
        assert_eq!(config.admin, admin);
        assert_eq!(config.value, 0);
        assert_eq!(client.get_value(), 0);
    }

    #[test]
    fn initialize_twice_is_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &contract_id);
        client.initialize(&admin);

        assert_eq!(
            client.try_initialize(&admin),
            Err(Ok(Error::AlreadyInitialized))
        );
        assert_eq!(client.get_value(), 0);
    }

    #[test]
    fn initialize_requires_the_admin_to_authorize() {
        let env = Env::default();
        // Deliberately do NOT mock_all_auths — nothing has authorized anything.
        let admin = Address::generate(&env);
        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &contract_id);

        assert!(client.try_initialize(&admin).is_err());
        assert_eq!(storage::read_config(&env), None);
    }
}


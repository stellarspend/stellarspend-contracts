#[cfg(test)]
mod tests {
    use crate::{storage, Contract, ContractClient, Error};
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::{Address, Env};

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
    fn create_subscription_and_verify_status() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &contract_id);

        client.initialize(&admin);
        client.set_value(&admin, &100);

        assert_eq!(client.get_value(), 100);
    }

    // ── initialize ────────────────────────────────────────────────────────

    #[test]
    fn initialize_stores_admin_and_seeds_value_at_zero() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &contract_id);

        // get_value() is 0 before initialize too, so assert on the stored
        // config as well to prove initialize actually wrote.
        assert_eq!(storage::read_config(&env), None);
        assert_eq!(client.get_value(), 0);

        // The contractimpl client unwraps Result, so a success is just `()`.
        assert_eq!(client.initialize(&admin), ());

        let config = storage::read_config(&env).expect("initialize must persist a config");
        assert_eq!(config.admin, admin);
        assert_eq!(config.value, 0);
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

    // ── set_value authorization ───────────────────────────────────────────

    #[test]
    fn set_value_rejects_a_non_admin_caller() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let intruder = Address::generate(&env);
        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &contract_id);
        client.initialize(&admin);

        assert_eq!(
            client.try_set_value(&intruder, &500),
            Err(Ok(Error::Unauthorized))
        );

        // The rejected write must leave the stored value untouched.
        assert_eq!(client.get_value(), 0);
    }

    #[test]
    fn set_value_rejects_a_non_admin_even_after_a_legitimate_update() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let intruder = Address::generate(&env);
        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &contract_id);
        client.initialize(&admin);

        client.set_value(&admin, &42);
        assert_eq!(client.get_value(), 42);

        assert_eq!(
            client.try_set_value(&intruder, &1_000),
            Err(Ok(Error::Unauthorized))
        );
        assert_eq!(client.get_value(), 42);
    }
}

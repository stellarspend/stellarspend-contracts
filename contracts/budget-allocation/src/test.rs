
#[cfg(test)]
mod tests {
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Address, Env,
    };

    use crate::{Contract, ContractClient};

    fn setup() -> (Env, Address, ContractClient<'static>) {
        let env = Env::default();
        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        (env, admin, client)
    }

    #[test]
    fn environment_starts_with_valid_ledger_sequence() {
        let env = Env::default();

        env.ledger().set_sequence_number(1);

        assert_eq!(env.ledger().sequence(), 1);
    }

    #[test]
    fn generated_addresses_are_distinct() {
        let env = Env::default();

        let first = Address::generate(&env);
        let second = Address::generate(&env);

        assert_ne!(first, second);
    }

    #[test]
    fn initialize_sets_initial_value_to_zero() {
        let (env, admin, client) = setup();

        env.mock_all_auths();

        client.initialize(&admin);

        assert_eq!(client.get_value(), 0);
    }

    #[test]
    fn initialized_value_remains_zero_on_repeated_reads() {
        let (env, admin, client) = setup();

        env.mock_all_auths();

        client.initialize(&admin);

        assert_eq!(client.get_value(), 0);
        assert_eq!(client.get_value(), 0);
        assert_eq!(client.get_value(), 0);
    }

    #[test]
    fn checked_add_handles_zero_boundary() {
        assert_eq!(0_i128.checked_add(0), Some(0));
    }

    #[test]
    fn checked_add_rejects_positive_overflow() {
        assert_eq!(i128::MAX.checked_add(1), None);
    }

    #[test]
    fn checked_add_rejects_negative_overflow() {
        assert_eq!(i128::MIN.checked_add(-1), None);
    }
}

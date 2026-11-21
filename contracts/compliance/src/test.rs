
#[cfg(test)]
mod tests {
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Address, Env,
    };

    #[test]
    fn ledger_sequence_can_be_updated() {
        let env = Env::default();
        let ledger = env.ledger();

        assert_eq!(ledger.sequence(), 0);

        ledger.set_sequence_number(1);

        assert_eq!(ledger.sequence(), 1);
    }

    #[test]
    fn mocked_authentication_preserves_environment_functionality() {
        let env = Env::default();

        env.mock_all_auths();
        env.ledger().set_sequence_number(42);

        assert_eq!(env.ledger().sequence(), 42);
    }

    #[test]
    fn generated_addresses_are_unique() {
        let env = Env::default();

        let first_address = Address::generate(&env);
        let second_address = Address::generate(&env);

        assert_ne!(first_address, second_address);
    }

    #[test]
    fn zero_arithmetic_returns_zero() {
        assert_eq!(0_i128.checked_add(0), Some(0));
        assert_eq!(0_i128.checked_sub(0), Some(0));
        assert_eq!(0_i128.checked_mul(0), Some(0));
    }

    #[test]
    fn positive_integer_overflow_is_detected() {
        assert_eq!(i128::MAX.checked_add(1), None);
        assert_eq!(i128::MAX.checked_mul(2), None);
    }

    #[test]
    fn negative_integer_overflow_is_detected() {
        assert_eq!(i128::MIN.checked_sub(1), None);
        assert_eq!(i128::MIN.checked_mul(2), None);
    }

    #[test]
    fn integer_boundaries_are_preserved() {
        assert_eq!(i128::MAX.checked_add(0), Some(i128::MAX));
        assert_eq!(i128::MIN.checked_sub(0), Some(i128::MIN));
    }

    #[test]
    fn checked_arithmetic_handles_boundary_values() {
        assert_eq!(i128::MAX.checked_sub(i128::MAX), Some(0));
        assert_eq!(i128::MIN.checked_add(i128::MAX), Some(-1));
        assert_eq!(i128::MIN.checked_mul(1), Some(i128::MIN));
        assert_eq!(i128::MAX.checked_mul(1), Some(i128::MAX));
    }

    #[test]
    fn checked_arithmetic_detects_underflow_and_overflow() {
        assert_eq!(i128::MIN.checked_add(-1), None);
        assert_eq!(i128::MAX.checked_sub(-1), None);
        assert_eq!(i128::MIN.checked_mul(-1), None);
    }

    #[test]
    fn set_value_from_non_admin_returns_unauthorized() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(crate::Contract, ());
        let client = crate::ContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let non_admin = Address::generate(&env);
        client.initialize(&admin);

        let result = client.try_set_value(&non_admin, &100_i128);
        assert_eq!(result, Err(Ok(crate::Error::Unauthorized)));
        assert_eq!(client.get_value(), 0);
    }
}

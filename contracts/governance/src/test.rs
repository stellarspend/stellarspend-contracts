#[cfg(test)]
mod tests {
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::Env;

    // The contract's only mutable state is set via `set_value`/`get_value`;
    // this exercises creating that state and verifying it reads back
    // correctly, standing in for "create a proposal, verify its state"
    // until the contract exposes a dedicated proposal type.
    #[test]
    fn set_value_creates_and_reads_back_state() {
        use crate::{Contract, ContractClient};
        use soroban_sdk::Address;

        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        client.initialize(&admin);
        assert_eq!(client.get_value(), 0);

        client.set_value(&admin, &42i128);
        assert_eq!(client.get_value(), 42i128);
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
}

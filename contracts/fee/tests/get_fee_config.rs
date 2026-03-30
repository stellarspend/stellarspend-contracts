#[cfg(test)]
mod test {
    use soroban_sdk::{Env, Address, testutils::*};
    use soroban_sdk::testutils::Address as _;

    use fee::{FeeContract, FeeContractClient};

    #[test]
    fn test_get_fee_config_default() {
        let env = Env::default();
        let contract_id = env.register(FeeContract, ());
        let client = FeeContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let token = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.initialize(&admin, &token, &treasury, &100u32, &1u64);

        let base_fee = client.get_fee_bps();
        let min_fee = client.get_min_fee();

        assert_eq!(base_fee, 100);
        assert_eq!(min_fee, 0);
    }
}
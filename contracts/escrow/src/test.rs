#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::Env;

    #[test]
    fn test_get_escrow_balance_non_existent() {
        let env = Env::default();
        let contract_id = env.register(EscrowContract, ());
        let client = EscrowContractClient::new(&env, &contract_id);

        let balance = client.get_escrow_balance(&999);
        assert_eq!(balance, 0);
    }
}

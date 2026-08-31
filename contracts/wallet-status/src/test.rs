use crate::{WalletStatusContract, WalletStatusContractClient};
use soroban_sdk::{symbol_short, Address, Env, Map, Symbol};

#[test]
fn test_statuses() {
    let env = Env::default();
    let contract_id = env.register_contract(None, WalletStatusContract);
    let client = WalletStatusContractClient::new(&env, &contract_id);

    let active_owner = Address::random(&env);
    let frozen_owner = Address::random(&env);

    let unset_owner = Address::random(&env);

    env.as_contract(&contract_id, || {
        let key = symbol_short!("status");
        let mut statuses: Map<Address, Symbol> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Map::new(&env));
        statuses.set(active_owner.clone(), symbol_short!("active"));
        statuses.set(frozen_owner.clone(), symbol_short!("frozen"));
        env.storage().persistent().set(&key, &statuses);
    });

    assert_eq!(client.get_wallet_status(&active_owner), symbol_short!("active"));
    assert_eq!(client.get_wallet_status(&frozen_owner), symbol_short!("frozen"));
}
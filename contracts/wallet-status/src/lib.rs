#[no_std]
use soroban_sdk{contract, contractimpl, Address, Env, Map, Symbol, symbol_short};

[contract]
pub struct WalletStatusContract;

[contractimpl]
impl WalletStatusContract {
    pub fn get_wallet_status(env: Env, owner: Address) -> Symbol {
        let key = symbol_short!("status");
        let statuses: Map<Address, Symbol> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Map::new(&env));
        statuses
            .get(owner)
            .cloned()
            .unwrap_or_else(|| symbol_short!("active"))
    }
}

#[cfg(test)]
mod test;

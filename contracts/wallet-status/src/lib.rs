#![no_std]

//! Tracks per-wallet status (e.g. active/frozen) in persistent storage,
//! defaulting to "active" for any address with no recorded status.

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, Map, Symbol};

#[contract]
pub struct WalletStatusContract;

#[contractimpl]
impl WalletStatusContract {
    /// Returns the stored status for `owner`, or `"active"` if no status
    /// has been recorded for that address yet.
    pub fn get_wallet_status(env: Env, owner: Address) -> Symbol {
        let key = symbol_short!("status");
        let statuses: Map<Address, Symbol> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Map::new(&env));
        statuses
            .get(owner)
            .unwrap_or_else(|| symbol_short!("active"))
    }
}

#[cfg(test)]
mod test;

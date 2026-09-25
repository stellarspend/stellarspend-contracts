#![no_std]

//! Wallet linking status view. Loose crate-free module (matches the
//! codebase's convention of not-yet-wired contract files) exposing
//! get_wallet_link_status per issue #1064.

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, Map, Symbol};

#[contract]
pub struct WalletLinkingContract;

#[contractimpl]
impl WalletLinkingContract {
    /// Returns the current link status for `owner`: "linked", "pending",
    /// or "unlinked". Returns "unlinked" as the documented default for
    /// an address with no recorded link status.
    pub fn get_wallet_link_status(env: Env, owner: Address) -> Symbol {
        let key = symbol_short!("wlstatus");
        let statuses: Map<Address, Symbol> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Map::new(&env));
        statuses
            .get(owner)
            .unwrap_or_else(|| symbol_short!("unlinked"))
    }
}

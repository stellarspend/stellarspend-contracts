#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, Address,
    Env, Map,
};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Blacklist,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum AssetControlError {
    NotInitialized = 1,
    Unauthorized = 2,
    AlreadyBlacklisted = 3,
    NotBlacklisted = 4,
    AlreadyInitialized = 5,
}

#[contract]
pub struct AssetControlContract;

#[contractimpl]
impl AssetControlContract {
    /// Initializes the contract with an admin address.
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(env, AssetControlError::AlreadyInitialized);
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::Blacklist, &Map::<Address, bool>::new(&env));
    }

    /// Adds an asset to the blacklist.
    pub fn add_to_blacklist(env: Env, asset: Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        let mut blacklist: Map<Address, bool> = env
            .storage()
            .instance()
            .get(&DataKey::Blacklist)
            .unwrap_or(Map::new(&env));

        if blacklist.contains_key(asset.clone()) {
            panic_with_error!(env, AssetControlError::AlreadyBlacklisted);
        }

        blacklist.set(asset.clone(), true);
        env.storage()
            .instance()
            .set(&DataKey::Blacklist, &blacklist);

        env.events()
            .publish((symbol_short!("asset"), symbol_short!("blacklist")), asset);
    }

    /// Removes an asset from the blacklist.
    pub fn remove_from_blacklist(env: Env, asset: Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        let mut blacklist: Map<Address, bool> = env
            .storage()
            .instance()
            .get(&DataKey::Blacklist)
            .unwrap_or(Map::new(&env));

        if !blacklist.contains_key(asset.clone()) {
            panic_with_error!(env, AssetControlError::NotBlacklisted);
        }

        blacklist.remove(asset.clone());
        env.storage()
            .instance()
            .set(&DataKey::Blacklist, &blacklist);

<<<<<<< HEAD
        env.events()
            .publish((symbol_short!("asset"), symbol_short!("unblack")), asset);
=======
        env.events().publish(
            (symbol_short!("asset"), symbol_short!("unblklist")),
            asset,
        );
>>>>>>> 067107d (fix(contracts): fix CI compilation errors across batch-transfer, spending-limits, multi-currency-wallet, and batch-rewards)
    }

    /// Checks if an asset is blacklisted.
    pub fn is_blacklisted(env: Env, asset: Address) -> bool {
        let blacklist: Map<Address, bool> = env
            .storage()
            .instance()
            .get(&DataKey::Blacklist)
            .unwrap_or(Map::new(&env));

        blacklist.get(asset).unwrap_or(false)
    }

    /// Checks an asset and emits an event if blacklisted, then panics.
    pub fn check_asset(env: Env, asset: Address) {
        if Self::is_blacklisted(env.clone(), asset.clone()) {
            env.events()
                .publish((symbol_short!("asset"), symbol_short!("blocked")), asset);
            panic_with_error!(env, AssetControlError::Unauthorized);
        }
    }
}

#[cfg(test)]
mod test;

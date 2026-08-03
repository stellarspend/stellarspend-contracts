#![no_std]

mod validation;

use shared::errors::SharedError;
use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, symbol_short, Address, Env, Symbol,
};

use crate::validation::validate_nickname;

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    Profile(Address),
    TotalProfiles,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct WalletProfile {
    pub user: Address,
    pub nickname: Symbol,
    pub created_at: u64,
    pub updated_at: u64,
    pub is_active: bool,
}

pub struct ProfileEvents;

impl ProfileEvents {
    pub fn profile_created(env: &Env, profile: &WalletProfile) {
        let topics = (symbol_short!("profile"), symbol_short!("created"));
        env.events()
            .publish(topics, (profile.user.clone(), profile.nickname.clone()));
    }

    pub fn nickname_updated(env: &Env, user: &Address, old_nickname: Symbol, new_nickname: Symbol) {
        let topics = (symbol_short!("profile"), symbol_short!("nickname"));
        env.events()
            .publish(topics, (user.clone(), old_nickname, new_nickname));
    }
}

#[contract]
pub struct WalletProfileContract;

#[contractimpl]
impl WalletProfileContract {
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&env, SharedError::AlreadyInitialized);
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::TotalProfiles, &0u64);
    }

    pub fn create_profile(env: Env, user: Address, nickname: Symbol) -> WalletProfile {
        user.require_auth();

        if validate_nickname(&nickname).is_err() {
            panic_with_error!(&env, SharedError::InvalidInput);
        }

        if env
            .storage()
            .persistent()
            .has(&DataKey::Profile(user.clone()))
        {
            panic_with_error!(&env, SharedError::ResourceAlreadyExists);
        }

        let current_ledger = env.ledger().sequence() as u64;

        let profile = WalletProfile {
            user: user.clone(),
            nickname: nickname.clone(),
            created_at: current_ledger,
            updated_at: current_ledger,
            is_active: true,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Profile(user.clone()), &profile);

        let total: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalProfiles)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::TotalProfiles, &(total + 1));

        ProfileEvents::profile_created(&env, &profile);

        profile
    }

    pub fn update_nickname(env: Env, user: Address, new_nickname: Symbol) -> WalletProfile {
        user.require_auth();

        if validate_nickname(&new_nickname).is_err() {
            panic_with_error!(&env, SharedError::InvalidInput);
        }

        let mut profile: WalletProfile = env
            .storage()
            .persistent()
            .get(&DataKey::Profile(user.clone()))
            .unwrap_or_else(|| panic_with_error!(&env, SharedError::ResourceNotFound));

        let old_nickname = profile.nickname.clone();

        profile.nickname = new_nickname.clone();
        profile.updated_at = env.ledger().sequence() as u64;

        env.storage()
            .persistent()
            .set(&DataKey::Profile(user.clone()), &profile);

        ProfileEvents::nickname_updated(&env, &user, old_nickname, new_nickname);

        profile
    }

    pub fn get_profile(env: Env, user: Address) -> Option<WalletProfile> {
        env.storage().persistent().get(&DataKey::Profile(user))
    }

    pub fn get_wallet_profile(env: Env, owner: Address) -> Option<WalletProfile> {
        env.storage().persistent().get(&DataKey::Profile(owner))
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Contract not initialized")
    }

    pub fn set_admin(env: Env, current_admin: Address, new_admin: Address) {
        current_admin.require_auth();
        Self::require_admin(&env, &current_admin);

        env.storage().instance().set(&DataKey::Admin, &new_admin);
    }

    pub fn get_total_profiles(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalProfiles)
            .unwrap_or(0)
    }

    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Contract not initialized");

        if *caller != admin {
            panic_with_error!(env, SharedError::Unauthorized);
        }
    }
}

#[cfg(test)]
mod test;

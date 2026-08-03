#![no_std]

use shared::errors::SharedError;
use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, symbol_short, Address, Env, String, Vec,
};

mod storage;

pub use storage::{
    add_user, deactivate_user as storage_deactivate_user, get_all_users, get_default_currency,
    get_user_active_status, get_user_count as storage_get_user_count, get_user_currency,
    get_user_last_login, get_user_nickname, get_user_settings, is_user_active, reset_user_data,
    set_default_currency, set_user_active_status, set_user_currency, set_user_last_login,
    set_user_nickname, user_exists, UserSettings,
};

#[cfg(test)]
mod test;

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
}

#[contract]
pub struct UsersContract;

#[contractimpl]
impl UsersContract {
    /// Initialize the users contract with an admin address.
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&env, SharedError::AlreadyInitialized);
        }

        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);

        env.events()
            .publish((symbol_short!("users"), symbol_short!("init")), admin);
    }

    /// Register a new user.
    ///
    /// Issue: "Track when user joined" - the current ledger timestamp is
    /// recorded via `set_user_last_login` immediately on registration and
    /// can be read back with `get_user_last_login`.
    ///
    /// Issue: "Emit event when user registers" - a `("users", "reg")` event
    /// carrying the registrant's address is published on every successful
    /// registration.
    ///
    /// Returns `true` when the user was newly registered, `false` if they
    /// were already present (idempotent, no event emitted for duplicates).
    pub fn register_user(env: Env, user: Address) -> bool {
        if user_exists(&env, user.clone()) {
            return false;
        }

        let is_new = add_user(&env, user.clone());

        if is_new {
            set_user_last_login(&env, user.clone(), env.ledger().timestamp());
            env.events()
                .publish((symbol_short!("users"), symbol_short!("reg")), user);
        }

        is_new
    }

    /// Return the total count of registered users.
    pub fn get_user_count(env: Env) -> u64 {
        storage_get_user_count(&env)
    }

    /// Return the total count of registered users as u32.
    pub fn get_users_count(env: Env) -> u32 {
        storage_get_user_count(&env) as u32
    }

    /// Return the total count of registered users.
    pub fn get_all_users_count(env: Env) -> u64 {
        storage_get_user_count(&env)
    }

    /// Return `true` if the given address has been registered.
    pub fn is_user_registered(env: Env, user: Address) -> bool {
        user_exists(&env, user)
    }

    /// Alias for `is_user_registered`; satisfies the `check_user_exists` API
    /// surface requested in issue #336.
    pub fn check_user_exists(env: Env, user: Address) -> bool {
        user_exists(&env, user)
    }

    /// Return all registered user addresses (admin only).
    pub fn get_all_users(env: Env, caller: Address) -> Vec<Address> {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        get_all_users(&env)
    }

    /// Reset the calling user's profile data.
    pub fn reset_user_data(env: Env, user: Address) -> bool {
        user.require_auth();

        let success = reset_user_data(&env, user.clone());

        if success {
            env.events()
                .publish((symbol_short!("users"), symbol_short!("reset")), user);
        }

        success
    }

    /// Set default currency preference for a registered user.
    pub fn set_default_currency(env: Env, user: Address, currency: String) {
        user.require_auth();

        if !user_exists(&env, user.clone()) {
            panic_with_error!(&env, SharedError::ResourceNotFound);
        }

        set_default_currency(&env, user.clone(), currency.clone());

        env.events().publish(
            (symbol_short!("users"), symbol_short!("def_curr")),
            (user, currency),
        );
    }

    /// Get default currency preference for a user.
    pub fn get_default_currency(env: Env, user: Address) -> Option<String> {
        get_default_currency(&env, user)
    }

    /// Deactivate a registered user account (only the user may call).
    pub fn deactivate_user(env: Env, user: Address) -> bool {
        user.require_auth();

        let success = storage_deactivate_user(&env, user.clone());

        if success {
            env.events()
                .publish((symbol_short!("users"), symbol_short!("deact")), user);
        }

        success
    }

    /// Return whether the given user account is active.
    pub fn is_user_active(env: Env, user: Address) -> bool {
        is_user_active(&env, user)
    }

    /// Update user profile data (currency, status, etc.)
    pub fn update_user_profile(
        env: Env,
        user: Address,
        new_currency: Option<String>,
        is_active: Option<bool>,
    ) -> bool {
        user.require_auth();

        if !user_exists(&env, user.clone()) {
            panic_with_error!(&env, SharedError::ResourceNotFound);
        }

        let mut updated = false;

        if let Some(currency) = new_currency {
            set_user_currency(&env, user.clone(), currency.clone());
            set_default_currency(&env, user.clone(), currency);
            updated = true;
        }

        if let Some(active) = is_active {
            set_user_active_status(&env, user.clone(), active);
            updated = true;
        }

        if updated {
            env.events()
                .publish((symbol_short!("users"), symbol_short!("profile")), user);
        }

        updated
    }

    /// Get the admin address
    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::Admin)
    }

    /// Return the activity status for the given user.
    pub fn get_user_active_status(env: Env, user: Address) -> bool {
        get_user_active_status(&env, user)
    }

    /// Set the activity status for a user (admin only).
    pub fn set_user_active_status(
        env: Env,
        caller: Address,
        user: Address,
        is_active: bool,
    ) -> bool {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let success = set_user_active_status(&env, user.clone(), is_active);

        if success {
            env.events().publish(
                (symbol_short!("users"), symbol_short!("actv_upd")),
                (user, is_active),
            );
        }

        success
    }

    /// Return the user's preferred currency string.
    pub fn get_user_currency(env: Env, user: Address) -> String {
        get_user_currency(&env, user)
    }

    /// Set the calling user's preferred currency.
    pub fn set_user_currency(env: Env, user: Address, currency: String) -> bool {
        user.require_auth();

        let success = set_user_currency(&env, user.clone(), currency.clone());

        if success {
            env.events().publish(
                (symbol_short!("users"), symbol_short!("curr_upd")),
                (user, currency),
            );
        }

        success
    }

    /// Return the ledger timestamp recorded when the user last logged in (or
    /// registered, whichever is more recent).
    pub fn get_user_last_login(env: Env, user: Address) -> Option<u64> {
        get_user_last_login(&env, user)
    }

    /// Get the user's nickname.
    pub fn get_user_nickname(env: Env, user: Address) -> Option<String> {
        get_user_nickname(&env, user)
    }

    /// Retrieve all stored settings for a user.
    ///
    /// Returns a `UserSettings` struct with all known preference fields.
    /// If the user has no stored state (empty state), sensible defaults are
    /// returned rather than panicking.
    pub fn get_user_settings(env: Env, user: Address) -> UserSettings {
        get_user_settings(&env, user)
    }

    /// Update the calling user's nickname.
    pub fn update_nickname(env: Env, user: Address, new_nickname: String) -> bool {
        user.require_auth();

        if !user_exists(&env, user.clone()) {
            panic_with_error!(&env, SharedError::ResourceNotFound);
        }

        let success = set_user_nickname(&env, user.clone(), new_nickname.clone());

        if success {
            env.events().publish(
                (symbol_short!("users"), symbol_short!("nick_upd")),
                (user, new_nickname),
            );
        }

        success
    }

    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(env, SharedError::NotInitialized));
        if caller != &admin {
            panic_with_error!(env, SharedError::Unauthorized);
        }
    }
}

#[cfg(test)]
mod check_user_exists_tests {
    use super::{UsersContract, UsersContractClient};
    use soroban_sdk::{testutils::Address as _, Address, Env};

    fn setup<'a>() -> (Env, Address, UsersContractClient<'a>) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(UsersContract, ());
        let client = UsersContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        (env, admin, client)
    }

    #[test]
    fn returns_false_for_unregistered_user() {
        let (env, _admin, client) = setup();
        let stranger = Address::generate(&env);
        assert!(!client.check_user_exists(&stranger));
    }

    #[test]
    fn returns_true_after_registration() {
        let (env, _admin, client) = setup();
        let user = Address::generate(&env);
        assert!(!client.check_user_exists(&user));
        client.register_user(&user);
        assert!(client.check_user_exists(&user));
    }

    #[test]
    fn matches_is_user_registered_for_parity() {
        let (env, _admin, client) = setup();
        let registered = Address::generate(&env);
        let unregistered = Address::generate(&env);
        client.register_user(&registered);

        assert_eq!(
            client.check_user_exists(&registered),
            client.is_user_registered(&registered),
        );
        assert_eq!(
            client.check_user_exists(&unregistered),
            client.is_user_registered(&unregistered),
        );
    }

    #[test]
    fn registration_records_join_timestamp() {
        let (env, _admin, client) = setup();
        let user = Address::generate(&env);

        assert!(client.get_user_last_login(&user).is_none());

        client.register_user(&user);

        assert!(client.get_user_last_login(&user).is_some());
    }

    #[test]
    fn get_user_count_returns_registered_user_total() {
        let (env, _admin, client) = setup();
        let first_user = Address::generate(&env);
        let second_user = Address::generate(&env);

        assert_eq!(client.get_user_count(), 0);

        client.register_user(&first_user);
        assert_eq!(client.get_user_count(), 1);

        client.register_user(&second_user);
        assert_eq!(client.get_user_count(), 2);

        client.register_user(&first_user);
        assert_eq!(client.get_user_count(), 2);
    }
}

#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, Address, Env,
};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum AdminError {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
}

#[contracttype]
#[derive(Clone, Debug)]
pub enum AdminDataKey {
    Admin,
}

fn bump_instance(env: &Env) {
    env.storage().instance().extend_ttl(5000, 5000);
}

fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&AdminDataKey::Admin, admin);
    bump_instance(env);
}

fn get_admin(env: &Env) -> Address {
    let admin = env
        .storage()
        .instance()
        .get(&AdminDataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(env, AdminError::NotInitialized));
    bump_instance(env);
    admin
}

fn is_initialized(env: &Env) -> bool {
    env.storage().instance().has(&AdminDataKey::Admin)
}

pub fn require_admin(env: &Env, caller: &Address) {
    let admin = get_admin(env);
    if &admin != caller {
        panic_with_error!(env, AdminError::Unauthorized);
    }
    caller.require_auth();
}

#[contract]
pub struct AdminContract;

#[contractimpl]
impl AdminContract {
    pub fn initialize(env: Env, admin: Address) {
        if is_initialized(&env) {
            panic_with_error!(&env, AdminError::AlreadyInitialized);
        }
        set_admin(&env, &admin);
    }

    pub fn transfer_admin(env: Env, current_admin: Address, new_admin: Address) {
        require_admin(&env, &current_admin);
        set_admin(&env, &new_admin);
    }

    pub fn get_admin(env: Env) -> Address {
        get_admin(&env)
    }

    pub fn is_initialized(env: Env) -> bool {
        is_initialized(&env)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn setup() -> (Env, Address, AdminContractClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(AdminContract, ());
        let client = AdminContractClient::new(&env, &contract_id);
        (env, contract_id, client)
    }

    #[test]
    fn test_initialize() {
        let (env, _, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);
        assert_eq!(client.get_admin(), admin);
        assert!(client.is_initialized());
    }

    #[test]
    fn test_initialize_twice_fails() {
        let (env, _, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);
        let other = Address::generate(&env);
        let result = client.try_initialize(&other);
        assert!(result.is_err());
    }

    #[test]
    fn test_transfer_admin() {
        let (env, _, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);
        let new_admin = Address::generate(&env);
        client.transfer_admin(&admin, &new_admin);
        assert_eq!(client.get_admin(), new_admin);
    }

    #[test]
    fn test_unauthorized_transfer_rejected() {
        let (env, _, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);
        let attacker = Address::generate(&env);
        let target = Address::generate(&env);
        env.mock_all_auths();
        let result = client.try_transfer_admin(&attacker, &target);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_admin_before_init_panics() {
        let env = Env::default();
        let contract_id = env.register(AdminContract, ());
        let client = AdminContractClient::new(&env, &contract_id);
        let result = client.try_get_admin();
        assert!(result.is_err());
    }

    #[test]
    fn test_must_sign_as_admin() {
        let env = Env::default();
        let contract_id = env.register(AdminContract, ());
        let client = AdminContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        let new_admin = Address::generate(&env);
        let result = client.try_transfer_admin(&admin, &new_admin);
        assert!(result.is_err());
    }
}

#![no_std]

use shared::utils::validate_amount;
use soroban_sdk::{contract, contractimpl, contracttype, panic_with_error, Address, Env};

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    Balance(Address, Address),
}
//  AlreadyInitialized = 1,

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum BalanceError {
    AlreadyInitialized = 1,
    Unauthorized = 2,
    InvalidAmount = 3,
}

impl From<BalanceError> for soroban_sdk::Error {
    fn from(value: BalanceError) -> Self {
        soroban_sdk::Error::from_contract_error(value as u32)
    }
}

#[contract]
pub struct BalanceContract;

#[contractimpl]
impl BalanceContract {
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&env, BalanceError::AlreadyInitialized);
        }

        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
    }

    pub fn set_user_balance(
        env: Env,
        admin: Address,
        user: Address,
        asset: Address,
        amount: i128,
    ) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        validate_amount(amount).unwrap_or_else(|_| {
            panic_with_error!(&env, BalanceError::InvalidAmount);
        });

        if amount == 0 {
            env.storage()
                .persistent()
                .remove(&DataKey::Balance(user, asset));
        } else {
            env.storage()
                .persistent()
                .set(&DataKey::Balance(user, asset), &amount);
        }
    }

    pub fn get_user_balance(env: Env, user: Address, asset: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Balance(user, asset))
            .unwrap_or(0)
    }

    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(env, BalanceError::Unauthorized));

        if admin != *caller {
            panic_with_error!(env, BalanceError::Unauthorized);
        }
    }
}

#[cfg(test)]
mod test {
    use super::{BalanceContract, BalanceContractClient};
    use soroban_sdk::{testutils::Address as _, Address, Env};

    fn setup() -> (Env, Address, Address, BalanceContractClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let asset = Address::generate(&env);
        let contract_id = env.register(BalanceContract, ());
        let client = BalanceContractClient::new(&env, &contract_id);
        client.initialize(&admin);
        (env, asset, admin, client)
    }

    #[test]
    fn get_user_balance_returns_zero_when_missing() {
        let (env, asset, _admin, client) = setup();
        let user = Address::generate(&env);

        assert_eq!(client.get_user_balance(&user, &asset), 0);
    }

    #[test]
    fn unknown_asset_returns_zero() {
        let (env, asset, admin, client) = setup();
        let user = Address::generate(&env);
        let other_asset = Address::generate(&env);

        client.set_user_balance(&admin, &user, &asset, &750i128);

        assert_eq!(client.get_user_balance(&user, &other_asset), 0);
    }

    #[test]
    fn get_user_balance_returns_stored_value() {
        let (env, asset, admin, client) = setup();
        let user = Address::generate(&env);

        client.set_user_balance(&admin, &user, &asset, &750i128);

        assert_eq!(client.get_user_balance(&user, &asset), 750);
    }

    #[test]
    fn setting_zero_clears_balance_back_to_default() {
        let (env, asset, admin, client) = setup();
        let user = Address::generate(&env);

        client.set_user_balance(&admin, &user, &asset, &750i128);
        client.set_user_balance(&admin, &user, &asset, &0i128);

        assert_eq!(client.get_user_balance(&user, &asset), 0);
    }

    #[test]
    #[should_panic]
    fn set_user_balance_rejects_negative_amounts() {
        let (env, asset, admin, client) = setup();
        let user = Address::generate(&env);

        client.set_user_balance(&admin, &user, &asset, &-1i128);
    }

    #[test]
    fn updating_balance_overwrites_previous_value() {
        let (env, asset, admin, client) = setup();
        let user = Address::generate(&env);

        client.set_user_balance(&admin, &user, &asset, &500i128);
        assert_eq!(client.get_user_balance(&user, &asset), 500);

        client.set_user_balance(&admin, &user, &asset, &1200i128);
        assert_eq!(client.get_user_balance(&user, &asset), 1200);
    }

    #[test]
    fn balances_are_isolated_between_users() {
        let (env, asset, admin, client) = setup();

        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        client.set_user_balance(&admin, &alice, &asset, &100i128);
        client.set_user_balance(&admin, &bob, &asset, &250i128);

        assert_eq!(client.get_user_balance(&alice, &asset), 100);
        assert_eq!(client.get_user_balance(&bob, &asset), 250);
    }

    #[test]
    fn balances_are_isolated_between_assets() {
        let (env, _asset, admin, client) = setup();
        let user = Address::generate(&env);
        let asset_a = Address::generate(&env);
        let asset_b = Address::generate(&env);

        client.set_user_balance(&admin, &user, &asset_a, &100i128);
        client.set_user_balance(&admin, &user, &asset_b, &250i128);

        assert_eq!(client.get_user_balance(&user, &asset_a), 100);
        assert_eq!(client.get_user_balance(&user, &asset_b), 250);
    }

    #[test]
    #[should_panic]
    fn non_admin_cannot_set_balance() {
        let (env, asset, _admin, client) = setup();

        let fake_admin = Address::generate(&env);
        let user = Address::generate(&env);

        client.set_user_balance(&fake_admin, &user, &asset, &100i128);
    }
}

//! Treasury reserve management.
//!
//! Tracks penalty, fee, and reward funds in a centralised reserve so that
//! accounting always balances.

#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, panic_with_error, symbol_short, Address, Env};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum TreasuryError {
    NotInitialized = 1,
    Unauthorized = 2,
    InsufficientBalance = 3,
    InvalidAmount = 4,
}

impl From<TreasuryError> for soroban_sdk::Error {
    fn from(e: TreasuryError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct ReserveBalance {
    pub penalties: i128,
    pub fees: i128,
    pub rewards: i128,
}

#[contracttype]
enum DataKey { Admin, Reserve }

#[contract]
pub struct TreasuryReserveContract;

#[contractimpl]
impl TreasuryReserveContract {
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Reserve, &ReserveBalance { penalties: 0, fees: 0, rewards: 0 });
    }

    pub fn deposit_penalty(env: Env, caller: Address, amount: i128) {
        caller.require_auth();
        if amount <= 0 { panic_with_error!(&env, TreasuryError::InvalidAmount); }
        let mut r: ReserveBalance = env.storage().instance().get(&DataKey::Reserve).unwrap();
        r.penalties = r.penalties.checked_add(amount).unwrap_or(i128::MAX);
        env.storage().instance().set(&DataKey::Reserve, &r);
        env.events().publish((symbol_short!("treasury"), symbol_short!("penalty")), (caller, amount));
    }

    pub fn deposit_fee(env: Env, caller: Address, amount: i128) {
        caller.require_auth();
        if amount <= 0 { panic_with_error!(&env, TreasuryError::InvalidAmount); }
        let mut r: ReserveBalance = env.storage().instance().get(&DataKey::Reserve).unwrap();
        r.fees = r.fees.checked_add(amount).unwrap_or(i128::MAX);
        env.storage().instance().set(&DataKey::Reserve, &r);
        env.events().publish((symbol_short!("treasury"), symbol_short!("fee")), (caller, amount));
    }

    pub fn get_balance(env: Env) -> ReserveBalance {
        env.storage().instance().get(&DataKey::Reserve).unwrap_or(ReserveBalance { penalties: 0, fees: 0, rewards: 0 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    #[test]
    fn balances_accumulate_correctly() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let id = env.register(TreasuryReserveContract, ());
        TreasuryReserveContract::initialize(env.clone(), admin.clone());
        let payer = Address::generate(&env);
        TreasuryReserveContract::deposit_penalty(env.clone(), payer.clone(), 500);
        TreasuryReserveContract::deposit_fee(env.clone(), payer.clone(), 200);
        let bal = TreasuryReserveContract::get_balance(env.clone());
        assert_eq!(bal.penalties, 500);
        assert_eq!(bal.fees, 200);
    }
}
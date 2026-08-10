use soroban_sdk::{contracttype, Address, String};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Payment {
    pub recipient: Address,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptEvent {
    pub batch_reference_id: String,
    pub token: Address,
    pub from: Address,
    pub total_payments: u32,
    pub total_amount: i128,
}

use soroban_sdk::{panic_with_error, Env};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    Unauthorized = 2,
}

impl From<Error> for soroban_sdk::Error {
    fn from(err: Error) -> Self {
        soroban_sdk::Error::from_contract_error(err as u32)
    }
}

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    BatchStatus(u64),
    BatchCounter,
}

fn bump_instance(_env: &Env) {}

/// Persist the admin address.
///
/// Private to this module — external code must go through `AdminContract::initialize`,
/// which enforces the one-time-only invariant.
pub(crate) fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
    bump_instance(env);
}

/// Load the admin address and refresh the instance TTL.
///
/// Panics with `Error::NotInitialized` if the contract has never been initialized,
/// giving callers a typed error instead of a host trap.
pub fn get_admin(env: &Env) -> Address {
    let admin = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized));
    bump_instance(env);
    admin
}

/// Return `true` when an admin has been set.
///
/// Cheaper than `get_admin` when you only need existence, not the address —
/// avoids a full deserialise.
pub fn is_initialized(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

/// Assert that `caller` is the stored admin.
///
/// Combines the storage read, the Soroban auth check, and the TTL bump in one
/// call so every admin-gated function reads identically:
///
/// ```rust
/// pub fn protected_action(env: Env, caller: Address) {
///     require_admin(&env, &caller);
///     // ... rest of logic
/// }
/// ```
pub fn require_admin(env: &Env, caller: &Address) {
    let admin = get_admin(env); // panics with NotInitialized if unset
    if &admin != caller {
        panic_with_error!(env, Error::Unauthorized);
    }
    caller.require_auth();
}

pub struct ContractUtils;

impl ContractUtils {
    pub fn get_admin(env: &Env) -> Address {
        get_admin(env)
    }
}

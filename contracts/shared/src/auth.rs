//! Centralised authorization helpers for StellarSpend contracts.
//!
//! Three orthogonal roles are supported:
//!
//! | Role     | Storage key              | Typical holder                  |
//! |----------|--------------------------|---------------------------------|
//! | Admin    | `SharedDataKey::Admin`   | Contract deployer / governance  |
//! | Owner    | caller == owner address  | Resource creator                |
//! | Operator | `SharedDataKey::Operator(addr)` | Delegated service account |
//!
//! ## Usage pattern
//!
//! 1. Call the `require_*` helper **before** any state mutation.
//! 2. The helpers invoke `caller.require_auth()` internally — do not repeat it.
//! 3. Operator approval is managed through `grant_operator` / `revoke_operator`,
//!    which themselves enforce admin-only access.

use soroban_sdk::{Address, Env};

use crate::{errors::SharedError, SharedDataKey};

// ---------------------------------------------------------------------------
// Admin
// ---------------------------------------------------------------------------

/// Reads the stored admin address, returning `NotInitialized` if absent.
pub fn get_admin(env: &Env) -> Result<Address, SharedError> {
    env.storage()
        .instance()
        .get(&SharedDataKey::Admin)
        .ok_or(SharedError::NotInitialized)
}

/// Returns `true` if `caller` matches the stored admin address.
///
/// Returns `false` (not an error) when the contract is uninitialized, so callers
/// that only need a boolean predicate do not have to handle `NotInitialized`.
pub fn is_admin(env: &Env, caller: &Address) -> bool {
    get_admin(env).map_or(false, |admin| caller == &admin)
}

/// Requires `caller` to be authenticated **and** to be the stored admin.
///
/// Returns `Unauthorized` when the caller is not the admin.
/// Returns `NotInitialized` when no admin has been stored yet.
pub fn require_admin(env: &Env, caller: &Address) -> Result<(), SharedError> {
    caller.require_auth();
    let admin = get_admin(env)?;
    if caller != &admin {
        return Err(SharedError::Unauthorized);
    }
    Ok(())
}

/// Returns the admin address stored under an arbitrary key.
///
/// This generic helper allows contracts to keep their own key enum and still
/// reuse the shared admin verification pattern.
pub fn get_admin_with_key<K>(env: &Env, key: &K) -> Result<Address, SharedError>
where
    K: contracttype::ContractType,
{
    env.storage()
        .instance()
        .get(key)
        .ok_or(SharedError::NotInitialized)
}

/// Requires the caller to be authenticated and to match the admin stored under the provided key.
pub fn require_admin_with_key<K>(env: &Env, key: &K, caller: &Address) -> Result<(), SharedError>
where
    K: contracttype::ContractType,
{
    caller.require_auth();
    let admin = get_admin_with_key(env, key)?;
    if caller != &admin {
        return Err(SharedError::Unauthorized);
    }
    Ok(())
}

/// Returns `true` when the caller matches the admin stored under the provided key.
pub fn is_admin_with_key<K>(env: &Env, key: &K, caller: &Address) -> bool
where
    K: contracttype::ContractType,
{
    get_admin_with_key(env, key).map_or(false, |admin| caller == &admin)
}

// ---------------------------------------------------------------------------
// Owner
// ---------------------------------------------------------------------------

/// Returns `true` if `caller` is the resource `owner`.
///
/// Ownership is purely an address comparison — no storage is read.
pub fn is_owner(caller: &Address, owner: &Address) -> bool {
    caller == owner
}

/// Requires `caller` to be authenticated **and** to be the resource `owner`.
///
/// Returns `Unauthorized` when the addresses differ.
pub fn require_owner(caller: &Address, owner: &Address) -> Result<(), SharedError> {
    caller.require_auth();
    if !is_owner(caller, owner) {
        return Err(SharedError::Unauthorized);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Operator
// ---------------------------------------------------------------------------

/// Returns `true` if `address` has been granted operator status.
pub fn is_operator(env: &Env, address: &Address) -> bool {
    env.storage()
        .instance()
        .get::<SharedDataKey, bool>(&SharedDataKey::Operator(address.clone()))
        .unwrap_or(false)
}

/// Grants operator status to `operator`. Only the current admin may call this.
///
/// Returns `Unauthorized` when `caller` is not the admin.
pub fn grant_operator(env: &Env, caller: &Address, operator: &Address) -> Result<(), SharedError> {
    require_admin(env, caller)?;
    env.storage()
        .instance()
        .set(&SharedDataKey::Operator(operator.clone()), &true);
    Ok(())
}

/// Revokes operator status from `operator`. Only the current admin may call this.
///
/// Silently succeeds when `operator` was not previously approved.
pub fn revoke_operator(env: &Env, caller: &Address, operator: &Address) -> Result<(), SharedError> {
    require_admin(env, caller)?;
    env.storage()
        .instance()
        .remove(&SharedDataKey::Operator(operator.clone()));
    Ok(())
}

/// Requires `caller` to be authenticated **and** to hold operator status.
///
/// Returns `Unauthorized` when the caller is not an approved operator.
pub fn require_operator(env: &Env, caller: &Address) -> Result<(), SharedError> {
    caller.require_auth();
    if !is_operator(env, caller) {
        return Err(SharedError::Unauthorized);
    }
    Ok(())
}

/// Requires `caller` to be authenticated **and** to be either the resource
/// `owner` or an approved operator.
///
/// This is the standard gate for actions that owners can always perform but
/// that may also be delegated to a service account.
pub fn require_owner_or_operator(
    env: &Env,
    caller: &Address,
    owner: &Address,
) -> Result<(), SharedError> {
    caller.require_auth();
    if !is_owner(caller, owner) && !is_operator(env, caller) {
        return Err(SharedError::Unauthorized);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{contract, contractimpl, Env};

    #[contract]
    struct TestContract;

    #[contractimpl]
    impl TestContract {
        pub fn noop() {}
    }

    fn setup() -> (Env, soroban_sdk::Address) {
        let env = Env::default();
        let id = env.register(TestContract, ());
        (env, id)
    }

    fn store_admin(env: &Env, admin: &Address) {
        env.storage().instance().set(&SharedDataKey::Admin, admin);
    }

    // --- get_admin ---

    #[test]
    fn get_admin_returns_not_initialized_when_absent() {
        let (env, id) = setup();
        env.as_contract(&id, || {
            assert_eq!(get_admin(&env), Err(SharedError::NotInitialized));
        });
    }

    #[test]
    fn get_admin_returns_stored_address() {
        let (env, id) = setup();
        let admin = Address::generate(&env);
        env.as_contract(&id, || {
            store_admin(&env, &admin);
            assert_eq!(get_admin(&env), Ok(admin));
        });
    }

    // --- is_admin ---

    #[test]
    fn is_admin_true_for_stored_admin() {
        let (env, id) = setup();
        let admin = Address::generate(&env);
        env.as_contract(&id, || {
            store_admin(&env, &admin);
            assert!(is_admin(&env, &admin));
        });
    }

    #[test]
    fn is_admin_false_for_different_address() {
        let (env, id) = setup();
        let admin = Address::generate(&env);
        let other = Address::generate(&env);
        env.as_contract(&id, || {
            store_admin(&env, &admin);
            assert!(!is_admin(&env, &other));
        });
    }

    #[test]
    fn is_admin_false_when_uninitialized() {
        let (env, id) = setup();
        let addr = Address::generate(&env);
        env.as_contract(&id, || {
            assert!(!is_admin(&env, &addr));
        });
    }

    // --- require_admin ---

    #[test]
    fn require_admin_ok_for_admin() {
        let (env, id) = setup();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        env.as_contract(&id, || {
            store_admin(&env, &admin);
            assert!(require_admin(&env, &admin).is_ok());
        });
    }

    #[test]
    fn require_admin_unauthorized_for_non_admin() {
        let (env, id) = setup();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let other = Address::generate(&env);
        env.as_contract(&id, || {
            store_admin(&env, &admin);
            assert_eq!(require_admin(&env, &other), Err(SharedError::Unauthorized));
        });
    }

    #[test]
    fn require_admin_not_initialized_when_no_admin_stored() {
        let (env, id) = setup();
        env.mock_all_auths();
        let caller = Address::generate(&env);
        env.as_contract(&id, || {
            assert_eq!(
                require_admin(&env, &caller),
                Err(SharedError::NotInitialized)
            );
        });
    }

    // --- is_owner ---

    #[test]
    fn is_owner_true_when_addresses_match() {
        let env = Env::default();
        let addr = Address::generate(&env);
        assert!(is_owner(&addr, &addr));
    }

    #[test]
    fn is_owner_false_when_addresses_differ() {
        let env = Env::default();
        let a = Address::generate(&env);
        let b = Address::generate(&env);
        assert!(!is_owner(&a, &b));
    }

    // --- require_owner ---

    #[test]
    fn require_owner_ok_when_caller_is_owner() {
        let env = Env::default();
        env.mock_all_auths();
        let addr = Address::generate(&env);
        assert!(require_owner(&addr, &addr).is_ok());
    }

    #[test]
    fn require_owner_unauthorized_for_different_address() {
        let env = Env::default();
        env.mock_all_auths();
        let caller = Address::generate(&env);
        let owner = Address::generate(&env);
        assert_eq!(
            require_owner(&caller, &owner),
            Err(SharedError::Unauthorized)
        );
    }

    // --- grant_operator / revoke_operator / is_operator ---

    #[test]
    fn operator_not_approved_by_default() {
        let (env, id) = setup();
        let op = Address::generate(&env);
        env.as_contract(&id, || {
            assert!(!is_operator(&env, &op));
        });
    }

    #[test]
    fn grant_operator_makes_is_operator_true() {
        let (env, id) = setup();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let op = Address::generate(&env);
        env.as_contract(&id, || {
            store_admin(&env, &admin);
            grant_operator(&env, &admin, &op).unwrap();
            assert!(is_operator(&env, &op));
        });
    }

    #[test]
    fn grant_operator_unauthorized_for_non_admin() {
        let (env, id) = setup();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let non_admin = Address::generate(&env);
        let op = Address::generate(&env);
        env.as_contract(&id, || {
            store_admin(&env, &admin);
            assert_eq!(
                grant_operator(&env, &non_admin, &op),
                Err(SharedError::Unauthorized)
            );
            assert!(!is_operator(&env, &op));
        });
    }

    #[test]
    fn revoke_operator_clears_approval() {
        let (env, id) = setup();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let op = Address::generate(&env);
        env.as_contract(&id, || {
            store_admin(&env, &admin);
            grant_operator(&env, &admin, &op).unwrap();
            revoke_operator(&env, &admin, &op).unwrap();
            assert!(!is_operator(&env, &op));
        });
    }

    #[test]
    fn revoke_operator_silently_succeeds_when_not_approved() {
        let (env, id) = setup();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let op = Address::generate(&env);
        env.as_contract(&id, || {
            store_admin(&env, &admin);
            assert!(revoke_operator(&env, &admin, &op).is_ok());
        });
    }

    #[test]
    fn revoke_operator_unauthorized_for_non_admin() {
        let (env, id) = setup();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let non_admin = Address::generate(&env);
        let op = Address::generate(&env);
        env.as_contract(&id, || {
            store_admin(&env, &admin);
            grant_operator(&env, &admin, &op).unwrap();
            assert_eq!(
                revoke_operator(&env, &non_admin, &op),
                Err(SharedError::Unauthorized)
            );
            assert!(is_operator(&env, &op)); // still approved
        });
    }

    // --- require_operator ---

    #[test]
    fn require_operator_ok_for_approved_operator() {
        let (env, id) = setup();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let op = Address::generate(&env);
        env.as_contract(&id, || {
            store_admin(&env, &admin);
            grant_operator(&env, &admin, &op).unwrap();
            assert!(require_operator(&env, &op).is_ok());
        });
    }

    #[test]
    fn require_operator_unauthorized_for_unapproved_address() {
        let (env, id) = setup();
        env.mock_all_auths();
        let op = Address::generate(&env);
        env.as_contract(&id, || {
            assert_eq!(require_operator(&env, &op), Err(SharedError::Unauthorized));
        });
    }

    // --- require_owner_or_operator ---

    #[test]
    fn owner_or_operator_ok_for_owner() {
        let (env, id) = setup();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        env.as_contract(&id, || {
            assert!(require_owner_or_operator(&env, &owner, &owner).is_ok());
        });
    }

    #[test]
    fn owner_or_operator_ok_for_approved_operator() {
        let (env, id) = setup();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let owner = Address::generate(&env);
        let op = Address::generate(&env);
        env.as_contract(&id, || {
            store_admin(&env, &admin);
            grant_operator(&env, &admin, &op).unwrap();
            assert!(require_owner_or_operator(&env, &op, &owner).is_ok());
        });
    }

    #[test]
    fn owner_or_operator_unauthorized_for_third_party() {
        let (env, id) = setup();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        let stranger = Address::generate(&env);
        env.as_contract(&id, || {
            assert_eq!(
                require_owner_or_operator(&env, &stranger, &owner),
                Err(SharedError::Unauthorized)
            );
        });
    }
}

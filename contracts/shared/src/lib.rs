#![no_std]

extern crate alloc;

use soroban_sdk::{contracttype, Address, Env, String};

pub mod assets;
pub mod auth;
pub mod authorization;
pub mod batch_result;
pub mod date_utils;
pub mod errors;
pub mod oracle;
pub mod reflector_oracle;
pub mod sanitizer;
pub mod reentrancy;
pub mod utils;
pub mod validation;

pub use batch_result::{BatchAtomicityPolicy, BatchExecutionResult, BatchItemResult};
pub use errors::SharedError;

pub const SHARED_VERSION: &str = "0.1.0";

pub fn get_version(env: Env) -> String {
    String::from_str(&env, SHARED_VERSION)
}

/// Health check response containing contract status and version.
#[contracttype]
#[derive(Clone, Debug)]
pub struct HealthStatus {
    pub status: String,
    pub version: String,
}

/// Returns contract health status and version for monitoring and frontend use.
pub fn health_check(env: Env) -> HealthStatus {
    HealthStatus {
        status: String::from_str(&env, "ok"),
        version: String::from_str(&env, SHARED_VERSION),
    }
}

#[contracttype]
#[derive(Clone)]
pub enum SharedDataKey {
    Admin,
    /// Approved operator flag keyed by the operator's address.
    Operator(Address),
}

/// Returns the current contract owner/admin address.
pub fn get_contract_owner(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&SharedDataKey::Admin)
        .expect("Contract owner not initialized")
}

/// Updates the contract owner/admin address.
/// Only the current owner can perform this action.
pub fn update_contract_owner(env: &Env, new_owner: Address) {
    let current_owner: Address = get_contract_owner(env);
    current_owner.require_auth();
    env.storage()
        .instance()
        .set(&SharedDataKey::Admin, &new_owner);
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod tests {
    use super::{get_version, health_check};
    use soroban_sdk::{Env, String};

    #[test]
    fn returns_shared_version() {
        let env = Env::default();
        let version = get_version(env);
        assert_eq!(version, String::from_str(&Env::default(), "0.1.0"));
    }

    #[test]
    fn health_check_returns_ok_and_version() {
        let env = Env::default();
        let status = health_check(env.clone());
        assert_eq!(status.status, String::from_str(&env, "ok"));
        assert_eq!(status.version, String::from_str(&env, "0.1.0"));
    }
}

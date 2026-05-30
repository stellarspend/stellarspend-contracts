//! Budget archiving utilities.
//!
//! Inactive budgets can be archived to keep active queries fast.
//! Archived budgets remain recoverable via `unarchive`.

#![no_std]

use soroban_sdk::{contracttype, panic_with_error, symbol_short, Address, Env};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ArchiveError {
    AlreadyArchived = 1,
    NotArchived = 2,
    Unauthorized = 3,
}

impl From<ArchiveError> for soroban_sdk::Error {
    fn from(e: ArchiveError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

#[contracttype]
#[derive(Clone)]
enum ArchiveKey { Archived(Address) }

/// Marks a budget as archived. Panics if already archived.
pub fn archive(env: &Env, user: &Address) {
    if is_archived(env, user) {
        panic_with_error!(env, ArchiveError::AlreadyArchived);
    }
    env.storage().persistent().set(&ArchiveKey::Archived(user.clone()), &true);
    env.events().publish((symbol_short!("budget"), symbol_short!("archived")), user.clone());
}

/// Removes the archived flag. Panics if not currently archived.
pub fn unarchive(env: &Env, user: &Address) {
    if !is_archived(env, user) {
        panic_with_error!(env, ArchiveError::NotArchived);
    }
    env.storage().persistent().remove(&ArchiveKey::Archived(user.clone()));
    env.events().publish((symbol_short!("budget"), symbol_short!("unarchived")), user.clone());
}

/// Returns `true` if the budget for `user` is currently archived.
pub fn is_archived(env: &Env, user: &Address) -> bool {
    env.storage()
        .persistent()
        .get::<ArchiveKey, bool>(&ArchiveKey::Archived(user.clone()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    #[test]
    fn archive_and_unarchive_roundtrip() {
        let env = Env::default();
        let user = Address::generate(&env);
        assert!(!is_archived(&env, &user));
        archive(&env, &user);
        assert!(is_archived(&env, &user));
        unarchive(&env, &user);
        assert!(!is_archived(&env, &user));
    }
}
//! # Access Control Contract
//!
//! A robust Role-Based Access Control (RBAC) system for StellarSpend contracts.
//! Supports multiple roles with hierarchical permissions and comprehensive event logging.

#![no_std]

use shared::auth::require_admin_with_key;
use soroban_sdk::{contract, contractimpl, contracttype, panic_with_error, Address, Env, Map};

/// Storage keys for the access control contract
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    /// Contract admin (super admin)
    Admin,
    /// Map of address to their roles
    UserRoles(Address),
    /// Total number of role assignments
    TotalRoleAssignments,
}

/// Available roles in the system
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Role {
    /// Super administrator with all permissions
    Admin = 0,
    /// Regular user with limited permissions
    User = 1,
    /// Operator with elevated permissions for operations
    Operator = 2,
    /// Auditor with read-only access
    Auditor = 3,
}

/// Error codes for access control operations
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum AccessControlError {
    /// Contract not initialized
    NotInitialized = 1,
    /// Caller is not authorized
    Unauthorized = 2,
    /// Invalid role specified
    InvalidRole = 3,
    /// User already has the role
    RoleAlreadyAssigned = 4,
    /// User does not have the role
    RoleNotAssigned = 5,
    /// Cannot revoke admin from self
    CannotRevokeSelfAdmin = 6,
}

impl From<AccessControlError> for soroban_sdk::Error {
    fn from(e: AccessControlError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

#[contract]
pub struct AccessControlContract;

#[contractimpl]
impl AccessControlContract {
    /// Initialize the contract with an admin address
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&env, AccessControlError::NotInitialized);
        }

        admin.require_auth();

        // Set the admin
        env.storage().instance().set(&DataKey::Admin, &admin);

        // Assign admin role to the initializer
        let mut roles = Map::new(&env);
        roles.set(Role::Admin, true);
        env.storage()
            .instance()
            .set(&DataKey::UserRoles(admin.clone()), &roles);

        // Initialize counters
        env.storage()
            .instance()
            .set(&DataKey::TotalRoleAssignments, &1u64);

        // Emit initialization event
        env.events()
            .publish(("access_control", "initialized"), admin);
    }

    /// Assign a role to a user (admin only)
    pub fn grant_role(env: Env, caller: Address, user: Address, role: Role) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        // Get or create user's role map
        let mut roles: Map<Role, bool> = env
            .storage()
            .instance()
            .get(&DataKey::UserRoles(user.clone()))
            .unwrap_or(Map::new(&env));

        // Check if role already assigned
        if roles.get(role.clone()).unwrap_or(false) {
            panic_with_error!(&env, AccessControlError::RoleAlreadyAssigned);
        }

        // Assign the role
        roles.set(role.clone(), true);
        env.storage()
            .instance()
            .set(&DataKey::UserRoles(user.clone()), &roles);

        // Update counter
        let count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalRoleAssignments)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::TotalRoleAssignments, &(count + 1));

        // Emit role granted event
        env.events()
            .publish(("access_control", "role_granted"), (user, role));
    }

    /// Revoke a role from a user (admin only)
    pub fn revoke_role(env: Env, caller: Address, user: Address, role: Role) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        // Prevent admin from revoking their own admin role
        if caller == user && role == Role::Admin {
            panic_with_error!(&env, AccessControlError::CannotRevokeSelfAdmin);
        }

        // Get user's role map
        let mut roles: Map<Role, bool> = env
            .storage()
            .instance()
            .get(&DataKey::UserRoles(user.clone()))
            .unwrap_or(Map::new(&env));

        // Check if role is assigned
        if !roles.get(role.clone()).unwrap_or(false) {
            panic_with_error!(&env, AccessControlError::RoleNotAssigned);
        }

        // Revoke the role
        roles.set(role.clone(), false);
        env.storage()
            .instance()
            .set(&DataKey::UserRoles(user.clone()), &roles);

        // Update counter
        let count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalRoleAssignments)
            .unwrap_or(0);
        if count > 0 {
            env.storage()
                .instance()
                .set(&DataKey::TotalRoleAssignments, &(count - 1));
        }

        // Emit role revoked event
        env.events()
            .publish(("access_control", "role_revoked"), (user, role));
    }

    /// Check if a user has a specific role
    pub fn has_role(env: Env, user: Address, role: Role) -> bool {
        let roles: Map<Role, bool> = env
            .storage()
            .instance()
            .get(&DataKey::UserRoles(user))
            .unwrap_or(Map::new(&env));

        roles.get(role).unwrap_or(false)
    }

    /// Check permissions for an action (generic gate)
    pub fn check_permission(env: Env, user: Address, role: Role) {
        let roles: Map<Role, bool> = env
            .storage()
            .instance()
            .get(&DataKey::UserRoles(user.clone()))
            .unwrap_or(Map::new(&env));

        let is_admin = roles.get(Role::Admin).unwrap_or(false);
        let has_role = roles.get(role).unwrap_or(false);

        if !is_admin && !has_role {
            panic_with_error!(&env, AccessControlError::Unauthorized);
        }
    }

    /// Get all roles for a user
    pub fn get_user_roles(env: Env, user: Address) -> soroban_sdk::Vec<Role> {
        let roles: Map<Role, bool> = env
            .storage()
            .instance()
            .get(&DataKey::UserRoles(user))
            .unwrap_or(Map::new(&env));

        let mut active_roles = soroban_sdk::Vec::new(&env);
        if roles.get(Role::Admin).unwrap_or(false) {
            active_roles.push_back(Role::Admin);
        }
        if roles.get(Role::User).unwrap_or(false) {
            active_roles.push_back(Role::User);
        }
        if roles.get(Role::Operator).unwrap_or(false) {
            active_roles.push_back(Role::Operator);
        }
        if roles.get(Role::Auditor).unwrap_or(false) {
            active_roles.push_back(Role::Auditor);
        }

        active_roles
    }

    /// Check if a user has any active role
    pub fn has_any_role(env: Env, user: Address) -> bool {
        let roles: Map<Role, bool> = env
            .storage()
            .instance()
            .get(&DataKey::UserRoles(user))
            .unwrap_or(Map::new(&env));

        roles.get(Role::Admin).unwrap_or(false)
            || roles.get(Role::User).unwrap_or(false)
            || roles.get(Role::Operator).unwrap_or(false)
            || roles.get(Role::Auditor).unwrap_or(false)
    }

    /// Transfer admin role to a new address (current admin only)
    pub fn transfer_admin(env: Env, current_admin: Address, new_admin: Address) {
        current_admin.require_auth();
        Self::require_admin(&env, &current_admin);

        // Revoke admin role from current admin
        let mut current_roles: Map<Role, bool> = env
            .storage()
            .instance()
            .get(&DataKey::UserRoles(current_admin.clone()))
            .unwrap_or(Map::new(&env));
        current_roles.set(Role::Admin, false);
        env.storage()
            .instance()
            .set(&DataKey::UserRoles(current_admin.clone()), &current_roles);

        // Grant admin role to new admin
        let mut new_roles: Map<Role, bool> = env
            .storage()
            .instance()
            .get(&DataKey::UserRoles(new_admin.clone()))
            .unwrap_or(Map::new(&env));
        new_roles.set(Role::Admin, true);
        env.storage()
            .instance()
            .set(&DataKey::UserRoles(new_admin.clone()), &new_roles);

        // Update admin storage
        env.storage().instance().set(&DataKey::Admin, &new_admin);

        // Emit admin transfer event
        env.events().publish(
            ("access_control", "admin_transferred"),
            (current_admin, new_admin),
        );
    }

    /// Get the current admin address
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, AccessControlError::NotInitialized))
    }

    /// Get total number of role assignments
    pub fn get_total_role_assignments(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalRoleAssignments)
            .unwrap_or(0)
    }
}

impl AccessControlContract {
    /// Internal helper: Require that the caller has admin role
    fn require_admin(env: &Env, caller: &Address) {
        if let Err(err) = require_admin_with_key(env, &DataKey::Admin, caller) {
            match err {
                shared::errors::SharedError::NotInitialized => {
                    panic_with_error!(env, AccessControlError::NotInitialized)
                }
                _ => panic_with_error!(env, AccessControlError::Unauthorized),
            }
        }
    }
}

#[cfg(test)]
mod test;

//! # Spending Categories Contract
//!
//! A Soroban smart contract for managing spending categories with
//! rename support, duplicate prevention, and event emission.
//!
//! ## Features
//!
//! - **Category Management**: Create and rename spending categories
//! - **Duplicate Prevention**: Ensures category names are unique per wallet
//! - **Event Emission**: Emits events for category creation and renames
//! - **Admin Control**: Only admin can manage categories

#![no_std]

mod validation;

use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, symbol_short, Address, Env, Symbol, Vec,
};

use crate::validation::validate_category_name;

/// Error codes for the spending categories contract.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum CategoryError {
    /// Contract not initialized
    NotInitialized = 1,
    /// Caller is not authorized
    Unauthorized = 2,
    /// Category name is empty or invalid
    InvalidName = 3,
    /// Category name already exists for this wallet
    DuplicateName = 4,
    /// Category not found
    CategoryNotFound = 5,
    /// Spending category limit cannot be negative
    InvalidLimit = 6,
}

impl From<CategoryError> for soroban_sdk::Error {
    fn from(e: CategoryError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

/// Represents a spending category entry.
#[derive(Clone, Debug)]
#[contracttype]
pub struct SpendingCategory {
    /// Unique category identifier
    pub category_id: u64,
    /// The wallet owner of this category
    pub user: Address,
    /// The category name
    pub name: Symbol,
    /// When the category was created (ledger sequence)
    pub created_at: u64,
    /// When the category was last updated
    pub updated_at: u64,
}

/// Storage keys for the contract.
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    LastCategoryId,
    TotalCategories,
    Category(u64),
    /// Maps (user, name) -> category_id for duplicate detection
    CategoryByName(Address, Symbol),
    /// Maps user -> list of category IDs
    UserCategories(Address),
    /// Maps (owner, category) -> spending limit for category-specific budgets
    SpendingCategoryLimit(Address, Symbol),
}

/// Events emitted by the contract.
pub struct CategoryEvents;

impl CategoryEvents {
    pub fn category_created(env: &Env, category: &SpendingCategory) {
        let topics = (symbol_short!("category"), symbol_short!("created"));
        env.events().publish(
            topics,
            (
                category.category_id,
                category.user.clone(),
                category.name.clone(),
            ),
        );
    }

    pub fn category_renamed(env: &Env, category_id: u64, old_name: Symbol, new_name: Symbol) {
        let topics = (symbol_short!("category"), symbol_short!("renamed"));
        env.events()
            .publish(topics, (category_id, old_name, new_name));
    }
}

#[contract]
pub struct SpendingCategoriesContract;

#[contractimpl]
impl SpendingCategoriesContract {
    /// Initializes the contract with an admin address.
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Contract already initialized");
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::LastCategoryId, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::TotalCategories, &0u64);
    }

    /// Creates a new spending category for a user.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `caller` - The admin address
    /// * `user` - The wallet owner
    /// * `name` - The category name
    ///
    /// # Returns
    /// * `SpendingCategory` - The created category
    pub fn create_category(
        env: Env,
        caller: Address,
        user: Address,
        name: Symbol,
    ) -> SpendingCategory {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        // Validate category name
        if validate_category_name(&name).is_err() {
            panic_with_error!(&env, CategoryError::InvalidName);
        }

        // Check for duplicate name for this user
        if env
            .storage()
            .persistent()
            .has(&DataKey::CategoryByName(user.clone(), name.clone()))
        {
            panic_with_error!(&env, CategoryError::DuplicateName);
        }

        let category_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::LastCategoryId)
            .unwrap_or(0)
            + 1;

        let current_ledger = env.ledger().sequence() as u64;

        let category = SpendingCategory {
            category_id,
            user: user.clone(),
            name: name.clone(),
            created_at: current_ledger,
            updated_at: current_ledger,
        };

        // Store category
        env.storage()
            .persistent()
            .set(&DataKey::Category(category_id), &category);

        // Store name-to-id mapping for duplicate detection
        env.storage()
            .persistent()
            .set(&DataKey::CategoryByName(user.clone(), name), &category_id);

        // Update user's category list
        let mut user_categories: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::UserCategories(user.clone()))
            .unwrap_or(Vec::new(&env));
        user_categories.push_back(category_id);
        env.storage()
            .persistent()
            .set(&DataKey::UserCategories(user.clone()), &user_categories);

        // Update counters
        env.storage()
            .instance()
            .set(&DataKey::LastCategoryId, &category_id);

        let total: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalCategories)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::TotalCategories, &(total + 1));

        // Emit event
        CategoryEvents::category_created(&env, &category);

        category
    }

    /// Renames an existing spending category.
    ///
    /// This preserves the category_id so that existing transactions
    /// and spending limits remain linked to the category.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `caller` - The admin address
    /// * `category_id` - The ID of the category to rename
    /// * `new_name` - The new name for the category
    ///
    /// # Returns
    /// * `SpendingCategory` - The updated category
    pub fn rename_category(
        env: Env,
        caller: Address,
        category_id: u64,
        new_name: Symbol,
    ) -> SpendingCategory {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        // Validate new name
        if validate_category_name(&new_name).is_err() {
            panic_with_error!(&env, CategoryError::InvalidName);
        }

        // Fetch existing category
        let mut category: SpendingCategory = env
            .storage()
            .persistent()
            .get(&DataKey::Category(category_id))
            .unwrap_or_else(|| panic_with_error!(&env, CategoryError::CategoryNotFound));

        let old_name = category.name.clone();

        // Check for duplicate name (excluding self)
        if old_name != new_name {
            if env.storage().persistent().has(&DataKey::CategoryByName(
                category.user.clone(),
                new_name.clone(),
            )) {
                panic_with_error!(&env, CategoryError::DuplicateName);
            }

            // Remove old name mapping
            env.storage().persistent().remove(&DataKey::CategoryByName(
                category.user.clone(),
                old_name.clone(),
            ));

            // Add new name mapping
            env.storage().persistent().set(
                &DataKey::CategoryByName(category.user.clone(), new_name.clone()),
                &category_id,
            );
        }

        // Update category
        category.name = new_name.clone();
        category.updated_at = env.ledger().sequence() as u64;

        env.storage()
            .persistent()
            .set(&DataKey::Category(category_id), &category);

        // Emit rename event
        CategoryEvents::category_renamed(&env, category_id, old_name, new_name);

        category
    }

    /// Retrieves a category by ID.
    pub fn get_category(env: Env, category_id: u64) -> Option<SpendingCategory> {
        env.storage()
            .persistent()
            .get(&DataKey::Category(category_id))
    }

    /// Retrieves all category IDs for a user.
    pub fn get_user_categories(env: Env, user: Address) -> Vec<u64> {
        env.storage()
            .persistent()
            .get(&DataKey::UserCategories(user))
            .unwrap_or(Vec::new(&env))
    }

    /// Sets the spending limit for a specific owner/category pair.
    ///
    /// Limits are stored independently from category metadata so callers can
    /// retrieve the configured cap without changing category enforcement.
    pub fn set_spending_category_limit(
        env: Env,
        caller: Address,
        owner: Address,
        category: Symbol,
        limit: i128,
    ) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        if limit < 0 {
            panic_with_error!(&env, CategoryError::InvalidLimit);
        }

        env.storage()
            .persistent()
            .set(&DataKey::SpendingCategoryLimit(owner, category), &limit);
    }

    /// Returns the spending limit for a specific owner/category pair.
    ///
    /// Unset category limits default to `0`.
    pub fn get_spending_category_limit(env: Env, owner: Address, category: Symbol) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::SpendingCategoryLimit(owner, category))
            .unwrap_or(0)
    }

    /// Returns true when the user has a category with the supplied name.
    ///
    /// This is intentionally read-only so composition contracts can validate
    /// category-aware spending rules without duplicating category storage.
    pub fn category_exists(env: Env, user: Address, name: Symbol) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::CategoryByName(user, name))
    }

    /// Returns the admin address.
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Contract not initialized")
    }

    /// Updates the admin address.
    pub fn set_admin(env: Env, current_admin: Address, new_admin: Address) {
        current_admin.require_auth();
        Self::require_admin(&env, &current_admin);

        env.storage().instance().set(&DataKey::Admin, &new_admin);
    }

    /// Returns the total number of categories created.
    pub fn get_total_categories(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalCategories)
            .unwrap_or(0)
    }

    // Internal helper to verify admin
    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Contract not initialized");

        if *caller != admin {
            panic_with_error!(env, CategoryError::Unauthorized);
        }
    }
}

#[cfg(test)]
mod test;

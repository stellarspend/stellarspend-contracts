//! # Budget Contract
//!
//! A Soroban smart contract for managing user budgets with validation and event emission.
//!
//! ## Features
//!
//! - **Individual Budget Updates**: Update single user budgets
//! - **Validation**: Prevents negative or zero allocations
//! - **Event Emission**: Tracks budget updates
//! - **Atomic Operations**: Ensures reliable state changes
//!
#![no_std]

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, Symbol};

/// Error codes for the budget contract.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum BudgetError {
    /// Contract not initialized
    NotInitialized = 1,
    /// Caller is not authorized
    Unauthorized = 2,
    /// Invalid budget amount (negative or zero)
    InvalidAmount = 3,
    /// User not found
    UserNotFound = 4,
    /// Amount violates a budget rule
    RuleViolation = 5,
}

impl From<BudgetError> for soroban_sdk::Error {
    fn from(e: BudgetError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

/// Budget record for a user
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BudgetRecord {
    pub user: Address,
    pub amount: i128,
    pub last_updated: u64,
}

/// Budget rules that can be configured dynamically
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BudgetRule {
    /// Maximum allowed budget amount
    MaxAmount(i128),
    /// Minimum allowed budget amount
    MinAmount(i128),
}

/// Storage keys for the contract
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    Budget(Address),
    TotalAllocated,
    GlobalRules,
    UserRules(Address),
}

#[contract]
pub struct BudgetContract;

#[contractimpl]
impl BudgetContract {
    /// Initializes the contract with an admin address.
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::TotalAllocated, &0i128);
    }

    /// Updates a single user's budget.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - The admin address calling the function
    /// * `user` - The user address to update budget for
    /// * `amount` - The new budget amount
    pub fn update_budget(env: Env, admin: Address, user: Address, amount: i128) {
        // Verify admin authority
        admin.require_auth();
        Self::require_admin(&env, &admin);

        // Validate amount
        if amount <= 0 {
            panic_with_error!(&env, BudgetError::InvalidAmount);
        }

        // Validate amount against rules
        Self::validate_budget_rules(&env, &user, amount);

        let current_time = env.ledger().timestamp();
        
        // Get current total allocated
        let mut total_allocated: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalAllocated)
            .unwrap_or(0);

        // Check if user exists and get old amount
        if let Some(old_record) = env.storage().persistent().get(&DataKey::Budget(user.clone())) {
            // Subtract old amount from total
            total_allocated = total_allocated.checked_sub(old_record.amount).unwrap_or(0);
        }

        // Add new amount to total
        total_allocated = total_allocated.checked_add(amount).unwrap_or(i128::MAX);

        // Create new budget record
        let record = BudgetRecord {
            user: user.clone(),
            amount,
            last_updated: current_time,
        };

        // Store the updated budget
        env.storage()
            .persistent()
            .set(&DataKey::Budget(user.clone()), &record);

        // Update total allocated
        env.storage()
            .instance()
            .set(&DataKey::TotalAllocated, &total_allocated);

        // Emit update event
        env.events().publish(
            (symbol_short!("budget"), symbol_short!("updated")),
            (user, amount, current_time),
        );
    }

    /// Retrieves the budget for a specific user.
    pub fn get_budget(env: Env, user: Address) -> Option<BudgetRecord> {
        env.storage().persistent().get(&DataKey::Budget(user))
    }

    /// Returns the admin address
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized")
    }

    /// Returns the total allocated budget amount
    pub fn get_total_allocated(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalAllocated)
            .unwrap_or(0)
    }

    /// Internal helper to verify admin authority
    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");

        if *caller != admin {
            panic_with_error!(env, BudgetError::Unauthorized);
        }
    }

    /// Internal helper to validate budget against configured rules
    fn validate_budget_rules(env: &Env, user: &Address, amount: i128) {
        // Check global rules
        let global_rules: Vec<BudgetRule> = env
            .storage()
            .instance()
            .get(&DataKey::GlobalRules)
            .unwrap_or(Vec::new(env));
        for rule in global_rules.iter() {
            Self::check_rule(env, rule, amount);
        }

        // Check user-specific rules
        let user_rules: Vec<BudgetRule> = env
            .storage()
            .instance()
            .get(&DataKey::UserRules(user.clone()))
            .unwrap_or(Vec::new(env));
        for rule in user_rules.iter() {
            Self::check_rule(env, rule, amount);
        }
    }

    fn check_rule(env: &Env, rule: BudgetRule, amount: i128) {
        match rule {
            BudgetRule::MaxAmount(max) => {
                if amount > max {
                    panic_with_error!(env, BudgetError::RuleViolation);
                }
            }
            BudgetRule::MinAmount(min) => {
                if amount < min {
                    panic_with_error!(env, BudgetError::RuleViolation);
                }
            }
        }
    }

    /// Adds a global budget rule
    pub fn add_global_rule(env: Env, admin: Address, rule: BudgetRule) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        let mut rules: Vec<BudgetRule> = env
            .storage()
            .instance()
            .get(&DataKey::GlobalRules)
            .unwrap_or(Vec::new(&env));
        rules.push_back(rule);
        env.storage()
            .instance()
            .set(&DataKey::GlobalRules, &rules);
    }

    /// Adds a user-specific budget rule
    pub fn add_user_rule(env: Env, admin: Address, user: Address, rule: BudgetRule) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        let mut rules: Vec<BudgetRule> = env
            .storage()
            .instance()
            .get(&DataKey::UserRules(user.clone()))
            .unwrap_or(Vec::new(&env));
        rules.push_back(rule);
        env.storage()
            .instance()
            .set(&DataKey::UserRules(user), &rules);
    }
}

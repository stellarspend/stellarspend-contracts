//! # Overdraft Protection Contract
//!
//! A Soroban smart contract for preventing transactions that exceed allocated budgets.
//!
//! ## Features
//!
//! - **Category Limit Checks**: Validates transactions against category-specific budgets
//! - **Overdraft Prevention**: Blocks transactions that would exceed allocated limits
//! - **Event Emission**: Emits events when overdraft attempts are detected
//! - **Spending Tracking**: Tracks spent amounts per category per user
//!

use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, symbol_short, Address, Env, Map,
    Symbol, Vec,
};

/// Error codes for the overdraft contract.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum OverdraftError {
    /// Contract not initialized
    NotInitialized = 1,
    /// Caller is not authorized
    Unauthorized = 2,
    /// Invalid amount (negative or zero)
    InvalidAmount = 3,
    /// Transaction would exceed budget limit (overdraft)
    OverdraftLimitExceeded = 4,
    /// Category not found
    CategoryNotFound = 5,
    /// Budget not found for user
    BudgetNotFound = 6,
    /// Invalid category
    InvalidCategory = 7,
}

impl From<OverdraftError> for soroban_sdk::Error {
    fn from(e: OverdraftError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

/// Budget category with limit and spent tracking
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CategoryBudget {
    /// Category name
    pub name: Symbol,
    /// Allocated budget limit
    pub limit: i128,
    /// Amount already spent
    pub spent: i128,
}

/// User's budget configuration with categories
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserBudget {
    /// User address
    pub user: Address,
    /// Category budgets
    pub categories: Map<Symbol, CategoryBudget>,
    /// Last updated timestamp
    pub last_updated: u64,
}

/// Storage keys for the contract
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    UserBudget(Address),
    TotalOverdraftAttempts,
    /// Per-wallet overdraft protection limit
    OverdraftLimit(Address),
}

/// Result of a transaction check
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransactionCheckResult {
    /// Whether the transaction is allowed
    pub allowed: bool,
    /// Remaining budget in the category
    pub remaining: i128,
    /// Amount that would be exceeded by (if not allowed)
    pub exceeded_by: i128,
}

/// Events emitted by the overdraft contract
pub struct OverdraftEvents;

impl OverdraftEvents {
    /// Event emitted when an overdraft attempt is blocked
    pub fn overdraft_attempted(
        env: &Env,
        user: &Address,
        category: &Symbol,
        requested_amount: i128,
        available_budget: i128,
    ) {
        let topics = (
            symbol_short!("overdraft"),
            symbol_short!("attempt"),
            category.clone(),
        );
        env.events()
            .publish(topics, (user.clone(), requested_amount, available_budget));
    }

    /// Event emitted when a transaction is successfully validated
    pub fn transaction_validated(
        env: &Env,
        user: &Address,
        category: &Symbol,
        amount: i128,
        remaining_budget: i128,
    ) {
        let topics = (
            symbol_short!("overdraft"),
            symbol_short!("validated"),
            category.clone(),
        );
        env.events()
            .publish(topics, (user.clone(), amount, remaining_budget));
    }

    /// Event emitted when budget is updated
    pub fn budget_updated(env: &Env, user: &Address, category: &Symbol, new_limit: i128) {
        let topics = (
            symbol_short!("overdraft"),
            symbol_short!("updated"),
            category.clone(),
        );
        env.events().publish(topics, (user.clone(), new_limit));
    }
}

#[contract]
pub struct OverdraftContract;

#[contractimpl]
impl OverdraftContract {
    /// Initializes the contract with an admin address.
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::TotalOverdraftAttempts, &0u64);
    }

    /// Sets up budget categories for a user.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - The admin address calling the function
    /// * `user` - The user address
    /// * `categories` - Vector of category budgets
    pub fn set_user_budget(
        env: Env,
        admin: Address,
        user: Address,
        categories: Vec<CategoryBudget>,
    ) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        let current_time = env.ledger().timestamp();

        // Create category map
        let mut category_map = Map::<Symbol, CategoryBudget>::new(&env);
        for category in categories.iter() {
            if category.limit < 0 {
                panic_with_error!(&env, OverdraftError::InvalidAmount);
            }
            category_map.set(category.name.clone(), category.clone());
        }

        let user_budget = UserBudget {
            user: user.clone(),
            categories: category_map,
            last_updated: current_time,
        };

        env.storage()
            .persistent()
            .set(&DataKey::UserBudget(user.clone()), &user_budget);
    }

    /// Adds or updates a single category budget for a user.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - The admin address calling the function
    /// * `user` - The user address
    /// * `category` - Category name
    /// * `limit` - Budget limit for the category
    pub fn set_category_budget(
        env: Env,
        admin: Address,
        user: Address,
        category: Symbol,
        limit: i128,
    ) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        if limit < 0 {
            panic_with_error!(&env, OverdraftError::InvalidAmount);
        }

        let current_time = env.ledger().timestamp();

        // Get existing user budget or create new one
        let mut user_budget = if let Some(existing) = env
            .storage()
            .persistent()
            .get::<DataKey, UserBudget>(&DataKey::UserBudget(user.clone()))
        {
            existing
        } else {
            UserBudget {
                user: user.clone(),
                categories: Map::<Symbol, CategoryBudget>::new(&env),
                last_updated: current_time,
            }
        };

        let category_budget = CategoryBudget {
            name: category.clone(),
            limit,
            spent: 0,
        };

        user_budget
            .categories
            .set(category.clone(), category_budget);
        user_budget.last_updated = current_time;

        env.storage()
            .persistent()
            .set(&DataKey::UserBudget(user.clone()), &user_budget);

        OverdraftEvents::budget_updated(&env, &user, &category, limit);
    }

    /// Checks if a transaction would exceed the category budget.
    /// This function does NOT block the transaction, only checks.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `user` - The user address
    /// * `category` - Category name
    /// * `amount` - Amount to check
    ///
    /// # Returns
    /// TransactionCheckResult with whether the transaction is allowed
    pub fn check_transaction(
        env: Env,
        user: Address,
        category: Symbol,
        amount: i128,
    ) -> TransactionCheckResult {
        if amount <= 0 {
            return TransactionCheckResult {
                allowed: false,
                remaining: 0,
                exceeded_by: 0,
            };
        }

        let user_budget: UserBudget = env
            .storage()
            .persistent()
            .get(&DataKey::UserBudget(user.clone()))
            .unwrap_or_else(|| panic_with_error!(&env, OverdraftError::BudgetNotFound));

        let category_budget = user_budget
            .categories
            .get(category.clone())
            .unwrap_or_else(|| panic_with_error!(&env, OverdraftError::CategoryNotFound));

        let remaining = category_budget.limit - category_budget.spent;
        let allowed = amount <= remaining;
        let exceeded_by = if allowed { 0 } else { amount - remaining };

        TransactionCheckResult {
            allowed,
            remaining,
            exceeded_by,
        }
    }

    /// Validates and processes a transaction against category budgets.
    /// This function BLOCKS transactions that would exceed the budget.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `user` - The user address making the transaction
    /// * `category` - Category name
    /// * `amount` - Transaction amount
    pub fn validate_transaction(env: Env, user: Address, category: Symbol, amount: i128) -> bool {
        user.require_auth();

        if amount <= 0 {
            panic_with_error!(&env, OverdraftError::InvalidAmount);
        }

        let mut user_budget: UserBudget = env
            .storage()
            .persistent()
            .get(&DataKey::UserBudget(user.clone()))
            .unwrap_or_else(|| panic_with_error!(&env, OverdraftError::BudgetNotFound));

        let mut category_budget = user_budget
            .categories
            .get(category.clone())
            .unwrap_or_else(|| panic_with_error!(&env, OverdraftError::CategoryNotFound));

        let remaining = category_budget.limit - category_budget.spent;

        // Check if transaction would exceed budget
        if amount > remaining {
            // Emit overdraft attempt event
            OverdraftEvents::overdraft_attempted(&env, &user, &category, amount, remaining);

            // Increment overdraft attempts counter
            let mut attempts: u64 = env
                .storage()
                .instance()
                .get(&DataKey::TotalOverdraftAttempts)
                .unwrap_or(0);
            attempts += 1;
            env.storage()
                .instance()
                .set(&DataKey::TotalOverdraftAttempts, &attempts);

            // Block the transaction
            panic_with_error!(&env, OverdraftError::OverdraftLimitExceeded);
        }

        // Update spent amount
        category_budget.spent += amount;
        let new_remaining = category_budget.limit - category_budget.spent;
        user_budget
            .categories
            .set(category.clone(), category_budget);
        user_budget.last_updated = env.ledger().timestamp();

        env.storage()
            .persistent()
            .set(&DataKey::UserBudget(user.clone()), &user_budget);

        // Emit validation success event
        OverdraftEvents::transaction_validated(&env, &user, &category, amount, new_remaining);

        true
    }

    /// Records spending without validation (for existing transactions).
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - The admin address
    /// * `user` - The user address
    /// * `category` - Category name
    /// * `amount` - Amount spent
    pub fn record_spending(
        env: Env,
        admin: Address,
        user: Address,
        category: Symbol,
        amount: i128,
    ) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        if amount <= 0 {
            panic_with_error!(&env, OverdraftError::InvalidAmount);
        }

        let mut user_budget: UserBudget = env
            .storage()
            .persistent()
            .get(&DataKey::UserBudget(user.clone()))
            .unwrap_or_else(|| panic_with_error!(&env, OverdraftError::BudgetNotFound));

        let mut category_budget = user_budget
            .categories
            .get(category.clone())
            .unwrap_or_else(|| panic_with_error!(&env, OverdraftError::CategoryNotFound));

        category_budget.spent += amount;
        user_budget
            .categories
            .set(category.clone(), category_budget);
        user_budget.last_updated = env.ledger().timestamp();

        env.storage()
            .persistent()
            .set(&DataKey::UserBudget(user.clone()), &user_budget);
    }

    /// Resets spent amounts for a user (e.g., at the start of a new period).
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - The admin address
    /// * `user` - The user address
    pub fn reset_spending(env: Env, admin: Address, user: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        let mut user_budget: UserBudget = env
            .storage()
            .persistent()
            .get(&DataKey::UserBudget(user.clone()))
            .unwrap_or_else(|| panic_with_error!(&env, OverdraftError::BudgetNotFound));

        // Reset all category spending to 0
        let mut new_categories = Map::<Symbol, CategoryBudget>::new(&env);
        for (_, category_budget) in user_budget.categories.iter() {
            let key = category_budget.name.clone();
            let reset_category = CategoryBudget {
                name: category_budget.name,
                limit: category_budget.limit,
                spent: 0,
            };
            new_categories.set(key, reset_category);
        }

        user_budget.categories = new_categories;
        user_budget.last_updated = env.ledger().timestamp();

        env.storage()
            .persistent()
            .set(&DataKey::UserBudget(user.clone()), &user_budget);
    }

    /// Gets the budget for a specific user.
    pub fn get_user_budget(env: Env, user: Address) -> Option<UserBudget> {
        env.storage().persistent().get(&DataKey::UserBudget(user))
    }

    /// Gets a specific category budget for a user.
    pub fn get_category_budget(
        env: Env,
        user: Address,
        category: Symbol,
    ) -> Option<CategoryBudget> {
        let user_budget = env
            .storage()
            .persistent()
            .get::<DataKey, UserBudget>(&DataKey::UserBudget(user))?;
        user_budget.categories.get(category)
    }

    /// Returns the total number of overdraft attempts.
    pub fn get_total_overdraft_attempts(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalOverdraftAttempts)
            .unwrap_or(0)
    }

    /// Sets the overdraft protection limit for a specific wallet.
    ///
    /// Only the admin may call this function. The limit represents the maximum
    /// negative balance (in stroop-equivalent units) that a wallet is permitted
    /// to enter before further transactions are blocked by the overdraft
    /// protection system.
    ///
    /// # Arguments
    /// * `env`   - The contract environment
    /// * `admin` - The admin address authorizing the change
    /// * `owner` - The wallet address whose limit is being configured
    /// * `limit` - The overdraft protection limit (must be ≥ 0)
    pub fn set_overdraft_limit(env: Env, admin: Address, owner: Address, limit: i128) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        if limit < 0 {
            panic_with_error!(&env, OverdraftError::InvalidAmount);
        }

        env.storage()
            .persistent()
            .set(&DataKey::OverdraftLimit(owner), &limit);
    }

    /// Returns the configured overdraft protection limit for `owner`.
    ///
    /// If no limit has been set for the address, this function returns `0`.
    ///
    /// # Arguments
    /// * `env`   - The contract environment
    /// * `owner` - The wallet address to query
    ///
    /// # Returns
    /// The overdraft protection limit as `i128`, or `0` if not configured.
    pub fn get_overdraft_limit(env: Env, owner: Address) -> i128 {
        env.storage()
            .persistent()
            .get::<DataKey, i128>(&DataKey::OverdraftLimit(owner))
            .unwrap_or(0)
    }

    /// Returns the admin address.
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized")
    }

    /// Internal helper to verify admin authority
    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");

        if *caller != admin {
            panic_with_error!(env, OverdraftError::Unauthorized);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger as _};

    #[test]
    fn test_initialize() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1000);

        let contract_id = env.register(OverdraftContract, ());
        let client = OverdraftContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        assert_eq!(client.get_admin(), admin);
        assert_eq!(client.get_total_overdraft_attempts(), 0);
    }

    #[test]
    fn test_set_category_budget() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1000);

        let contract_id = env.register(OverdraftContract, ());
        let client = OverdraftContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let user = Address::generate(&env);
        let category = symbol_short!("food");

        client.set_category_budget(&admin, &user, &category, &1000i128);

        let category_budget = client.get_category_budget(&user, &category);
        assert!(category_budget.is_some());
        assert_eq!(category_budget.unwrap().limit, 1000);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #4)")]
    fn test_overdraft_blocked() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1000);

        let contract_id = env.register(OverdraftContract, ());
        let client = OverdraftContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let user = Address::generate(&env);
        let category = symbol_short!("food");

        // Set budget limit of 100
        client.set_category_budget(&admin, &user, &category, &100i128);

        // Try to spend 150 (should fail)
        client.validate_transaction(&user, &category, &150i128);
    }

    #[test]
    fn test_successful_transaction() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1000);

        let contract_id = env.register(OverdraftContract, ());
        let client = OverdraftContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let user = Address::generate(&env);
        let category = symbol_short!("food");

        // Set budget limit of 500
        client.set_category_budget(&admin, &user, &category, &500i128);

        // Spend 200 (should succeed)
        let result = client.validate_transaction(&user, &category, &200i128);
        assert!(result);

        // Check remaining budget is 300
        let check = client.check_transaction(&user, &category, &100i128);
        assert!(check.allowed);
        assert_eq!(check.remaining, 300);
    }

    #[test]
    fn test_check_transaction() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1000);

        let contract_id = env.register(OverdraftContract, ());
        let client = OverdraftContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let user = Address::generate(&env);
        let category = symbol_short!("food");

        client.set_category_budget(&admin, &user, &category, &1000i128);

        let check = client.check_transaction(&user, &category, &500i128);
        assert!(check.allowed);
        assert_eq!(check.remaining, 1000);

        let check_over = client.check_transaction(&user, &category, &1500i128);
        assert!(!check_over.allowed);
        assert_eq!(check_over.exceeded_by, 500);
    }

    #[test]
    fn test_reset_spending() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1000);

        let contract_id = env.register(OverdraftContract, ());
        let client = OverdraftContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let user = Address::generate(&env);
        let category = symbol_short!("food");

        client.set_category_budget(&admin, &user, &category, &1000i128);

        // Spend 500
        client.validate_transaction(&user, &category, &500i128);

        // Verify spent
        let cat_budget = client.get_category_budget(&user, &category).unwrap();
        assert_eq!(cat_budget.spent, 500);

        // Reset spending
        client.reset_spending(&admin, &user);

        // Verify spent is 0
        let cat_budget_after = client.get_category_budget(&user, &category).unwrap();
        assert_eq!(cat_budget_after.spent, 0);
    }
}

#[cfg(test)]
mod overdraft_extra_tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger as _};

    fn setup(env: &Env) -> (soroban_sdk::Address, soroban_sdk::Address) {
        env.mock_all_auths();
        env.ledger().set_timestamp(1000);
        let contract_id = env.register(OverdraftContract, ());
        let client = OverdraftContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        client.initialize(&admin);
        (contract_id, admin)
    }

    #[test]
    fn test_admin_override_path() {
        let env = Env::default();
        let (contract_id, admin) = setup(&env);
        let client = OverdraftContractClient::new(&env, &contract_id);
        let user = Address::generate(&env);
        let cat = symbol_short!("rent");
        client.set_category_budget(&admin, &user, &cat, &1000i128);
        // Admin records spending without validation (override path)
        client.record_spending(&admin, &user, &cat, &1200i128);
        let budget = client.get_category_budget(&user, &cat).unwrap();
        assert_eq!(budget.spent, 1200);
    }

    #[test]
    fn test_overdraft_attempt_increments_counter() {
        let env = Env::default();
        let (contract_id, admin) = setup(&env);
        let client = OverdraftContractClient::new(&env, &contract_id);
        let user = Address::generate(&env);
        let cat = symbol_short!("food");
        client.set_category_budget(&admin, &user, &cat, &50i128);
        // check_transaction does not modify state, but verifies the budget would be exceeded
        let result = client.check_transaction(&user, &cat, &100i128);
        assert!(!result.allowed);
        assert_eq!(result.exceeded_by, 50);
        // Counter is only incremented by validate_transaction; no overdraft attempts yet
        assert_eq!(client.get_total_overdraft_attempts(), 0);
    }

    #[test]
    fn test_check_transaction_does_not_modify_state() {
        let env = Env::default();
        let (contract_id, admin) = setup(&env);
        let client = OverdraftContractClient::new(&env, &contract_id);
        let user = Address::generate(&env);
        let cat = symbol_short!("misc");
        client.set_category_budget(&admin, &user, &cat, &500i128);
        client.check_transaction(&user, &cat, &300i128);
        // Spent should still be 0 (check doesn't modify)
        let budget = client.get_category_budget(&user, &cat).unwrap();
        assert_eq!(budget.spent, 0);
    }

    /// Returns the correct limit after `set_overdraft_limit` is called.
    #[test]
    fn test_get_overdraft_limit_returns_correct_value() {
        let env = Env::default();
        let (contract_id, admin) = setup(&env);
        let client = OverdraftContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let limit: i128 = 5_000;

        client.set_overdraft_limit(&admin, &owner, &limit);

        assert_eq!(client.get_overdraft_limit(&owner), limit);
    }

    /// Returns 0 for an address that has never had a limit configured.
    #[test]
    fn test_get_overdraft_limit_returns_zero_for_unknown_address() {
        let env = Env::default();
        let (contract_id, _admin) = setup(&env);
        let client = OverdraftContractClient::new(&env, &contract_id);

        let unknown = Address::generate(&env);

        assert_eq!(client.get_overdraft_limit(&unknown), 0);
    }
}

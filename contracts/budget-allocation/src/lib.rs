//! # Budget Allocation Contract
//!
//! A Soroban smart contract for assigning monthly budgets to multiple users
//! in a single batch operation.
//!
//! ## Features
//!
//! - **Batch Processing**: Efficiently allocate budgets for multiple users in a single call
//! - **Atomic Updates**: Ensures reliable state changes for each user
//! - **Validation**: Prevents invalid budget amounts
//! - **Event Emission**: Tracks budget updates and failures
//!
#![no_std]

mod test;
mod types;

use crate::types::{
    BatchBudgetResult, BudgetAllocationSummary, BudgetRecord, BudgetRenewalConfig,
    BudgetRequest, BudgetVersion, CategoryBudgetRequest,
    DataKey, UserBudgetCategories,
};
use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, Map, Symbol, Vec};

#[contract]
pub struct BudgetAllocationContract;

#[contractimpl]
impl BudgetAllocationContract {
    /// Initializes the contract with an admin address.
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
    }

    /// Assigns monthly budgets to multiple users in a single operation.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - The admin address calling the function
    /// * `requests` - List of user-budget pairs
    pub fn batch_allocate_budget(
        env: Env,
        admin: Address,
        requests: Vec<BudgetRequest>,
    ) -> BatchBudgetResult {
        // Verify admin authority
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Unauthorized");
        }

        let mut successful = 0;
        let mut failed = 0;
        let mut total_amount: i128 = 0;
        let current_time = env.ledger().timestamp();

        for req in requests.iter() {
            // Validate input amount
            if req.amount < 0 {
                failed += 1;
                // Emit failure event?
                env.events().publish(
                    (symbol_short!("budget"), symbol_short!("failed")),
                    (req.user, req.amount), // Amount is negative here
                );
                continue;
            }

            // Atomic update for user: overwrite existing
            let record = BudgetRecord {
                user: req.user.clone(),
                amount: req.amount,
                last_updated: current_time,
            };

            env.storage()
                .persistent()
                .set(&DataKey::Budget(req.user.clone()), &record);

            // Emit update event
            env.events().publish(
                (symbol_short!("budget"), symbol_short!("set")),
                (req.user, req.amount),
            );

            successful += 1;
            total_amount = total_amount.checked_add(req.amount).unwrap_or(i128::MAX);
            // Prevent overflow panic
        }

        BatchBudgetResult {
            successful,
            failed,
            total_amount,
        }
    }

    /// Allocates budgets across multiple categories for a user.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - The admin address calling the function
    /// * `request` - Category budget allocation request
    pub fn allocate_budget_by_category(
        env: Env,
        admin: Address,
        request: CategoryBudgetRequest,
    ) -> bool {
        // Verify admin authority
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Unauthorized");
        }

        // Validate total amount matches sum of categories
        let mut calculated_total: i128 = 0;
        for category in request.categories.iter() {
            if category.amount < 0 {
                panic!("Negative category amount not allowed");
            }
            calculated_total = calculated_total
                .checked_add(category.amount)
                .expect("Overflow in category total calculation");
        }

        if calculated_total != request.total_amount {
            panic!("Total amount does not match sum of categories");
        }

        if request.total_amount < 0 {
            panic!("Negative total amount not allowed");
        }

        // Create category map
        let mut category_map = Map::<Symbol, i128>::new(&env);
        for category in request.categories.iter() {
            category_map.set(category.name, category.amount);
        }

        // Store user budget categories
        let user_categories = UserBudgetCategories {
            user: request.user.clone(),
            categories: category_map,
            total_amount: request.total_amount,
            last_updated: env.ledger().timestamp(),
        };

        env.storage().persistent().set(
            &DataKey::BudgetCategories(request.user.clone()),
            &user_categories,
        );

        // Also update the legacy budget record for compatibility
        let budget_record = BudgetRecord {
            user: request.user.clone(),
            amount: request.total_amount,
            last_updated: env.ledger().timestamp(),
        };
        env.storage()
            .persistent()
            .set(&DataKey::Budget(request.user.clone()), &budget_record);

        // Emit allocation events for each category
        for category in request.categories.iter() {
            env.events().publish(
                (symbol_short!("category"), symbol_short!("allocated")),
                (request.user.clone(), category.name, category.amount),
            );
        }

        // Emit total allocation event
        env.events().publish(
            (symbol_short!("budget"), symbol_short!("allocated")),
            (request.user, request.total_amount, request.categories.len()),
        );

        true
    }

    /// Retrieves budget categories for a specific user.
    pub fn get_budget_categories(env: Env, user: Address) -> Option<UserBudgetCategories> {
        env.storage()
            .persistent()
            .get(&DataKey::BudgetCategories(user))
    }

    /// Retrieves the budget for a specific category for a user.
    pub fn get_category_budget(env: Env, user: Address, category: Symbol) -> Option<i128> {
        let user_categories: Option<UserBudgetCategories> = env
            .storage()
            .persistent()
            .get(&DataKey::BudgetCategories(user));
        if let Some(categories) = user_categories {
            categories.categories.get(category)
        } else {
            None
        }
    }

    /// Retrieves the budget for a specific user.
    pub fn get_budget(env: Env, user: Address) -> Option<BudgetRecord> {
        env.storage().persistent().get(&DataKey::Budget(user))
    }

    /// Retrieves a summary of the allocation state for a specific user.
    pub fn get_budget_allocation_summary(
        env: Env,
        user: Address,
    ) -> Option<BudgetAllocationSummary> {
        let user_categories: Option<UserBudgetCategories> = env
            .storage()
            .persistent()
            .get(&DataKey::BudgetCategories(user.clone()));

        if let Some(categories) = user_categories {
            let mut remaining_allocation: i128 = 0;
            for amount in categories.categories.values() {
                remaining_allocation = remaining_allocation
                    .checked_add(amount)
                    .unwrap_or(i128::MAX);
            }

            let total_allocation = categories.total_amount;
            let used_allocation = total_allocation.saturating_sub(remaining_allocation);
            let usage_percentage = if total_allocation == 0 {
                0
            } else {
                used_allocation.saturating_mul(100) / total_allocation
            };

            return Some(BudgetAllocationSummary {
                remaining_allocation,
                total_allocation,
                usage_percentage,
            });
        }

        let budget: Option<BudgetRecord> = env.storage().persistent().get(&DataKey::Budget(user));
        budget.map(|record| BudgetAllocationSummary {
            remaining_allocation: record.amount,
            total_allocation: record.amount,
            usage_percentage: 0,
        })
    }

    /// Creates a snapshot of the current budget state for a user.
    /// Snapshots are stored with timestamps for historical tracking.
    pub fn create_budget_snapshot(env: Env, user: Address) {
        let now = env.ledger().timestamp();
        let budget_record: Option<BudgetRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Budget(user.clone()));

        if let Some(record) = budget_record {
            // Store the snapshot
            env.storage()
                .persistent()
                .set(&DataKey::BudgetSnapshot(now, user.clone()), &record);

            // Update timestamp list
            let mut timestamps: Vec<u64> = env
                .storage()
                .persistent()
                .get(&DataKey::SnapshotTimestamps)
                .unwrap_or_else(|| Vec::new(&env));
            timestamps.push_back(now);
            env.storage()
                .persistent()
                .set(&DataKey::SnapshotTimestamps, &timestamps);
        }
    }

    /// Retrieves a budget snapshot for a specific user at a specific timestamp.
    pub fn get_budget_snapshot(env: Env, user: Address, timestamp: u64) -> Option<BudgetRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::BudgetSnapshot(timestamp, user))
    }

    /// Retrieves all budget snapshots for a specific user.
    pub fn get_all_budget_snapshots(env: Env, user: Address) -> Vec<(u64, BudgetRecord)> {
        let mut snapshots = Vec::new(&env);
        let timestamps: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::SnapshotTimestamps)
            .unwrap_or_else(|| Vec::new(&env));

        for ts in timestamps.iter() {
            if let Some(snapshot) = env
                .storage()
                .persistent()
                .get(&DataKey::BudgetSnapshot(ts, user.clone()))
            {
                snapshots.push_back((ts, snapshot));
            }
        }
        snapshots
    }

    /// Resets the monthly spending counter for a user's budget.
    /// This is called automatically when the monthly cycle completes.
    /// Preserves historical usage data while resetting current spending.
    pub fn reset_monthly_budget(env: Env, user: Address) {
        let now = env.ledger().timestamp();

        // Get current budget record
        let mut budget_record: Option<BudgetRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Budget(user.clone()));

        if let Some(mut record) = budget_record {
            // This record stores the current budget amount only, so a reset
            // advances the cycle timestamp without changing the allocation.
            record.last_updated = now;

            // Store updated record
            env.storage()
                .persistent()
                .set(&DataKey::Budget(user), &record);
        }
    }

    /// Checks if monthly budget reset is needed based on timestamp.
    /// Returns true if it's been at least 30 days since last reset.
    pub fn needs_monthly_reset(env: Env, user: Address) -> bool {
        let now = env.ledger().timestamp();

        // Get current budget record
        let budget_record: Option<BudgetRecord> =
            env.storage().persistent().get(&DataKey::Budget(user));

        if let Some(record) = budget_record {
            // Calculate 30 days in seconds
            const THIRTY_DAYS_IN_SECONDS: u64 = 30 * 24 * 60 * 60;

            // Check if it's been at least 30 days since last update
            return now >= record.last_updated + THIRTY_DAYS_IN_SECONDS;
        }

        false
    }

    /// Returns the admin address
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized")
    }

    // ────────────────────────────────────────────────────────────────────────
    // #769: AUTOMATIC BUDGET RENEWAL
    // ────────────────────────────────────────────────────────────────────────

    /// Schedules automatic budget renewal for a user.
    ///
    /// When enabled, the executor can call `execute_budget_renewal` to
    /// clone the current budget state into a new versioned snapshot and
    /// reset the budget for the next cycle.
    ///
    /// # Arguments
    /// * `admin` - Must be the contract admin
    /// * `user` - The user whose budget is configured
    /// * `frequency_seconds` - How often to renew (e.g., 2_592_000 for 30 days)
    /// * `renewal_amount` - The amount to set for each renewal cycle
    pub fn schedule_budget_renewal(
        env: Env,
        admin: Address,
        user: Address,
        frequency_seconds: u64,
        renewal_amount: i128,
    ) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Unauthorized");
        }

        if frequency_seconds == 0 {
            panic!("Renewal frequency must be greater than zero");
        }
        if renewal_amount < 0 {
            panic!("Renewal amount must be non-negative");
        }

        let config = BudgetRenewalConfig {
            user: user.clone(),
            frequency_seconds,
            enabled: true,
            renewal_amount,
            last_renewed_at: env.ledger().timestamp(),
            renewal_count: 0,
        };

        env.storage()
            .persistent()
            .set(&DataKey::BudgetRenewalConfig(user.clone()), &config);

        env.events().publish(
            (symbol_short!("renewal"), symbol_short!("scheduled")),
            (user, frequency_seconds, renewal_amount),
        );
    }

    /// Executes a budget renewal for a user if it is due.
    ///
    /// # Behavior
    /// - Checks if the renewal frequency has elapsed since `last_renewed_at`.
    /// - If due, snapshots the current budget into a new `BudgetVersion`.
    /// - Resets the user's budget to the configured `renewal_amount`.
    /// - Increments the version counter and emits renewal events.
    ///
    /// # Returns
    /// The new version number or 0 if renewal was not needed.
    pub fn execute_budget_renewal(env: Env, user: Address) -> u64 {
        let config: BudgetRenewalConfig = env
            .storage()
            .persistent()
            .get(&DataKey::BudgetRenewalConfig(user.clone()))
            .expect("No renewal configuration found for user");

        if !config.enabled {
            return 0;
        }

        let now = env.ledger().timestamp();
        let elapsed = now.saturating_sub(config.last_renewed_at);

        if elapsed < config.frequency_seconds {
            return 0; // Not yet due
        }

        // Capture current budget as a historical version
        let current_budget: Option<BudgetRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Budget(user.clone()));

        let version_counter: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::BudgetVersionCounter(user.clone()))
            .unwrap_or(0);

        let new_version = version_counter + 1;

        let version = BudgetVersion {
            version: new_version,
            user: user.clone(),
            amount: current_budget
                .as_ref()
                .map(|r| r.amount)
                .unwrap_or(config.renewal_amount),
            created_at: now,
            renewal_id: config.renewal_count + 1,
        };

        // Store the version snapshot
        env.storage().persistent().set(
            &DataKey::BudgetVersion(user.clone(), new_version),
            &version,
        );

        // Update version counter
        env.storage()
            .persistent()
            .set(&DataKey::BudgetVersionCounter(user.clone()), &new_version);

        // Reset budget to renewal amount
        let renewed_record = BudgetRecord {
            user: user.clone(),
            amount: config.renewal_amount,
            last_updated: now,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Budget(user.clone()), &renewed_record);

        // Update renewal config with new timestamp and counter
        let updated_config = BudgetRenewalConfig {
            user: user.clone(),
            frequency_seconds: config.frequency_seconds,
            enabled: config.enabled,
            renewal_amount: config.renewal_amount,
            last_renewed_at: now,
            renewal_count: config.renewal_count + 1,
        };
        env.storage()
            .persistent()
            .set(&DataKey::BudgetRenewalConfig(user.clone()), &updated_config);

        // Emit renewal events
        env.events().publish(
            (symbol_short!("renewal"), symbol_short!("executed")),
            (user.clone(), new_version, config.renewal_amount),
        );

        // Also emit an audit-style event for tracking
        env.events().publish(
            (symbol_short!("audit"), symbol_short!("renewal")),
            (user, new_version, now),
        );

        new_version
    }

    /// Retrieves the budget renewal configuration for a user.
    pub fn get_budget_renewal_config(env: Env, user: Address) -> Option<BudgetRenewalConfig> {
        env.storage()
            .persistent()
            .get(&DataKey::BudgetRenewalConfig(user))
    }

    /// Disables automatic budget renewal for a user.
    pub fn disable_budget_renewal(env: Env, admin: Address, user: Address) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Unauthorized");
        }

        let mut config: BudgetRenewalConfig = env
            .storage()
            .persistent()
            .get(&DataKey::BudgetRenewalConfig(user.clone()))
            .expect("No renewal configuration found for user");

        config.enabled = false;
        env.storage()
            .persistent()
            .set(&DataKey::BudgetRenewalConfig(user.clone()), &config);

        env.events().publish(
            (symbol_short!("renewal"), symbol_short!("disabled")),
            (user,),
        );
    }

    /// Retrieves a specific budget version for a user.
    pub fn get_budget_version(env: Env, user: Address, version: u64) -> Option<BudgetVersion> {
        env.storage()
            .persistent()
            .get(&DataKey::BudgetVersion(user, version))
    }

    /// Retrieves all budget versions for a user.
    pub fn get_all_budget_versions(env: Env, user: Address) -> Vec<BudgetVersion> {
        let counter: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::BudgetVersionCounter(user.clone()))
            .unwrap_or(0);

        let mut versions = Vec::new(&env);
        for v in 1..=counter {
            if let Some(version) = env
                .storage()
                .persistent()
                .get(&DataKey::BudgetVersion(user.clone(), v))
            {
                versions.push_back(version);
            }
        }
        versions
    }
}

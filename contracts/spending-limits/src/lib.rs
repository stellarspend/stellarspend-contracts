//! # Spending Limits Contract
//!
//! A Soroban smart contract for managing batch spending limit updates
//! for multiple users simultaneously.
//!
//! ## Features
//!
//! - **Batch Processing**: Efficiently update spending limits for multiple users in a single call
//! - **Comprehensive Validation**: Validates limit amounts and user addresses
//! - **Event Emission**: Emits events for limit updates and batch processing
//! - **Error Handling**: Gracefully handles invalid inputs with detailed error codes
//! - **Optimized Storage**: Minimizes storage writes by batching operations
//! - **Partial Failure Support**: Invalid updates don't affect valid ones
//!
//! ## Optimization Strategies
//!
//! - Single-pass processing for O(n) complexity
//! - Minimized storage operations (batch writes at the end)
//! - Efficient data structures
//! - Batched event emissions

#![no_std]

mod cross_contract;
mod types;
mod validation;

use soroban_sdk::{contract, contractimpl, panic_with_error, Address, Env, Symbol, Vec};

use shared::authorization::{add_allowed_contract, is_allowed_contract, remove_allowed_contract};

pub use crate::types::{
    BatchLimitMetrics, BatchLimitResult, DataKey, EscalationConfig, ExceptionRule,
    LimitEvents, LimitStrategy, LimitUpdateResult, LimitsConfig, SpendingLimit,
    SpendingLimitRequest, MAX_BATCH_SIZE,
};
pub use crate::types::error_code;
use crate::validation::validate_limit_request;

// Add cross-contract imports for whitelist functionality
use crate::cross_contract::DataKey as CrossContractDataKey;

/// Error codes for the spending limits contract.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum SpendingLimitError {
    /// Contract not initialized
    NotInitialized = 1,
    /// Caller is not authorized
    Unauthorized = 2,
    /// Invalid batch data
    InvalidBatch = 3,
    /// Batch is empty
    EmptyBatch = 4,
    /// Batch exceeds maximum size
    BatchTooLarge = 5,
    /// Daily limit exceeded
    DailyLimitExceeded = 6,
    /// Hourly limit exceeded
    HourlyLimitExceeded = 7,
    /// Monthly limit exceeded
    MonthlyLimitExceeded = 8,
    /// Invalid spend amount
    InvalidAmount = 9,
    /// Category is not in the approved list
    CategoryNotApproved = 10,
}

impl From<SpendingLimitError> for soroban_sdk::Error {
    fn from(e: SpendingLimitError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

#[contract]
pub struct SpendingLimitsContract;

#[contractimpl]
impl SpendingLimitsContract {
    /// Initializes the contract with an admin address.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - The admin address that can manage the contract
    ///
    /// # Storage Optimization
    /// Uses a consolidated `LimitsConfig` struct instead of 4 separate
    /// storage entries, reducing initialization writes from 4 to 1.
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::LimitsConfig) {
            panic!("Contract already initialized");
        }

        let config = LimitsConfig {
            admin,
            last_batch_id: 0,
            total_limits_updated: 0,
            total_batches_processed: 0,
        };
        env.storage()
            .instance()
            .set(&DataKey::LimitsConfig, &config);
    }

    /// Updates monthly spending limits for multiple users in a batch.
    ///
    /// This is the main entry point for batch limit updates. It validates all requests,
    /// updates limits, emits events, and handles partial failures gracefully.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `caller` - The address calling this function (must be admin)
    /// * `requests` - Vector of spending limit update requests
    ///
    /// # Returns
    /// * `BatchLimitResult` - Result containing updated limits and metrics
    ///
    /// # Events Emitted
    /// * `batch_started` - When processing begins
    /// * `limit_updated` - For each successful limit update
    /// * `limit_update_failed` - For each failed limit update
    /// * `high_value_limit` - For limits with high values
    /// * `batch_completed` - When processing completes
    ///
    /// # Errors
    /// * `EmptyBatch` - If no requests provided
    /// * `BatchTooLarge` - If batch exceeds maximum size
    /// * `Unauthorized` - If caller is not admin
    pub fn batch_update_spending_limits(
        env: Env,
        caller: Address,
        requests: Vec<SpendingLimitRequest>,
    ) -> BatchLimitResult {
        // Verify authorization
        caller.require_auth();
        Self::require_admin(&env, &caller);

        // Validate batch size
        let request_count = requests.len();
        if request_count == 0 {
            panic_with_error!(&env, SpendingLimitError::EmptyBatch);
        }
        if request_count > MAX_BATCH_SIZE {
            panic_with_error!(&env, SpendingLimitError::BatchTooLarge);
        }

        // Get batch ID and increment from consolidated config
        let mut config: LimitsConfig = env
            .storage()
            .instance()
            .get(&DataKey::LimitsConfig)
            .expect("Contract not initialized");
        let batch_id: u64 = config.last_batch_id + 1;

        // Emit batch started event
        LimitEvents::batch_started(&env, batch_id, request_count);

        // Get current ledger timestamp
        let current_ledger = env.ledger().sequence() as u64;

        // Initialize result tracking
        let mut results: Vec<LimitUpdateResult> = Vec::new(&env);
        let mut successful_count: u32 = 0;
        let mut failed_count: u32 = 0;
        let mut total_limits_value: i128 = 0;

        // Process each request
        for request in requests.iter() {
            // Validate the request
            match validate_limit_request(&request) {
                Ok(()) => {
                    // Validation succeeded - update the limit
                    let limit = SpendingLimit {
                        user: request.user.clone(),
                        monthly_limit: request.monthly_limit,
                        daily_limit: request.daily_limit,
                        hourly_limit: request.hourly_limit,
                        reset_window_seconds: request.reset_window_seconds,
                        current_spending: 0, // Reset spending when updating limit
                        category: request.category.clone(),
                        updated_at: current_ledger,
                        is_active: true,
                        strategy: request.strategy.clone(),
                    };

                    // Accumulate metrics
                    total_limits_value = total_limits_value
                        .checked_add(request.monthly_limit)
                        .unwrap_or(i128::MAX);
                    successful_count += 1;

                    // Store the limit (optimized - one write per limit)
                    env.storage()
                        .persistent()
                        .set(&DataKey::SpendingLimit(request.user.clone()), &limit);

                    // Emit success event
                    LimitEvents::limit_updated(&env, batch_id, &limit);

                    // Emit high-value limit event if applicable (>= 1,000,000 XLM)
                    if request.monthly_limit >= 10_000_000_000_000_000 {
                        LimitEvents::high_value_limit(
                            &env,
                            batch_id,
                            &request.user,
                            request.monthly_limit,
                        );
                    }

                    results.push_back(LimitUpdateResult::Success(limit));
                }
                Err(error_code) => {
                    // Validation failed - record failure
                    failed_count += 1;

                    // Emit failure event
                    LimitEvents::limit_update_failed(&env, batch_id, &request.user, error_code);

                    results.push_back(LimitUpdateResult::Failure(request.user.clone(), error_code));
                }
            }
        }

        // Calculate average limit amount
        let avg_limit_amount = if successful_count > 0 {
            total_limits_value / successful_count as i128
        } else {
            0
        };

        // Create metrics
        let metrics = BatchLimitMetrics {
            total_requests: request_count,
            successful_updates: successful_count,
            failed_updates: failed_count,
            total_limits_value,
            avg_limit_amount,
            processed_at: current_ledger,
        };

        // Update consolidated config (single write instead of 4)
        config.last_batch_id = batch_id;
        config.total_limits_updated = config
            .total_limits_updated
            .checked_add(successful_count as u64)
            .unwrap_or(u64::MAX);
        config.total_batches_processed = config
            .total_batches_processed
            .checked_add(1)
            .unwrap_or(u64::MAX);
        env.storage()
            .instance()
            .set(&DataKey::LimitsConfig, &config);

        // Emit batch completed event
        LimitEvents::batch_completed(
            &env,
            batch_id,
            successful_count,
            failed_count,
            total_limits_value,
        );

        BatchLimitResult {
            batch_id,
            total_requests: request_count,
            successful: successful_count,
            failed: failed_count,
            results,
            metrics,
        }
    }

    /// Configures escalation rules for spending enforcement.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - Admin address (must authorize)
    /// * `small_threshold` - Amount below which spends are "small" (auto-approved)
    /// * `medium_threshold` - Amount below which spends are "medium" (logged)
    ///   Spends at or above this threshold are "large" and require admin approval
    /// * `enabled` - Whether escalation rules are active
    pub fn configure_escalation_rules(
        env: Env,
        admin: Address,
        small_threshold: i128,
        medium_threshold: i128,
        enabled: bool,
    ) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        if small_threshold <= 0 || medium_threshold <= small_threshold {
            panic_with_error!(&env, SpendingLimitError::InvalidAmount);
        }

        let config = EscalationConfig {
            small_threshold,
            medium_threshold,
            enabled,
        };

        env.storage()
            .instance()
            .set(&DataKey::EscalationConfig, &config);

        LimitEvents::escalation_configured(&env, small_threshold, medium_threshold, enabled);
    }

    /// Returns the current escalation configuration.
    pub fn get_escalation_config(env: Env) -> Option<EscalationConfig> {
        env.storage().instance().get(&DataKey::EscalationConfig)
    }

    /// Admin approves a large spend that was escalated.
    ///
    /// After approval, the spend is recorded against the user's limits
    /// as though it passed normal enforcement.
    pub fn approve_escalated_spend(env: Env, admin: Address, user: Address, amount: i128) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        // Record the spend against the user's limits
        let mut limit: SpendingLimit = match env
            .storage()
            .persistent()
            .get(&DataKey::SpendingLimit(user.clone()))
        {
            Some(l) => l,
            None => panic_with_error!(&env, SpendingLimitError::InvalidAmount),
        };

        limit.current_spending = limit
            .current_spending
            .checked_add(amount)
            .unwrap_or(i128::MAX);
        limit.updated_at = env.ledger().timestamp();

        env.storage()
            .persistent()
            .set(&DataKey::SpendingLimit(user.clone()), &limit);

        LimitEvents::escalation_approved(&env, &admin, &user, amount);
    }

    /// Enforces the configured daily and monthly spending limits for a user,
    /// including escalation tier checks.
    ///
    /// This function:
    /// - Tracks per-user daily and monthly totals using the current ledger timestamp.
    /// - Checks escalation tiers: small (auto), medium (logged), large (requires approval).
    /// - Rejects spends that would exceed either the derived daily limit or the stored
    ///   monthly limit.
    /// - Emits a `limit_exceeded` event when a violation occurs.
    /// - Emits an `escalation_triggered` event for medium/large spends.
    ///
    /// If no limit is configured for the user or the limit is inactive, the spend is
    /// allowed and no state is updated.
    pub fn enforce_spending_limit(env: Env, user: Address, amount: i128, category: Option<Symbol>) {
        // Validate amount
        if amount <= 0 {
            panic_with_error!(&env, SpendingLimitError::InvalidAmount);
        }

        // If a category is supplied and the user has an active exception for it,
        // bypass all limit checks and emit a bypass event.
        if let Some(ref cat) = category {
            if Self::is_exempt_internal(&env, &user, cat) {
                LimitEvents::exception_bypassed(&env, &user, amount, cat);
                return;
            }
        }

        // Check if destination is whitelisted (spending whitelist)
        // This prevents unauthorized destinations from receiving funds
        if !Self::is_destination_whitelisted_internal(&env, &user) {
            panic_with_error!(&env, SpendingLimitError::Unauthorized);
        }

        // Look up configured limit; if none, there is nothing to enforce.
        let mut limit: SpendingLimit = match env
            .storage()
            .persistent()
            .get(&DataKey::SpendingLimit(user.clone()))
        {
            Some(l) => l,
            None => return,
        };

        if !limit.is_active {
            return;
        }

        let now = env.ledger().timestamp();

        // Derive logical window/month identifiers from timestamp.
        const SECONDS_PER_HOUR: u64 = 3600;
        const SECONDS_PER_DAY: u64 = 86_400;
        const SECONDS_PER_MONTH: u64 = SECONDS_PER_DAY * 30;

        let hourly_window_id = if now == 0 {
            0
        } else {
            (now - 1) / SECONDS_PER_HOUR
        };
        let daily_window_id = if now == 0 {
            0
        } else {
            (now - 1) / SECONDS_PER_DAY
        };
        let month_id = if now == 0 {
            0
        } else {
            (now - 1) / SECONDS_PER_MONTH
        };

        // Load current window totals.
        let hourly_key = DataKey::HourlySpending(user.clone(), hourly_window_id);
        let daily_key = DataKey::DailySpending(user.clone(), daily_window_id);
        let monthly_key = DataKey::MonthlySpending(user.clone(), month_id);

        let current_hourly: i128 = env.storage().persistent().get(&hourly_key).unwrap_or(0);
        let current_daily: i128 = env.storage().persistent().get(&daily_key).unwrap_or(0);
        let current_monthly: i128 = env.storage().persistent().get(&monthly_key).unwrap_or(0);

        let new_hourly = current_hourly
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(&env, SpendingLimitError::InvalidBatch));
        let new_daily = current_daily
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(&env, SpendingLimitError::InvalidBatch));
        let new_monthly = current_monthly
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(&env, SpendingLimitError::InvalidBatch));

        let mut hourly_ok = true;
        let mut daily_ok = true;
        let mut monthly_ok = true;

        if new_hourly > limit.hourly_limit {
            hourly_ok = false;
        }
        if new_daily > limit.daily_limit {
            daily_ok = false;
        }
        if new_monthly > limit.monthly_limit {
            monthly_ok = false;
        }

        if !hourly_ok || !daily_ok || !monthly_ok {
            let remaining_hourly = if current_hourly >= limit.hourly_limit {
                0
            } else {
                limit.hourly_limit - current_hourly
            };
            let remaining_daily = if current_daily >= limit.daily_limit {
                0
            } else {
                limit.daily_limit - current_daily
            };
            let remaining_monthly = if current_monthly >= limit.monthly_limit {
                0
            } else {
                limit.monthly_limit - current_monthly
            };

            LimitEvents::limit_exceeded(
                &env,
                &user,
                amount,
                remaining_hourly,
                remaining_daily,
                remaining_monthly,
            );

            if !hourly_ok {
                panic_with_error!(&env, SpendingLimitError::HourlyLimitExceeded);
            } else if !daily_ok {
                panic_with_error!(&env, SpendingLimitError::DailyLimitExceeded);
            } else {
                panic_with_error!(&env, SpendingLimitError::MonthlyLimitExceeded);
            }
        }

        // If adaptive strategy is enabled and user is nearing their limit (>= 90%)
        // automatically increase the limit by 10% for future transactions.
        if limit.strategy == crate::types::LimitStrategy::Adaptive
            && new_monthly >= (limit.monthly_limit * 9 / 10)
        {
            let old_limit = limit.monthly_limit;
            let increment = limit.monthly_limit / 10;
            let proposed_limit = old_limit
                .checked_add(increment)
                .unwrap_or(crate::types::MAX_SPENDING_LIMIT);

            limit.monthly_limit = if proposed_limit > crate::types::MAX_SPENDING_LIMIT {
                crate::types::MAX_SPENDING_LIMIT
            } else {
                proposed_limit
            };

            if limit.monthly_limit != old_limit {
                LimitEvents::limit_adjusted(&env, &user, old_limit, limit.monthly_limit);
            }
        }

        // Persist updated totals.
        env.storage().persistent().set(&hourly_key, &new_hourly);
        env.storage().persistent().set(&daily_key, &new_daily);
        env.storage().persistent().set(&monthly_key, &new_monthly);

        // Keep the embedded "current_spending" and "updated_at" in sync with the
        // current logical month usage.
        limit.current_spending = new_monthly;
        limit.updated_at = now;
        env.storage()
            .persistent()
            .set(&DataKey::SpendingLimit(user), &limit);
    }

    /// Read-only cross-contract guard for composed policy engines.
    ///
    /// This mirrors `enforce_spending_limit` limit math and whitelist/category
    /// bypass behavior, but does not mutate usage. Callers that compose multiple
    /// policies can first check every rule, then call `enforce_spending_limit`
    /// only after the whole transaction is approved.
    pub fn check_spending_limit(
        env: Env,
        user: Address,
        amount: i128,
        category: Option<Symbol>,
    ) -> bool {
        if amount <= 0 {
            return false;
        }

        if let Some(ref cat) = category {
            if Self::is_exempt_internal(&env, &user, cat) {
                return true;
            }
        }

        if !Self::is_destination_whitelisted_internal(&env, &user) {
            return false;
        }

        let limit: SpendingLimit = match env
            .storage()
            .persistent()
            .get(&DataKey::SpendingLimit(user.clone()))
        {
            Some(l) => l,
            None => return true,
        };

        if !limit.is_active {
            return true;
        }

        let now = env.ledger().timestamp();
        const SECONDS_PER_HOUR: u64 = 3600;
        const SECONDS_PER_DAY: u64 = 86_400;
        const SECONDS_PER_MONTH: u64 = SECONDS_PER_DAY * 30;

        let hourly_window_id = if now == 0 {
            0
        } else {
            (now - 1) / SECONDS_PER_HOUR
        };
        let daily_window_id = if now == 0 {
            0
        } else {
            (now - 1) / SECONDS_PER_DAY
        };
        let month_id = if now == 0 {
            0
        } else {
            (now - 1) / SECONDS_PER_MONTH
        };

        let current_hourly: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::HourlySpending(user.clone(), hourly_window_id))
            .unwrap_or(0);
        let current_daily: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::DailySpending(user.clone(), daily_window_id))
            .unwrap_or(0);
        let current_monthly: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::MonthlySpending(user, month_id))
            .unwrap_or(0);

        let Some(new_hourly) = current_hourly.checked_add(amount) else {
            return false;
        };
        let Some(new_daily) = current_daily.checked_add(amount) else {
            return false;
        };
        let Some(new_monthly) = current_monthly.checked_add(amount) else {
            return false;
        };

        new_hourly <= limit.hourly_limit
            && new_daily <= limit.daily_limit
            && new_monthly <= limit.monthly_limit
    }

    /// Records an admin-approved emergency spending override on-chain.
    ///
    /// Requires the admin's signature (`require_auth`) so the override cannot be
    /// performed without explicit approval, and emits an auditable event so the
    /// bypass is preserved in the on-chain audit trail.
    pub fn emergency_override(env: Env, admin: Address, user: Address, amount: i128) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        if amount <= 0 {
            panic_with_error!(&env, SpendingLimitError::InvalidAmount);
        }

        env.events().publish(
            (
                soroban_sdk::symbol_short!("limit"),
                soroban_sdk::symbol_short!("override"),
            ),
            (admin, user, amount, env.ledger().timestamp()),
        );
    }

    /// Retrieves a user's full spending limit record.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `user` - The user's address
    ///
    /// # Returns
    /// * `Option<SpendingLimit>` - The full limit record if found, `None` otherwise.
    pub fn get_spending_limit_details(env: Env, user: Address) -> Option<SpendingLimit> {
        env.storage()
            .persistent()
            .get(&DataKey::SpendingLimit(user))
    }

    /// Returns the current spending limit for a given address as a scalar.
    ///
    /// This is the read-only view requested in [#961]: it returns the configured
    /// monthly spending limit (in stroops) for `owner`, or `0` when no limit has
    /// been set for that address (documented default). Callers that need the full
    /// record (reset window, category, activity state) should use
    /// [`Self::get_spending_limit_details`].
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `owner` - The address whose spending limit to query
    ///
    /// # Returns
    /// * `i128` - The current monthly spending limit, or `0` if none is set.
    pub fn get_spending_limit(env: Env, owner: Address) -> i128 {
        env.storage()
            .persistent()
            .get::<DataKey, SpendingLimit>(&DataKey::SpendingLimit(owner))
            .map(|limit| limit.monthly_limit)
            .unwrap_or(0)
    }

    /// Returns the admin address.
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get::<DataKey, LimitsConfig>(&DataKey::LimitsConfig)
            .expect("Contract not initialized")
            .admin
    }

    /// Updates the admin address.
    pub fn set_admin(env: Env, current_admin: Address, new_admin: Address) {
        current_admin.require_auth();
        Self::require_admin(&env, &current_admin);

        let mut config: LimitsConfig = env
            .storage()
            .instance()
            .get(&DataKey::LimitsConfig)
            .expect("Contract not initialized");
        config.admin = new_admin;
        env.storage()
            .instance()
            .set(&DataKey::LimitsConfig, &config);
    }

    /// Adds a destination address to the spending whitelist.
    /// Only admin can call this method.
    pub fn whitelist_destination(env: Env, caller: Address, destination: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let key = CrossContractDataKey::Whitelist(destination.clone());
        add_allowed_contract(&env, &key);
    }

    /// Removes a destination address from the spending whitelist.
    /// Only admin can call this method.
    pub fn remove_from_whitelist(env: Env, caller: Address, destination: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let key = CrossContractDataKey::Whitelist(destination.clone());
        remove_allowed_contract(&env, &key);
    }

    /// Checks if a destination address is whitelisted for receiving funds.
    /// This is a public read-only method that can be called by anyone.
    pub fn is_destination_whitelisted(env: Env, destination: Address) -> bool {
        // Use the same whitelist storage pattern as cross-contract module
        // Check if destination is in whitelist
        env.storage()
            .persistent()
            .has(&CrossContractDataKey::Whitelist(destination.clone()))
    }

    /// Returns the last created batch ID.
    pub fn get_last_batch_id(env: Env) -> u64 {
        env.storage()
            .instance()
            .get::<DataKey, LimitsConfig>(&DataKey::LimitsConfig)
            .map(|c| c.last_batch_id)
            .unwrap_or(0)
    }

    /// Returns the total number of limits updated.
    pub fn get_total_limits_updated(env: Env) -> u64 {
        env.storage()
            .instance()
            .get::<DataKey, LimitsConfig>(&DataKey::LimitsConfig)
            .map(|c| c.total_limits_updated)
            .unwrap_or(0)
    }

    /// Returns the total number of batches processed.
    pub fn get_total_batches_processed(env: Env) -> u64 {
        env.storage()
            .instance()
            .get::<DataKey, LimitsConfig>(&DataKey::LimitsConfig)
            .map(|c| c.total_batches_processed)
            .unwrap_or(0)
    }

    /// Adds a category to the admin-approved exception allow-list.
    ///
    /// Only categories in this list can be used in exception rules. Admin only.
    pub fn add_approved_category(env: Env, caller: Address, category: Symbol) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let mut categories: Vec<Symbol> = env
            .storage()
            .instance()
            .get(&DataKey::ApprovedCategories)
            .unwrap_or(Vec::new(&env));

        if !categories.contains(&category) {
            categories.push_back(category.clone());
            env.storage()
                .instance()
                .set(&DataKey::ApprovedCategories, &categories);
            LimitEvents::approved_category_added(&env, &category);
        }
    }

    /// Removes a category from the admin-approved exception allow-list. Admin only.
    pub fn remove_approved_category(env: Env, caller: Address, category: Symbol) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let categories: Vec<Symbol> = env
            .storage()
            .instance()
            .get(&DataKey::ApprovedCategories)
            .unwrap_or(Vec::new(&env));

        let mut new_categories: Vec<Symbol> = Vec::new(&env);
        let mut found = false;
        for cat in categories.iter() {
            if cat == category {
                found = true;
            } else {
                new_categories.push_back(cat);
            }
        }

        if found {
            env.storage()
                .instance()
                .set(&DataKey::ApprovedCategories, &new_categories);
            LimitEvents::approved_category_removed(&env, &category);
        }
    }

    /// Returns all admin-approved exception categories.
    pub fn get_approved_categories(env: Env) -> Vec<Symbol> {
        env.storage()
            .instance()
            .get(&DataKey::ApprovedCategories)
            .unwrap_or(Vec::new(&env))
    }

    /// Grants a spending limit exception to a user for a specific approved category.
    ///
    /// When `enforce_spending_limit` is called with this category for this user,
    /// the normal daily/monthly checks are skipped. Admin only.
    ///
    /// # Errors
    /// * `CategoryNotApproved` - if the category is not in the approved list
    pub fn add_exception(env: Env, caller: Address, user: Address, category: Symbol) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        // Verify the category is approved before granting exception
        let categories: Vec<Symbol> = env
            .storage()
            .instance()
            .get(&DataKey::ApprovedCategories)
            .unwrap_or(Vec::new(&env));

        if !categories.contains(&category) {
            panic_with_error!(&env, SpendingLimitError::CategoryNotApproved);
        }

        let rule = ExceptionRule {
            user: user.clone(),
            category: category.clone(),
            created_at: env.ledger().sequence() as u64,
            is_active: true,
        };

        env.storage().persistent().set(
            &DataKey::ExceptionRule(user.clone(), category.clone()),
            &rule,
        );

        LimitEvents::exception_added(&env, &user, &category);
    }

    /// Removes an active spending limit exception for a user+category pair. Admin only.
    pub fn remove_exception(env: Env, caller: Address, user: Address, category: Symbol) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let key = DataKey::ExceptionRule(user.clone(), category.clone());
        if env.storage().persistent().has(&key) {
            env.storage().persistent().remove(&key);
            LimitEvents::exception_removed(&env, &user, &category);
        }
    }

    /// Returns the exception rule for a user+category pair, if one exists.
    pub fn get_exception(env: Env, user: Address, category: Symbol) -> Option<ExceptionRule> {
        env.storage()
            .persistent()
            .get(&DataKey::ExceptionRule(user, category))
    }

    /// Returns `true` if the user has an active exception for the given category.
    pub fn is_exempt(env: Env, user: Address, category: Symbol) -> bool {
        Self::is_exempt_internal(&env, &user, &category)
    }

    /// Internal helper: checks exemption without consuming Env.
    fn is_exempt_internal(env: &Env, user: &Address, category: &Symbol) -> bool {
        match env
            .storage()
            .persistent()
            .get::<DataKey, ExceptionRule>(&DataKey::ExceptionRule(user.clone(), category.clone()))
        {
            Some(rule) => rule.is_active,
            None => false,
        }
    }

    // Internal helper to verify admin
    fn require_admin(env: &Env, caller: &Address) {
        let config: LimitsConfig = env
            .storage()
            .instance()
            .get(&DataKey::LimitsConfig)
            .expect("Contract not initialized");

        if *caller != config.admin {
            panic_with_error!(env, SpendingLimitError::Unauthorized);
        }
    }

    /// Internal helper for destination whitelist checks without consuming Env.
    fn is_destination_whitelisted_internal(env: &Env, destination: &Address) -> bool {
        let key = CrossContractDataKey::Whitelist(destination.clone());
        // Use the same whitelist storage pattern as cross-contract module
        // Check if destination is in whitelist
        is_allowed_contract(&env, &key)
    }

    pub fn override_spending_limit(
        env: Env,
        admin: Address,
        user: Address,
        new_monthly_limit: i128,
    ) {
        admin.require_auth();

        Self::require_admin(&env, &admin);

        if new_monthly_limit <= 0 {
            panic_with_error!(&env, SpendingLimitError::InvalidAmount);
        }

        let mut limit: SpendingLimit = env
            .storage()
            .persistent()
            .get(&DataKey::SpendingLimit(user.clone()))
            .expect("Spending limit not found");

        let old_limit = limit.monthly_limit;

        limit.monthly_limit = new_monthly_limit;

        env.storage()
            .persistent()
            .set(&DataKey::SpendingLimit(user.clone()), &limit);

        LimitEvents::spending_limit_overridden(&env, &admin, &user, old_limit, new_monthly_limit);
    }
}

#[cfg(test)]
mod test;


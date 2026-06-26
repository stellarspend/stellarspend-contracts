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
//! - **Delegated Management**: Owners can authorize managers with granular permissions
//!
#![no_std]

mod storage;
mod types;

use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, symbol_short, Address, Env, Map,
    Symbol, Vec,
};

pub use storage::{
    BudgetCheckpoint, BudgetConfigVersion, BudgetFreeze, BudgetTemplate, CategoryBudget,
    CategoryTransfer, DataKey, SpendingWindow, UserBudget, DEFAULT_FREEZE_DURATION_SECONDS,
    RAPID_SPEND_THRESHOLD, RAPID_SPEND_WINDOW_SECONDS,
};

pub use types::Beneficiary;

use storage::{
    clear_budget_freeze, delete_template, get_budget_config_history, get_budget_config_version,
    get_budget_freeze, get_category_available, get_template, get_transfer,
    get_user_budget as load_user_budget, get_user_templates, get_user_transfers,
    increment_suspicious_count, is_budget_frozen, next_transfer_id, record_spend_timestamp,
    record_transfer, save_budget_config_version, save_template, set_budget_freeze, set_user_budget,
};

/// Deletion cooldown period in seconds (24 hours).
pub const DELETION_COOLDOWN_SECONDS: u64 = 86_400;

/// Error codes for the budget contract.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum BudgetError {
    NotInitialized = 1,
    Unauthorized = 2,
    InvalidAmount = 3,
    UserNotFound = 4,
    DeletionCooldownNotElapsed = 5,
    NoPendingDeletion = 6,
    BudgetNotFound = 7,
    CategoryNotFound = 8,
    InsufficientBalance = 9,
    SameCategory = 10,
    BudgetFrozen = 11,
    SuspiciousActivity = 12,
    NotABeneficiary = 13,
    InactivityPeriodNotElapsed = 14,
    InvalidPercentages = 15,
    CheckpointNotFound = 16,
    IntegrityCheckFailed = 17,
    BudgetExpired = 18,
    BudgetInactive = 19,
    /// Caller is not a delegated manager for this owner
    NotDelegated = 20,
    /// Requested amount exceeds the manager's granted permission limit
    ExceedsPermission = 21,
    /// Delegation exists but has been revoked (inactive)
    DelegationNotActive = 22,
}

impl From<BudgetError> for soroban_sdk::Error {
    fn from(e: BudgetError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

/// Budget record for a user.
/// Budget record for a user with multi-asset support.
#[derive(Clone, Debug)]
#[contracttype]
pub struct BudgetRecord {
    pub user: Address,
    pub amount: i128,
    pub asset: Option<Address>,
    pub last_updated: u64,
    pub expires_at: Option<u64>,
    pub is_active: bool,
    pub is_archived: bool,
}

/// Permission record for a delegated budget manager.
///
/// An owner grants a manager the ability to update the owner's budget up to
/// `max_amount`. The owner retains ultimate control and can always call
/// `update_budget` directly via the admin path or revoke the delegation.
#[derive(Clone, Debug)]
#[contracttype]
pub struct DelegationPermission {
    /// Maximum budget amount the manager may set (must be > 0)
    pub max_amount: i128,
    /// Ledger sequence when the delegation was created
    pub created_at: u64,
    /// Whether the delegation is currently active
    pub is_active: bool,
}

/// Pending deletion record with cooldown expiry timestamp.
#[derive(Clone, Debug)]
#[contracttype]
pub struct PendingDeletion {
    pub user: Address,
    pub cooldown_expiry: u64,
}

/// Events emitted by the budget contract.
pub struct BudgetEvents;

impl BudgetEvents {
    pub fn checkpoint_created(env: &Env, user: &Address, timestamp: u64) {
        env.events().publish(
            (symbol_short!("budget"), symbol_short!("chk_new")),
            (user.clone(), timestamp),
        );
    }

    pub fn budget_restored(env: &Env, user: &Address, timestamp: u64) {
        env.events().publish(
            (symbol_short!("budget"), symbol_short!("restored")),
            (user.clone(), timestamp),
        );
    }

    pub fn category_budget_set(env: &Env, user: &Address, category: &Symbol, limit: i128) {
        env.events().publish(
            (
                symbol_short!("budget"),
                symbol_short!("cat_set"),
                category.clone(),
            ),
            (user.clone(), limit),
        );
    }

    pub fn category_transfer(
        env: &Env,
        user: &Address,
        from: &Symbol,
        to: &Symbol,
        amount: i128,
        transfer_id: u64,
    ) {
        env.events().publish(
            (
                symbol_short!("budget"),
                symbol_short!("transfer"),
                transfer_id,
            ),
            (user.clone(), from.clone(), to.clone(), amount),
        );
    }

    pub fn spend_recorded(
        env: &Env,
        user: &Address,
        category: &Symbol,
        amount: i128,
        remaining: i128,
    ) {
        env.events().publish(
            (
                symbol_short!("budget"),
                symbol_short!("spent"),
                category.clone(),
            ),
            (user.clone(), amount, remaining),
        );
    }

    pub fn budget_frozen(env: &Env, user: &Address, frozen_at: u64, auto_unfreeze_at: u64) {
        env.events().publish(
            (symbol_short!("budget"), symbol_short!("frozen")),
            (user.clone(), frozen_at, auto_unfreeze_at),
        );
    }

    pub fn budget_unfrozen(env: &Env, user: &Address, unfrozen_at: u64) {
        env.events().publish(
            (symbol_short!("budget"), symbol_short!("unfrozen")),
            (user.clone(), unfrozen_at),
        );
    }

    pub fn ownership_transferred(
        env: &Env,
        old_owner: &Address,
        new_owner: &Address,
        timestamp: u64,
    ) {
        env.events().publish(
            (symbol_short!("budget"), symbol_short!("own_trsf")),
            (old_owner.clone(), new_owner.clone(), timestamp),
        );
    }

    pub fn beneficiaries_updated(env: &Env, user: &Address) {
        env.events().publish(
            (symbol_short!("budget"), symbol_short!("ben_upd")),
            user.clone(),
        );
    }

    pub fn inheritance_beneficiaries_updated(env: &Env, user: &Address) {
        env.events().publish(
            (symbol_short!("budget"), symbol_short!("inh_upd")),
            user.clone(),
        );
    }

    pub fn funds_distributed(
        env: &Env,
        owner: &Address,
        beneficiary: &Address,
        amount: i128,
        asset: &Option<Address>,
        timestamp: u64,
    ) {
        env.events().publish(
            (symbol_short!("budget"), symbol_short!("distrib")),
            (
                owner.clone(),
                beneficiary.clone(),
                amount,
                asset.clone(),
                timestamp,
            ),
        );
    }
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
        env.storage()
            .instance()
            .set(&DataKey::TotalAllocated, &0i128);
    }

    /// Updates a single user's budget with optional multi-asset support.
    pub fn update_budget(
        env: Env,
        admin: Address,
        user: Address,
        amount: i128,
        asset: Option<Address>,
    ) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        if amount <= 0 {
            panic_with_error!(&env, BudgetError::InvalidAmount);
        }

        let current_time = env.ledger().timestamp();

        let mut total_allocated: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalAllocated)
            .unwrap_or(0);

        let mut expires_at = None;
        let mut is_active = true;

        if let Some(old_record) = env
            .storage()
            .persistent()
            .get::<DataKey, BudgetRecord>(&DataKey::Budget(user.clone()))
        {
            total_allocated = total_allocated.checked_sub(old_record.amount).unwrap_or(0);
            expires_at = old_record.expires_at;
            is_active = old_record.is_active;
        }

        total_allocated = total_allocated.checked_add(amount).unwrap_or(i128::MAX);

        let record = BudgetRecord {
            user: user.clone(),
            amount,
            asset: asset.clone(),
            last_updated: current_time,
            expires_at,
            is_active,
            is_archived: false,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Budget(user.clone()), &record);
        if let Some(ref asset_addr) = asset {
            env.storage().persistent().set(
                &DataKey::BudgetAsset(user.clone(), asset_addr.clone()),
                &record,
            );
            let mut user_assets: Vec<Address> = env
                .storage()
                .persistent()
                .get(&DataKey::UserAssets(user.clone()))
                .unwrap_or(Vec::new(&env));
            if !user_assets.contains(asset_addr) {
                user_assets.push_back(asset_addr.clone());
                env.storage()
                    .persistent()
                    .set(&DataKey::UserAssets(user.clone()), &user_assets);
            }
        } else {
            env.storage()
                .persistent()
                .set(&DataKey::Budget(user.clone()), &record);
        }

        env.storage()
            .instance()
            .set(&DataKey::TotalAllocated, &total_allocated);

        storage::set_last_activity(&env, &user, current_time);

        // Record a version snapshot of the current category config (if any) so
        // the overall budget amount change is also captured in history.
        if let Some(user_budget) = storage::get_user_budget(&env, &user) {
            save_budget_config_version(&env, &user, &user_budget.categories, current_time);
        }

        env.events().publish(
            (symbol_short!("budget"), symbol_short!("updated")),
            (user, amount, current_time),
        );
    }

    /// Retrieves the budget record for a specific user.
    /// Sets or updates a category budget limit for a user.
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
            panic_with_error!(&env, BudgetError::InvalidAmount);
        }

        let now = env.ledger().timestamp();
        let mut budget = load_user_budget(&env, &user).unwrap_or(UserBudget {
            user: user.clone(),
            categories: Map::new(&env),
            last_updated: now,
        });

        let spent = budget
            .categories
            .get(category.clone())
            .map(|c| c.spent)
            .unwrap_or(0);

        budget.categories.set(
            category.clone(),
            CategoryBudget {
                name: category.clone(),
                limit,
                spent,
            },
        );
        budget.last_updated = now;
        set_user_budget(&env, &budget);

        save_budget_config_version(&env, &user, &budget.categories, now);
        storage::set_last_activity(&env, &user, now);

        BudgetEvents::category_budget_set(&env, &user, &category, limit);
    }

    fn assert_active_and_not_expired(env: &Env, user: &Address) {
        if let Some(mut record) = env
            .storage()
            .persistent()
            .get::<DataKey, BudgetRecord>(&DataKey::Budget(user.clone()))
        {
            if !record.is_active {
                panic_with_error!(env, BudgetError::BudgetInactive);
            }
            if let Some(expires_at) = record.expires_at {
                if env.ledger().timestamp() >= expires_at {
                    record.is_active = false;
                    env.storage()
                        .persistent()
                        .set(&DataKey::Budget(user.clone()), &record);
                    panic_with_error!(env, BudgetError::BudgetExpired);
                }
            }
        }
    }

    pub fn configure_expiration(env: Env, admin: Address, user: Address, expires_at: u64) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        let mut record = env
            .storage()
            .persistent()
            .get::<DataKey, BudgetRecord>(&DataKey::Budget(user.clone()))
            .unwrap_or_else(|| panic_with_error!(&env, BudgetError::UserNotFound));

        record.expires_at = Some(expires_at);
        env.storage()
            .persistent()
            .set(&DataKey::Budget(user.clone()), &record);
    }

    pub fn mark_inactive(env: Env, admin: Address, user: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        let mut record = env
            .storage()
            .persistent()
            .get::<DataKey, BudgetRecord>(&DataKey::Budget(user.clone()))
            .unwrap_or_else(|| panic_with_error!(&env, BudgetError::UserNotFound));

        record.is_active = false;
        env.storage()
            .persistent()
            .set(&DataKey::Budget(user.clone()), &record);
    }

    pub fn archive_budget(env: Env, admin: Address, user: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        let mut record = env
            .storage()
            .persistent()
            .get::<DataKey, BudgetRecord>(&DataKey::Budget(user.clone()))
            .unwrap_or_else(|| panic_with_error!(&env, BudgetError::UserNotFound));

        if record.is_archived {
            return;
        }

        record.is_archived = true;
        env.storage()
            .persistent()
            .remove(&DataKey::Budget(user.clone()));
        env.storage()
            .persistent()
            .set(&DataKey::ArchivedBudget(user.clone()), &record);

        env.events().publish(
            (symbol_short!("budget"), symbol_short!("archived")),
            (user, env.ledger().timestamp()),
        );
    }

    pub fn unarchive_budget(env: Env, admin: Address, user: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        let mut record = env
            .storage()
            .persistent()
            .get::<DataKey, BudgetRecord>(&DataKey::ArchivedBudget(user.clone()))
            .unwrap_or_else(|| panic_with_error!(&env, BudgetError::BudgetNotFound));

        if !record.is_archived {
            return;
        }

        record.is_archived = false;
        record.is_active = true;
        env.storage()
            .persistent()
            .remove(&DataKey::ArchivedBudget(user.clone()));
        env.storage()
            .persistent()
            .set(&DataKey::Budget(user.clone()), &record);

        env.events().publish(
            (symbol_short!("budget"), symbol_short!("unarchived")),
            (user, env.ledger().timestamp()),
        );
    }

    pub fn deactivate_if_expired(env: Env, user: Address) {
        if let Some(mut record) = env
            .storage()
            .persistent()
            .get::<DataKey, BudgetRecord>(&DataKey::Budget(user.clone()))
        {
            if let Some(expires_at) = record.expires_at {
                if env.ledger().timestamp() >= expires_at {
                    record.is_active = false;
                    env.storage()
                        .persistent()
                        .set(&DataKey::Budget(user.clone()), &record);
                }
            }
        }
    }

    pub fn transfer_between_categories(
        env: Env,
        user: Address,
        from_category: Symbol,
        to_category: Symbol,
        amount: i128,
    ) -> u64 {
        user.require_auth();
        Self::assert_not_frozen(&env, &user);
        Self::assert_active_and_not_expired(&env, &user);

        if amount <= 0 {
            panic_with_error!(&env, BudgetError::InvalidAmount);
        }
        if from_category == to_category {
            panic_with_error!(&env, BudgetError::SameCategory);
        }

        let mut budget = load_user_budget(&env, &user).unwrap_or_else(|| {
            panic_with_error!(&env, BudgetError::BudgetNotFound);
        });

        let from = budget
            .categories
            .get(from_category.clone())
            .unwrap_or_else(|| panic_with_error!(&env, BudgetError::CategoryNotFound));
        let available = get_category_available(&from);
        if available < amount {
            panic_with_error!(&env, BudgetError::InsufficientBalance);
        }

        let to = budget
            .categories
            .get(to_category.clone())
            .unwrap_or_else(|| panic_with_error!(&env, BudgetError::CategoryNotFound));

        budget.categories.set(
            from_category.clone(),
            CategoryBudget {
                name: from_category.clone(),
                limit: from.limit - amount,
                spent: from.spent,
            },
        );
        budget.categories.set(
            to_category.clone(),
            CategoryBudget {
                name: to_category.clone(),
                limit: to.limit + amount,
                spent: to.spent,
            },
        );
        budget.last_updated = env.ledger().timestamp();
        set_user_budget(&env, &budget);

        storage::set_last_activity(&env, &user, budget.last_updated);

        let transfer_id = next_transfer_id(&env);
        let transfer = CategoryTransfer {
            transfer_id,
            user: user.clone(),
            from_category,
            to_category,
            amount,
            timestamp: budget.last_updated,
        };
        record_transfer(&env, &transfer);
        BudgetEvents::category_transfer(
            &env,
            &user,
            &transfer.from_category,
            &transfer.to_category,
            amount,
            transfer_id,
        );

        transfer_id
    }

    /// Records spending from a category and detects rapid repeated spending.
    pub fn spend_from_category(env: Env, user: Address, category: Symbol, amount: i128) -> i128 {
        user.require_auth();
        Self::assert_not_frozen(&env, &user);
        Self::assert_active_and_not_expired(&env, &user);

        if amount <= 0 {
            panic_with_error!(&env, BudgetError::InvalidAmount);
        }

        let now = env.ledger().timestamp();
        let mut budget = load_user_budget(&env, &user).unwrap_or_else(|| {
            panic_with_error!(&env, BudgetError::BudgetNotFound);
        });

        let cat = budget
            .categories
            .get(category.clone())
            .unwrap_or_else(|| panic_with_error!(&env, BudgetError::CategoryNotFound));

        let available = get_category_available(&cat);
        if available < amount {
            panic_with_error!(&env, BudgetError::InsufficientBalance);
        }

        let updated = CategoryBudget {
            name: category.clone(),
            limit: cat.limit,
            spent: cat.spent + amount,
        };
        let remaining = get_category_available(&updated);

        budget.categories.set(category.clone(), updated);
        budget.last_updated = now;
        set_user_budget(&env, &budget);

        storage::set_last_activity(&env, &user, now);

        let recent_count = record_spend_timestamp(&env, &user, now);
        if recent_count >= RAPID_SPEND_THRESHOLD {
            Self::freeze_for_suspicious_activity(&env, &user, now);
        }

        BudgetEvents::spend_recorded(&env, &user, &category, amount, remaining);
        remaining
    }

    /// Manually unfreezes a user's budget. Callable by admin or the user.
    pub fn unfreeze_budget(env: Env, caller: Address, user: Address) {
        caller.require_auth();
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if caller != admin && caller != user {
            panic_with_error!(&env, BudgetError::Unauthorized);
        }

        if get_budget_freeze(&env, &user).is_some() {
            clear_budget_freeze(&env, &user);
            let now = env.ledger().timestamp();
            storage::set_last_activity(&env, &user, now);
            BudgetEvents::budget_unfrozen(&env, &user, now);
        }
    }

    /// Schedules a budget for deletion with a 24-hour cooldown.
    pub fn schedule_deletion(env: Env, admin: Address, user: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        if env
            .storage()
            .persistent()
            .get::<DataKey, BudgetRecord>(&DataKey::Budget(user.clone()))
            .is_none()
        {
            panic_with_error!(&env, BudgetError::UserNotFound);
        }

        let current_time = env.ledger().timestamp();
        let cooldown_expiry = current_time
            .checked_add(DELETION_COOLDOWN_SECONDS)
            .unwrap_or(u64::MAX);

        let pending = PendingDeletion {
            user: user.clone(),
            cooldown_expiry,
        };

        env.storage()
            .persistent()
            .set(&DataKey::PendingDeletion(user.clone()), &pending);

        env.events().publish(
            (symbol_short!("budget"), symbol_short!("del_sched")),
            (user, cooldown_expiry),
        );
    }

    /// Cancels a pending budget deletion.
    pub fn cancel_deletion(env: Env, admin: Address, user: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        if !env
            .storage()
            .persistent()
            .has(&DataKey::PendingDeletion(user.clone()))
        {
            panic_with_error!(&env, BudgetError::NoPendingDeletion);
        }

        env.storage()
            .persistent()
            .remove(&DataKey::PendingDeletion(user.clone()));

        env.events()
            .publish((symbol_short!("budget"), symbol_short!("del_canc")), user);
    }

    /// Executes a scheduled budget deletion after the cooldown period has elapsed.
    pub fn execute_deletion(env: Env, admin: Address, user: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        let pending: PendingDeletion = env
            .storage()
            .persistent()
            .get(&DataKey::PendingDeletion(user.clone()))
            .unwrap_or_else(|| panic_with_error!(&env, BudgetError::NoPendingDeletion));

        let current_time = env.ledger().timestamp();
        if current_time < pending.cooldown_expiry {
            panic_with_error!(&env, BudgetError::DeletionCooldownNotElapsed);
        }

        let mut old_amount = env
            .storage()
            .persistent()
            .get::<DataKey, BudgetRecord>(&DataKey::Budget(user.clone()))
            .map(|r| r.amount)
            .unwrap_or(0);

        let user_assets: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::UserAssets(user.clone()))
            .unwrap_or(Vec::new(&env));
        for asset in user_assets.iter() {
            if let Some(record) = env
                .storage()
                .persistent()
                .get::<DataKey, BudgetRecord>(&DataKey::BudgetAsset(user.clone(), asset.clone()))
            {
                old_amount = old_amount.checked_add(record.amount).unwrap_or(old_amount);
            }
            env.storage()
                .persistent()
                .remove(&DataKey::BudgetAsset(user.clone(), asset.clone()));
        }
        env.storage()
            .persistent()
            .remove(&DataKey::UserAssets(user.clone()));

        env.storage()
            .persistent()
            .remove(&DataKey::Budget(user.clone()));
        env.storage()
            .persistent()
            .remove(&DataKey::PendingDeletion(user.clone()));

        let total_allocated: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalAllocated)
            .unwrap_or(0);
        let new_total = total_allocated.checked_sub(old_amount).unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::TotalAllocated, &new_total);

        env.events().publish(
            (symbol_short!("budget"), symbol_short!("deleted")),
            (user, current_time),
        );
    }

    /// Returns remaining balance for a category (limit - spent).
    pub fn get_category_balance(env: Env, user: Address, category: Symbol) -> i128 {
        let budget = load_user_budget(&env, &user).unwrap_or_else(|| {
            panic_with_error!(&env, BudgetError::BudgetNotFound);
        });
        let cat = budget
            .categories
            .get(category)
            .unwrap_or_else(|| panic_with_error!(&env, BudgetError::CategoryNotFound));
        get_category_available(&cat)
    }

    /// Returns a user's full category budget configuration.
    pub fn get_user_budget(env: Env, user: Address) -> UserBudget {
        load_user_budget(&env, &user).unwrap_or_else(|| {
            panic_with_error!(&env, BudgetError::BudgetNotFound);
        })
    }

    /// Returns a single transfer record by ID.
    pub fn get_transfer(env: Env, transfer_id: u64) -> CategoryTransfer {
        get_transfer(&env, transfer_id).unwrap_or_else(|| {
            panic_with_error!(&env, BudgetError::BudgetNotFound);
        })
    }

    /// Returns transfer history for a user (most recent retained entries).
    pub fn get_transfer_history(env: Env, user: Address) -> Vec<CategoryTransfer> {
        get_user_transfers(&env, &user)
    }

    /// Returns the full budget configuration history for a user (oldest first).
    pub fn get_budget_history(env: Env, user: Address) -> Vec<BudgetConfigVersion> {
        get_budget_config_history(&env, &user)
    }

    /// Returns a specific budget configuration version for a user, or panics if not found.
    pub fn get_budget_version(env: Env, user: Address, version: u32) -> BudgetConfigVersion {
        get_budget_config_version(&env, &user, version).unwrap_or_else(|| {
            panic_with_error!(&env, BudgetError::BudgetNotFound);
        })
    }

    /// Returns whether the user's budget is currently frozen.
    pub fn is_frozen(env: Env, user: Address) -> bool {
        is_budget_frozen(&env, &user, env.ledger().timestamp())
    }

    /// Returns the current freeze state, if any.
    pub fn get_freeze_state(env: Env, user: Address) -> Option<BudgetFreeze> {
        get_budget_freeze(&env, &user)
    }

    /// Returns total suspicious-activity freeze events recorded.
    pub fn get_suspicious_activity_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::SuspiciousActivityCount)
            .unwrap_or(0)
    }

    /// Returns the pending deletion for a user, if one exists.
    pub fn get_pending_deletion(env: Env, user: Address) -> Option<PendingDeletion> {
        env.storage()
            .persistent()
            .get(&DataKey::PendingDeletion(user))
    }

    /// Retrieves the budget for a specific user (default/native asset).
    pub fn get_budget(env: Env, user: Address) -> Option<BudgetRecord> {
        env.storage()
            .persistent()
            .get::<DataKey, BudgetRecord>(&DataKey::Budget(user))
            .filter(|record| !record.is_archived)
    }

    /// Retrieves the archived budget for a specific user.
    pub fn get_archived_budget(env: Env, user: Address) -> Option<BudgetRecord> {
        env.storage()
            .persistent()
            .get::<DataKey, BudgetRecord>(&DataKey::ArchivedBudget(user))
    }

    /// Retrieves the budget for a specific user and asset.
    pub fn get_budget_by_asset(env: Env, user: Address, asset: Address) -> Option<BudgetRecord> {
        env.storage()
            .persistent()
            .get::<DataKey, BudgetRecord>(&DataKey::BudgetAsset(user.clone(), asset.clone()))
            .filter(|record| !record.is_archived)
    }

    /// Retrieves the archived budget for a specific user and asset.
    pub fn get_archived_budget_by_asset(env: Env, user: Address, asset: Address) -> Option<BudgetRecord> {
        env.storage()
            .persistent()
            .get::<DataKey, BudgetRecord>(&DataKey::ArchivedBudgetAsset(user, asset))
    }

    /// Returns all asset contract IDs for a user's multi-asset budgets.
    pub fn get_user_assets(env: Env, user: Address) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::UserAssets(user))
            .unwrap_or(Vec::new(&env))
    }

    /// Returns the admin address.
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized")
    }

    /// Returns the total allocated budget amount.
    pub fn get_total_allocated(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalAllocated)
            .unwrap_or(0)
    }

    // ─── Delegated Budget Management (#600) ────────────────────────────────────

    /// Grants a manager the ability to update the caller's (owner's) budget up to
    /// `max_amount`. The owner retains full control and can always call
    /// `update_budget` directly. A manager can only be granted by the owner
    /// themselves.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `owner` - The budget owner granting delegation
    /// * `manager` - The address being granted manager rights
    /// * `max_amount` - Maximum budget amount the manager may set (must be > 0)
    pub fn delegate_manager(env: Env, owner: Address, manager: Address, max_amount: i128) {
        owner.require_auth();

        if max_amount <= 0 {
            panic_with_error!(&env, BudgetError::InvalidAmount);
        }

        let perm = DelegationPermission {
            max_amount,
            created_at: env.ledger().sequence() as u64,
            is_active: true,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Delegation(owner.clone(), manager.clone()), &perm);

        // Track manager in owner's delegate list
        let mut delegates: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::OwnerDelegates(owner.clone()))
            .unwrap_or(Vec::new(&env));
        if !delegates.contains(&manager) {
            delegates.push_back(manager.clone());
            env.storage()
                .persistent()
                .set(&DataKey::OwnerDelegates(owner.clone()), &delegates);
        }

        env.events().publish(
            (symbol_short!("delegate"), symbol_short!("granted")),
            (owner, manager, max_amount),
        );
    }

    /// Revokes a manager's delegation. Owner only.
    ///
    /// After revocation, the manager can no longer call `delegated_update_budget`
    /// on behalf of this owner.
    pub fn revoke_manager(env: Env, owner: Address, manager: Address) {
        owner.require_auth();

        let key = DataKey::Delegation(owner.clone(), manager.clone());
        if let Some(mut perm) = env
            .storage()
            .persistent()
            .get::<DataKey, DelegationPermission>(&key)
        {
            perm.is_active = false;
            env.storage().persistent().set(&key, &perm);

            env.events().publish(
                (symbol_short!("delegate"), symbol_short!("revoked")),
                (owner, manager),
            );
        }
    }

    /// Returns the delegation permission for a specific owner+manager pair, if any.
    pub fn get_delegation(
        env: Env,
        owner: Address,
        manager: Address,
    ) -> Option<DelegationPermission> {
        env.storage()
            .persistent()
            .get(&DataKey::Delegation(owner, manager))
    }

    /// Returns all manager addresses that the owner has ever delegated to.
    pub fn get_owner_delegates(env: Env, owner: Address) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::OwnerDelegates(owner))
            .unwrap_or(Vec::new(&env))
    }

    /// Allows a delegated manager to update the owner's budget within the
    /// permission limit granted by the owner.
    ///
    /// Managers operate strictly within assigned permissions; the owner retains
    /// ultimate control and can update without restriction via `update_budget`.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `manager` - The delegated manager calling this function
    /// * `owner` - The budget owner on whose behalf the manager is acting
    /// * `amount` - The new budget amount (must be <= `max_amount` in permission)
    ///
    /// # Errors
    /// * `NotDelegated` - if no delegation exists from owner to manager
    /// * `DelegationNotActive` - if the delegation was revoked
    /// * `ExceedsPermission` - if amount exceeds the manager's granted max
    /// * `InvalidAmount` - if amount is zero or negative
    pub fn delegated_update_budget(env: Env, manager: Address, owner: Address, amount: i128) {
        manager.require_auth();

        if amount <= 0 {
            panic_with_error!(&env, BudgetError::InvalidAmount);
        }

        // Load and verify delegation
        let perm: DelegationPermission = match env
            .storage()
            .persistent()
            .get(&DataKey::Delegation(owner.clone(), manager.clone()))
        {
            Some(p) => p,
            None => panic_with_error!(&env, BudgetError::NotDelegated),
        };

        if !perm.is_active {
            panic_with_error!(&env, BudgetError::DelegationNotActive);
        }

        if amount > perm.max_amount {
            panic_with_error!(&env, BudgetError::ExceedsPermission);
        }

        let current_time = env.ledger().timestamp();

        let mut total_allocated: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalAllocated)
            .unwrap_or(0);

        let mut expires_at = None;
        let mut is_active = true;
        let mut asset = None;
        if let Some(old_record) = env
            .storage()
            .persistent()
            .get::<DataKey, BudgetRecord>(&DataKey::Budget(owner.clone()))
        {
            total_allocated = total_allocated.checked_sub(old_record.amount).unwrap_or(0);
            expires_at = old_record.expires_at;
            is_active = old_record.is_active;
            asset = old_record.asset;
        }

        total_allocated = total_allocated.checked_add(amount).unwrap_or(i128::MAX);

        let record = BudgetRecord {
            user: owner.clone(),
            amount,
            asset,
            last_updated: current_time,
            expires_at,
            is_active,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Budget(owner.clone()), &record);
        env.storage()
            .instance()
            .set(&DataKey::TotalAllocated, &total_allocated);

        env.events().publish(
            (symbol_short!("budget"), symbol_short!("delegated")),
            (owner, manager, amount, current_time),
        );
    }

    /// Sets the inactivity timeout for a user.
    pub fn set_inactivity_timeout(env: Env, user: Address, timeout: u64) {
        user.require_auth();
        storage::set_inactivity_timeout(&env, &user, timeout);
        storage::set_last_activity(&env, &user, env.ledger().timestamp());
    }

    /// Gets the inactivity timeout for a user.
    pub fn get_inactivity_timeout(env: Env, user: Address) -> u64 {
        storage::get_inactivity_timeout(&env, &user)
    }

    /// Gets the last activity timestamp for a user.
    pub fn get_last_activity(env: Env, user: Address) -> u64 {
        Self::get_last_activity_time(&env, &user)
    }

    /// Registers inheritance beneficiaries for ownership transfer.
    pub fn set_inheritance_bens(env: Env, user: Address, beneficiaries: Vec<Address>) {
        user.require_auth();
        storage::set_inheritance_beneficiaries(&env, &user, &beneficiaries);
        storage::set_last_activity(&env, &user, env.ledger().timestamp());
        BudgetEvents::inheritance_beneficiaries_updated(&env, &user);
    }

    /// Gets registered inheritance beneficiaries.
    pub fn get_inheritance_beneficiaries(env: Env, user: Address) -> Vec<Address> {
        storage::get_inheritance_beneficiaries(&env, &user)
    }

    /// Registers beneficiaries and their allocation percentages (must sum to 100%).
    pub fn register_beneficiaries(env: Env, user: Address, beneficiaries: Vec<Beneficiary>) {
        user.require_auth();

        if !beneficiaries.is_empty() {
            let mut sum: u32 = 0;
            for b in beneficiaries.iter() {
                if b.percentage == 0 {
                    panic_with_error!(&env, BudgetError::InvalidPercentages);
                }
                sum = sum
                    .checked_add(b.percentage)
                    .unwrap_or_else(|| panic_with_error!(&env, BudgetError::InvalidPercentages));
            }
            if sum != 100 {
                panic_with_error!(&env, BudgetError::InvalidPercentages);
            }
        }

        storage::set_beneficiaries(&env, &user, &beneficiaries);
        storage::set_last_activity(&env, &user, env.ledger().timestamp());
        BudgetEvents::beneficiaries_updated(&env, &user);
    }

    /// Gets registered beneficiaries with allocation percentages.
    pub fn get_beneficiaries(env: Env, user: Address) -> Vec<Beneficiary> {
        storage::get_beneficiaries(&env, &user)
    }

    /// Claims ownership of a budget if the owner has been inactive.
    pub fn claim_ownership(env: Env, beneficiary: Address, owner: Address) {
        beneficiary.require_auth();

        let inheritance = storage::get_inheritance_beneficiaries(&env, &owner);
        let mut is_beneficiary = false;
        for addr in inheritance.iter() {
            if addr == beneficiary {
                is_beneficiary = true;
                break;
            }
        }
        if !is_beneficiary {
            panic_with_error!(&env, BudgetError::NotABeneficiary);
        }

        let last_activity = Self::get_last_activity_time(&env, &owner);
        if last_activity == 0 {
            panic_with_error!(&env, BudgetError::BudgetNotFound);
        }

        let timeout = storage::get_inactivity_timeout(&env, &owner);
        let now = env.ledger().timestamp();
        if now < last_activity.checked_add(timeout).unwrap_or(u64::MAX) {
            panic_with_error!(&env, BudgetError::InactivityPeriodNotElapsed);
        }

        Self::transfer_budget_ownership(&env, &owner, &beneficiary);
        BudgetEvents::ownership_transferred(&env, &owner, &beneficiary, now);
    }

    /// Distributes remaining funds to registered percentage beneficiaries.
    pub fn distribute_remaining_funds(env: Env, caller: Address, owner: Address) {
        caller.require_auth();

        let last_activity = Self::get_last_activity_time(&env, &owner);
        if last_activity == 0 {
            panic_with_error!(&env, BudgetError::BudgetNotFound);
        }

        let timeout = storage::get_inactivity_timeout(&env, &owner);
        let now = env.ledger().timestamp();
        if now < last_activity.checked_add(timeout).unwrap_or(u64::MAX) {
            panic_with_error!(&env, BudgetError::InactivityPeriodNotElapsed);
        }

        let beneficiaries = storage::get_beneficiaries(&env, &owner);
        if beneficiaries.is_empty() {
            panic_with_error!(&env, BudgetError::NotABeneficiary);
        }

        let admin = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::Admin)
            .expect("Not initialized");
        let mut is_authorized = caller == admin;
        for b in beneficiaries.iter() {
            if b.address == caller {
                is_authorized = true;
                break;
            }
        }
        if !is_authorized {
            panic_with_error!(&env, BudgetError::Unauthorized);
        }

        for b in beneficiaries.iter() {
            let percentage = b.percentage;

            // 1. BudgetRecord (Default/Native asset)
            if let Some(owner_record) = env
                .storage()
                .persistent()
                .get::<DataKey, BudgetRecord>(&DataKey::Budget(owner.clone()))
            {
                let share = owner_record
                    .amount
                    .checked_mul(percentage as i128)
                    .unwrap_or(0)
                    / 100;
                if share > 0 {
                    if let Some(mut existing_record) = env
                        .storage()
                        .persistent()
                        .get::<DataKey, BudgetRecord>(&DataKey::Budget(b.address.clone()))
                    {
                        existing_record.amount = existing_record
                            .amount
                            .checked_add(share)
                            .unwrap_or(existing_record.amount);
                        existing_record.last_updated = now;
                        env.storage()
                            .persistent()
                            .set(&DataKey::Budget(b.address.clone()), &existing_record);
                    } else {
                        env.storage().persistent().set(
                            &DataKey::Budget(b.address.clone()),
                            &BudgetRecord {
                                user: b.address.clone(),
                                amount: share,
                                asset: None,
                                last_updated: now,
                                expires_at: None,
                                is_active: true,
                            },
                        );
                    }
                    BudgetEvents::funds_distributed(&env, &owner, &b.address, share, &None, now);
                }
            }

            // 2. BudgetAsset (Multi-asset budgets)
            let owner_assets = env
                .storage()
                .persistent()
                .get::<DataKey, Vec<Address>>(&DataKey::UserAssets(owner.clone()))
                .unwrap_or_else(|| Vec::new(&env));

            for asset in owner_assets.iter() {
                if let Some(owner_record) =
                    env.storage()
                        .persistent()
                        .get::<DataKey, BudgetRecord>(&DataKey::BudgetAsset(
                            owner.clone(),
                            asset.clone(),
                        ))
                {
                    let share = owner_record
                        .amount
                        .checked_mul(percentage as i128)
                        .unwrap_or(0)
                        / 100;
                    if share > 0 {
                        if let Some(mut existing_record) =
                            env.storage().persistent().get::<DataKey, BudgetRecord>(
                                &DataKey::BudgetAsset(b.address.clone(), asset.clone()),
                            )
                        {
                            existing_record.amount = existing_record
                                .amount
                                .checked_add(share)
                                .unwrap_or(existing_record.amount);
                            existing_record.last_updated = now;
                            env.storage().persistent().set(
                                &DataKey::BudgetAsset(b.address.clone(), asset.clone()),
                                &existing_record,
                            );
                        } else {
                            env.storage().persistent().set(
                                &DataKey::BudgetAsset(b.address.clone(), asset.clone()),
                                &BudgetRecord {
                                    user: b.address.clone(),
                                    amount: share,
                                    asset: Some(asset.clone()),
                                    last_updated: now,
                                    expires_at: None,
                                    is_active: true,
                                },
                            );
                        }

                        let mut b_assets = env
                            .storage()
                            .persistent()
                            .get::<DataKey, Vec<Address>>(&DataKey::UserAssets(b.address.clone()))
                            .unwrap_or_else(|| Vec::new(&env));
                        if !b_assets.contains(&asset) {
                            b_assets.push_back(asset.clone());
                            env.storage()
                                .persistent()
                                .set(&DataKey::UserAssets(b.address.clone()), &b_assets);
                        }

                        BudgetEvents::funds_distributed(
                            &env,
                            &owner,
                            &b.address,
                            share,
                            &Some(asset.clone()),
                            now,
                        );
                    }
                }
            }

            // 3. UserBudget (Category budgets)
            if let Some(owner_user_budget) = storage::get_user_budget(&env, &owner) {
                let mut b_user_budget =
                    storage::get_user_budget(&env, &b.address).unwrap_or_else(|| UserBudget {
                        user: b.address.clone(),
                        categories: Map::new(&env),
                        last_updated: now,
                    });

                for (_, cat) in owner_user_budget.categories.iter() {
                    let limit_share = cat.limit.checked_mul(percentage as i128).unwrap_or(0) / 100;
                    let spent_share = cat.spent.checked_mul(percentage as i128).unwrap_or(0) / 100;

                    if limit_share > 0 {
                        if let Some(mut existing_cat) =
                            b_user_budget.categories.get(cat.name.clone())
                        {
                            existing_cat.limit = existing_cat
                                .limit
                                .checked_add(limit_share)
                                .unwrap_or(existing_cat.limit);
                            existing_cat.spent = existing_cat
                                .spent
                                .checked_add(spent_share)
                                .unwrap_or(existing_cat.spent);
                            b_user_budget.categories.set(cat.name.clone(), existing_cat);
                        } else {
                            b_user_budget.categories.set(
                                cat.name.clone(),
                                CategoryBudget {
                                    name: cat.name.clone(),
                                    limit: limit_share,
                                    spent: spent_share,
                                },
                            );
                        }
                    }
                }

                b_user_budget.last_updated = now;
                storage::set_user_budget(&env, &b_user_budget);
            }
        }

        // Cleanup owner's budget and configuration completely after distribution
        env.storage()
            .persistent()
            .remove(&DataKey::Budget(owner.clone()));

        let owner_assets = env
            .storage()
            .persistent()
            .get::<DataKey, Vec<Address>>(&DataKey::UserAssets(owner.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        for asset in owner_assets.iter() {
            env.storage()
                .persistent()
                .remove(&DataKey::BudgetAsset(owner.clone(), asset));
        }
        env.storage()
            .persistent()
            .remove(&DataKey::UserAssets(owner.clone()));

        env.storage()
            .persistent()
            .remove(&DataKey::UserBudget(owner.clone()));
        storage::clear_budget_freeze(&env, &owner);
        env.storage()
            .persistent()
            .remove(&DataKey::SpendingWindow(owner.clone()));
        env.storage()
            .persistent()
            .remove(&DataKey::UserTransfers(owner.clone()));
        env.storage()
            .persistent()
            .remove(&DataKey::LastActivity(owner.clone()));
        env.storage()
            .persistent()
            .remove(&DataKey::InactivityTimeout(owner.clone()));
        env.storage()
            .persistent()
            .remove(&DataKey::InheritanceBeneficiaries(owner.clone()));
        env.storage()
            .persistent()
            .remove(&DataKey::Beneficiaries(owner.clone()));
    }

    fn get_last_activity_time(env: &Env, user: &Address) -> u64 {
        let stored = storage::get_last_activity(env, user);
        if stored > 0 {
            return stored;
        }
        if let Some(record) = env
            .storage()
            .persistent()
            .get::<DataKey, BudgetRecord>(&DataKey::Budget(user.clone()))
        {
            return record.last_updated;
        }
        if let Some(budget) = storage::get_user_budget(env, user) {
            return budget.last_updated;
        }
        0
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, Env};

    fn setup_test_contract() -> (Env, Address, BudgetContractClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(BudgetContract, ());
        let client = BudgetContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        (env, admin, client)
    }

    #[test]
    fn test_initialize() {
        let (_, admin, client) = setup_test_contract();
        assert_eq!(client.get_admin(), admin);
        assert_eq!(client.get_total_allocated(), 0);
    }

    #[test]
    #[should_panic(expected = "Already initialized")]
    fn test_initialize_twice_fails() {
        let (env, _, client) = setup_test_contract();
        let new_admin = Address::generate(&env);
        client.initialize(&new_admin);
    }

    #[test]
    fn test_update_budget() {
        let (env, admin, client) = setup_test_contract();
        let user = Address::generate(&env);

        client.update_budget(&admin, &user, &1_000_i128, &None);

        let record = client.get_budget(&user).unwrap();
        assert_eq!(record.amount, 1_000);
        assert_eq!(record.user, user);
    }

    #[test]
    fn test_total_allocated_tracks_updates() {
        let (env, admin, client) = setup_test_contract();
        let user1 = Address::generate(&env);
        let user2 = Address::generate(&env);

        client.update_budget(&admin, &user1, &500_i128, &None);
        client.update_budget(&admin, &user2, &300_i128, &None);

        assert_eq!(client.get_total_allocated(), 800);
    }

    // ─── Delegation tests (#600) ───────────────────────────────────────────────

    #[test]
    fn test_delegate_manager_grants_permission() {
        let (env, admin, client) = setup_test_contract();
        let owner = Address::generate(&env);
        let manager = Address::generate(&env);

        client.delegate_manager(&owner, &manager, &1_000_i128);

        let perm = client.get_delegation(&owner, &manager).unwrap();
        assert_eq!(perm.max_amount, 1_000);
        assert!(perm.is_active);
    }

    #[test]
    fn test_owner_delegates_list_updated() {
        let (env, _, client) = setup_test_contract();
        let owner = Address::generate(&env);
        let manager1 = Address::generate(&env);
        let manager2 = Address::generate(&env);

        client.delegate_manager(&owner, &manager1, &500_i128);
        client.delegate_manager(&owner, &manager2, &800_i128);

        let delegates = client.get_owner_delegates(&owner);
        assert_eq!(delegates.len(), 2);
        assert!(delegates.contains(&manager1));
        assert!(delegates.contains(&manager2));
    }

    #[test]
    fn test_delegated_update_budget_within_limit() {
        let (env, admin, client) = setup_test_contract();
        let owner = Address::generate(&env);
        let manager = Address::generate(&env);

        // Grant manager permission up to 500
        client.delegate_manager(&owner, &manager, &500_i128);

        // Manager sets owner's budget to 400 (within limit)
        client.delegated_update_budget(&manager, &owner, &400_i128);

        let record = client.get_budget(&owner).unwrap();
        assert_eq!(record.amount, 400);
        assert_eq!(record.user, owner);
    }

    #[test]
    #[should_panic]
    fn test_delegated_update_budget_exceeds_permission_panics() {
        let (env, _, client) = setup_test_contract();
        let owner = Address::generate(&env);
        let manager = Address::generate(&env);

        client.delegate_manager(&owner, &manager, &500_i128);

        // Amount 501 exceeds max_amount 500 — must panic
        client.delegated_update_budget(&manager, &owner, &501_i128);
    }

    #[test]
    #[should_panic]
    fn test_delegated_update_budget_without_delegation_panics() {
        let (env, _, client) = setup_test_contract();
        let owner = Address::generate(&env);
        let rogue = Address::generate(&env);

        // No delegation was ever granted to rogue
        client.delegated_update_budget(&rogue, &owner, &100_i128);
    }

    #[test]
    #[should_panic]
    fn test_delegated_update_budget_after_revoke_panics() {
        let (env, _, client) = setup_test_contract();
        let owner = Address::generate(&env);
        let manager = Address::generate(&env);

        client.delegate_manager(&owner, &manager, &500_i128);
        client.revoke_manager(&owner, &manager);

        // Delegation was revoked — must panic
        client.delegated_update_budget(&manager, &owner, &100_i128);
    }

    #[test]
    fn test_revoke_manager_marks_inactive() {
        let (env, _, client) = setup_test_contract();
        let owner = Address::generate(&env);
        let manager = Address::generate(&env);

        client.delegate_manager(&owner, &manager, &500_i128);
        assert!(client.get_delegation(&owner, &manager).unwrap().is_active);

        client.revoke_manager(&owner, &manager);
        assert!(!client.get_delegation(&owner, &manager).unwrap().is_active);
    }

    #[test]
    fn test_owner_retains_control_after_delegation() {
        let (env, admin, client) = setup_test_contract();
        let owner = Address::generate(&env);
        let manager = Address::generate(&env);

        // Grant manager limited permission
        client.delegate_manager(&owner, &manager, &100_i128);

        // Admin (ultimate control) can still set any amount, including above manager's limit
        client.update_budget(&admin, &owner, &999_999_i128, &None);

        let record = client.get_budget(&owner).unwrap();
        assert_eq!(record.amount, 999_999);
    }

    #[test]
    fn test_delegate_duplicate_manager_is_idempotent() {
        let (env, _, client) = setup_test_contract();
        let owner = Address::generate(&env);
        let manager = Address::generate(&env);

        client.delegate_manager(&owner, &manager, &300_i128);
        client.delegate_manager(&owner, &manager, &300_i128);

        // Should only appear once in the delegates list
        assert_eq!(client.get_owner_delegates(&owner).len(), 1);
    }

    #[test]
    #[should_panic]
    fn test_delegate_zero_max_amount_panics() {
        let (env, _, client) = setup_test_contract();
        let owner = Address::generate(&env);
        let manager = Address::generate(&env);

        client.delegate_manager(&owner, &manager, &0_i128);
    }
}

#[contractimpl]
impl BudgetContract {
    fn transfer_budget_ownership(env: &Env, old_owner: &Address, new_owner: &Address) {
        let now = env.ledger().timestamp();

        // 1. BudgetRecord (Default/Native asset)
        if let Some(mut record) = env
            .storage()
            .persistent()
            .get::<DataKey, BudgetRecord>(&DataKey::Budget(old_owner.clone()))
        {
            record.user = new_owner.clone();
            record.last_updated = now;

            if let Some(mut existing_record) = env
                .storage()
                .persistent()
                .get::<DataKey, BudgetRecord>(&DataKey::Budget(new_owner.clone()))
            {
                existing_record.amount = existing_record
                    .amount
                    .checked_add(record.amount)
                    .unwrap_or(existing_record.amount);
                existing_record.last_updated = now;
                env.storage()
                    .persistent()
                    .set(&DataKey::Budget(new_owner.clone()), &existing_record);
            } else {
                env.storage()
                    .persistent()
                    .set(&DataKey::Budget(new_owner.clone()), &record);
            }
            env.storage()
                .persistent()
                .remove(&DataKey::Budget(old_owner.clone()));
        }

        // 2. BudgetAsset (Multi-asset budgets)
        let old_assets = env
            .storage()
            .persistent()
            .get::<DataKey, Vec<Address>>(&DataKey::UserAssets(old_owner.clone()))
            .unwrap_or_else(|| Vec::new(env));

        let mut new_assets = env
            .storage()
            .persistent()
            .get::<DataKey, Vec<Address>>(&DataKey::UserAssets(new_owner.clone()))
            .unwrap_or_else(|| Vec::new(env));

        for asset in old_assets.iter() {
            if let Some(mut record) =
                env.storage()
                    .persistent()
                    .get::<DataKey, BudgetRecord>(&DataKey::BudgetAsset(
                        old_owner.clone(),
                        asset.clone(),
                    ))
            {
                record.user = new_owner.clone();
                record.last_updated = now;

                if let Some(mut existing_record) = env
                    .storage()
                    .persistent()
                    .get::<DataKey, BudgetRecord>(&DataKey::BudgetAsset(
                        new_owner.clone(),
                        asset.clone(),
                    ))
                {
                    existing_record.amount = existing_record
                        .amount
                        .checked_add(record.amount)
                        .unwrap_or(existing_record.amount);
                    existing_record.last_updated = now;
                    env.storage().persistent().set(
                        &DataKey::BudgetAsset(new_owner.clone(), asset.clone()),
                        &existing_record,
                    );
                } else {
                    env.storage().persistent().set(
                        &DataKey::BudgetAsset(new_owner.clone(), asset.clone()),
                        &record,
                    );
                }
                env.storage()
                    .persistent()
                    .remove(&DataKey::BudgetAsset(old_owner.clone(), asset.clone()));
            }

            if !new_assets.contains(&asset) {
                new_assets.push_back(asset.clone());
            }
        }

        if new_assets.len() > 0 {
            env.storage()
                .persistent()
                .set(&DataKey::UserAssets(new_owner.clone()), &new_assets);
        }
        env.storage()
            .persistent()
            .remove(&DataKey::UserAssets(old_owner.clone()));

        // 3. UserBudget (Category budgets)
        if let Some(mut old_user_budget) = storage::get_user_budget(env, old_owner) {
            old_user_budget.user = new_owner.clone();
            old_user_budget.last_updated = now;

            if let Some(mut new_user_budget) = storage::get_user_budget(env, new_owner) {
                for (_, cat) in old_user_budget.categories.iter() {
                    if let Some(mut existing_cat) = new_user_budget.categories.get(cat.name.clone())
                    {
                        existing_cat.limit = existing_cat
                            .limit
                            .checked_add(cat.limit)
                            .unwrap_or(existing_cat.limit);
                        existing_cat.spent = existing_cat
                            .spent
                            .checked_add(cat.spent)
                            .unwrap_or(existing_cat.spent);
                        new_user_budget
                            .categories
                            .set(cat.name.clone(), existing_cat);
                    } else {
                        new_user_budget
                            .categories
                            .set(cat.name.clone(), cat.clone());
                    }
                }
                new_user_budget.last_updated = now;
                storage::set_user_budget(env, &new_user_budget);
            } else {
                storage::set_user_budget(env, &old_user_budget);
            }
            env.storage()
                .persistent()
                .remove(&DataKey::UserBudget(old_owner.clone()));
        }

        // 4. BudgetFreeze
        if let Some(freeze) = storage::get_budget_freeze(env, old_owner) {
            storage::set_budget_freeze(env, new_owner, &freeze);
            storage::clear_budget_freeze(env, old_owner);
        }

        // 5. SpendingWindow
        if let Some(window) = env
            .storage()
            .persistent()
            .get::<DataKey, SpendingWindow>(&DataKey::SpendingWindow(old_owner.clone()))
        {
            env.storage()
                .persistent()
                .set(&DataKey::SpendingWindow(new_owner.clone()), &window);
            env.storage()
                .persistent()
                .remove(&DataKey::SpendingWindow(old_owner.clone()));
        }

        // 6. UserTransfers
        if let Some(transfers) = env
            .storage()
            .persistent()
            .get::<DataKey, Vec<u64>>(&DataKey::UserTransfers(old_owner.clone()))
        {
            env.storage()
                .persistent()
                .set(&DataKey::UserTransfers(new_owner.clone()), &transfers);
            env.storage()
                .persistent()
                .remove(&DataKey::UserTransfers(old_owner.clone()));
        }

        // 7. Cleanup old configurations
        env.storage()
            .persistent()
            .remove(&DataKey::LastActivity(old_owner.clone()));
        env.storage()
            .persistent()
            .remove(&DataKey::InactivityTimeout(old_owner.clone()));
        env.storage()
            .persistent()
            .remove(&DataKey::InheritanceBeneficiaries(old_owner.clone()));
        env.storage()
            .persistent()
            .remove(&DataKey::Beneficiaries(old_owner.clone()));

        // Initialize new owner activity
        storage::set_last_activity(env, new_owner, now);
    }

    fn freeze_for_suspicious_activity(env: &Env, user: &Address, now: u64) {
        let auto_unfreeze_at = now.saturating_add(DEFAULT_FREEZE_DURATION_SECONDS);
        set_budget_freeze(
            env,
            user,
            &BudgetFreeze {
                is_frozen: true,
                frozen_at: now,
                auto_unfreeze_at,
            },
        );
        increment_suspicious_count(env);
        BudgetEvents::budget_frozen(env, user, now, auto_unfreeze_at);
    }

    fn assert_not_frozen(env: &Env, user: &Address) {
        if is_budget_frozen(env, user, env.ledger().timestamp()) {
            panic_with_error!(env, BudgetError::BudgetFrozen);
        }
    }

    /// Saves a budget template from a user's current budget.
    pub fn save_budget_template(
        env: Env,
        user: Address,
        template_id: Symbol,
        template_name: Symbol,
    ) {
        user.require_auth();

        let budget = load_user_budget(&env, &user).unwrap_or_else(|| {
            panic_with_error!(&env, BudgetError::BudgetNotFound);
        });

        let now = env.ledger().timestamp();
        let template = BudgetTemplate {
            id: template_id,
            name: template_name,
            categories: budget.categories.clone(),
            created_by: user.clone(),
            created_at: now,
        };

        save_template(&env, &template);
        storage::set_last_activity(&env, &user, now);
    }

    /// Creates a template from explicit category limits (spent amounts set to 0).
    pub fn create_template(
        env: Env,
        user: Address,
        template_id: Symbol,
        template_name: Symbol,
        categories: Map<Symbol, i128>,
    ) {
        user.require_auth();

        let now = env.ledger().timestamp();
        let mut category_budgets = Map::new(&env);

        for (name, limit) in categories.iter() {
            category_budgets.set(
                name.clone(),
                CategoryBudget {
                    name: name.clone(),
                    limit,
                    spent: 0,
                },
            );
        }

        let template = BudgetTemplate {
            id: template_id,
            name: template_name,
            categories: category_budgets,
            created_by: user.clone(),
            created_at: now,
        };

        save_template(&env, &template);
        storage::set_last_activity(&env, &user, now);
    }

    /// Clones a template to a user's budget (sets categories with spent=0).
    pub fn clone_template(env: Env, user: Address, template_id: Symbol) {
        user.require_auth();

        let template = get_template(&env, template_id).unwrap_or_else(|| {
            panic_with_error!(&env, BudgetError::BudgetNotFound);
        });

        let now = env.ledger().timestamp();
        let mut budget = load_user_budget(&env, &user).unwrap_or(UserBudget {
            user: user.clone(),
            categories: Map::new(&env),
            last_updated: now,
        });

        // Apply template categories with spent=0
        for (name, cat) in template.categories.iter() {
            budget.categories.set(
                name.clone(),
                CategoryBudget {
                    name: name.clone(),
                    limit: cat.limit,
                    spent: 0,
                },
            );
        }

        budget.last_updated = now;
        set_user_budget(&env, &budget);
        storage::set_last_activity(&env, &user, now);
    }

    /// Retrieves a budget template.
    pub fn get_template(env: Env, template_id: Symbol) -> Option<BudgetTemplate> {
        get_template(&env, template_id)
    }

    /// Retrieves all template IDs for a user.
    pub fn get_user_templates(env: Env, user: Address) -> Vec<Symbol> {
        get_user_templates(&env, &user)
    }

    /// Deletes a template (only creator can delete).
    pub fn delete_template(env: Env, user: Address, template_id: Symbol) {
        user.require_auth();
        delete_template(&env, template_id, &user);
        storage::set_last_activity(&env, &user, env.ledger().timestamp());
    }

    /// Clones an existing template to a new template ID.
    pub fn copy_template(
        env: Env,
        user: Address,
        source_template_id: Symbol,
        new_template_id: Symbol,
        new_template_name: Symbol,
    ) {
        user.require_auth();

        let source_template = get_template(&env, source_template_id).unwrap_or_else(|| {
            panic_with_error!(&env, BudgetError::BudgetNotFound);
        });

        let now = env.ledger().timestamp();
        let new_template = BudgetTemplate {
            id: new_template_id,
            name: new_template_name,
            categories: source_template.categories.clone(),
            created_by: user.clone(),
            created_at: now,
        };

        save_template(&env, &new_template);
        storage::set_last_activity(&env, &user, now);
    }

    /// Creates a recovery checkpoint of the current user budget.
    /// Note: Recovery flattens all categories into a single 'default' bucket
    /// to ensure a clean state reset during recovery.
    pub fn create_recovery_checkpoint(env: Env, user: Address) {
        user.require_auth();
        let budget = load_user_budget(&env, &user).unwrap_or_else(|| {
            panic_with_error!(&env, BudgetError::BudgetNotFound);
        });

        let mut total_limit: i128 = 0;
        let mut total_spent: i128 = 0;

        for (_, cat) in budget.categories.iter() {
            total_limit = total_limit.saturating_add(cat.limit);
            total_spent = total_spent.saturating_add(cat.spent);
        }

        let checkpoint = BudgetCheckpoint {
            owner: user.clone(),
            limit: total_limit,
            spent: total_spent,
            version: 1,
        };

        env.storage()
            .persistent()
            .set(&DataKey::BudgetCheckpoint(user.clone()), &checkpoint);

        BudgetEvents::checkpoint_created(&env, &user, env.ledger().timestamp());
    }

    /// Restores the user budget from the latest recovery checkpoint.
    pub fn restore_budget_from_checkpoint(env: Env, user: Address) {
        user.require_auth();
        let checkpoint: BudgetCheckpoint = env
            .storage()
            .persistent()
            .get(&DataKey::BudgetCheckpoint(user.clone()))
            .unwrap_or_else(|| {
                panic_with_error!(&env, BudgetError::CheckpointNotFound);
            });

        // Defense-in-depth: Verify internal owner field matches caller
        if checkpoint.owner != user || checkpoint.version != 1 {
            panic_with_error!(&env, BudgetError::IntegrityCheckFailed);
        }

        let mut categories = Map::new(&env);
        let default_name = symbol_short!("default");
        categories.set(
            default_name.clone(),
            CategoryBudget {
                name: default_name,
                limit: checkpoint.limit,
                spent: checkpoint.spent,
            },
        );

        let budget = UserBudget {
            user: user.clone(),
            categories,
            last_updated: env.ledger().timestamp(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::UserBudget(user.clone()), &budget);
        storage::set_last_activity(&env, &user, budget.last_updated);

        BudgetEvents::budget_restored(&env, &user, budget.last_updated);
    }

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
}

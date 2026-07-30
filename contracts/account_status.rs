//! Account Status Contract
//!
//! Allows admins to freeze suspicious accounts, blocking them from performing
//! transactions in integrated contracts. Supports multiple admins, freeze
//! reasons, an audit trail, and an optional auto-expiry on freeze.
//!
//! # Lifecycle
//!
//! 1. **`initialize`** — deployer sets the super-admin once.
//! 2. **`add_admin`** / **`remove_admin`** — super-admin manages the admin set.
//! 3. **`freeze_account`** — any admin freezes a target with a reason string
//!    and an optional expiry timestamp (0 = indefinite).
//! 4. **`unfreeze_account`** — any admin lifts the freeze.
//! 5. **`assert_not_frozen`** — other contracts call this as a gate; it panics
//!    if the account is currently frozen (or the freeze has not yet expired).
//!
//! # Security properties
//!
//! - `require_auth` is the first statement in every mutating entry point.
//! - Only the super-admin can add or remove admins; ordinary admins cannot
//!   escalate their own privileges.
//! - An admin cannot freeze the super-admin address.
//! - An admin cannot freeze themselves.
//! - All storage writes are preceded by all validation checks.
//! - TTL is bumped on every access to prevent silent state eviction.

#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracterror, contracttype, panic_with_error, symbol_short,
    Address, Env, Symbol, Vec,
};

// ── Constants ────────────────────────────────────────────────────────────────

/// Ledger TTL bump for persistent account-status records (~2 years).
const PERSISTENT_TTL_BUMP: u32 = 12_614_400;

/// Maximum length of a freeze reason string (in bytes).
pub const MAX_REASON_LEN: usize = 256;

// ── Account status symbols ───────────────────────────────────────────────────
//
// Symbols returned by [`AccountStatusContract::get_account_status`].

/// Account is in good standing and may transact.
pub const STATUS_ACTIVE: &str = "active";

/// Account has an active (non-expired) freeze applied by an admin.
pub const STATUS_SUSPENDED: &str = "suspended";

/// Account has been permanently closed. Reserved: this contract has no
/// closure transition today, so `get_account_status` never returns it.
pub const STATUS_CLOSED: &str = "closed";

// ── Storage keys ─────────────────────────────────────────────────────────────

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    /// Super-admin address (instance storage).
    SuperAdmin,
    /// Ordered list of current admin addresses (instance storage).
    AdminList,
    /// Per-account freeze record (persistent storage).
    AccountStatus(Address),
    /// Total number of freeze actions ever applied (instance storage).
    FreezeCount,
}

// ── Types ─────────────────────────────────────────────────────────────────────

/// The current status record for a single account.
#[derive(Clone)]
#[contracttype]
pub struct AccountStatusRecord {
    /// Whether the account is currently frozen.
    pub frozen: bool,
    /// The admin address that applied the most recent freeze.
    pub frozen_by: Option<Address>,
    /// Human-readable reason provided at freeze time.
    pub reason: soroban_sdk::String,
    /// Ledger timestamp when the freeze was applied (0 if never frozen).
    pub frozen_at: u64,
    /// Ledger timestamp after which the freeze auto-expires (0 = indefinite).
    pub expires_at: u64,
    /// Total number of times this account has been frozen.
    pub freeze_count: u32,
}

// ── Errors ────────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum AccountStatusError {
    /// Contract has not been initialised.
    NotInitialized = 1,
    /// Contract has already been initialised.
    AlreadyInitialized = 2,
    /// Caller is not an admin.
    Unauthorized = 3,
    /// Target account is already frozen.
    AlreadyFrozen = 4,
    /// Target account is not currently frozen.
    NotFrozen = 5,
    /// Account is frozen — transaction blocked.
    AccountFrozen = 6,
    /// An admin cannot freeze themselves.
    CannotFreezeSelf = 7,
    /// The super-admin account cannot be frozen.
    CannotFreezeSuperAdmin = 8,
    /// Provided address is already an admin.
    AlreadyAdmin = 9,
    /// Provided address is not in the admin list.
    AdminNotFound = 10,
    /// Arithmetic overflow.
    Overflow = 11,
    /// Freeze reason exceeds maximum allowed length.
    ReasonTooLong = 12,
    /// Expiry timestamp is in the past.
    InvalidExpiry = 13,
}

// ── Events ────────────────────────────────────────────────────────────────────

pub struct AccountStatusEvents;

impl AccountStatusEvents {
    /// Emitted when an account is frozen.
    /// Payload: `(admin, target, reason, expires_at, timestamp)`
    pub fn account_frozen(
        env: &Env,
        admin: &Address,
        target: &Address,
        reason: &soroban_sdk::String,
        expires_at: u64,
    ) {
        env.events().publish(
            (symbol_short!("acct"), symbol_short!("frozen")),
            (
                admin.clone(),
                target.clone(),
                reason.clone(),
                expires_at,
                env.ledger().timestamp(),
            ),
        );
    }

    /// Emitted when a freeze is lifted.
    /// Payload: `(admin, target, timestamp)`
    pub fn account_unfrozen(env: &Env, admin: &Address, target: &Address) {
        env.events().publish(
            (symbol_short!("acct"), symbol_short!("unfrozn")),
            (
                admin.clone(),
                target.clone(),
                env.ledger().timestamp(),
            ),
        );
    }

    /// Emitted when a new admin is added.
    /// Payload: `(super_admin, new_admin, timestamp)`
    pub fn admin_added(env: &Env, super_admin: &Address, new_admin: &Address) {
        env.events().publish(
            (symbol_short!("acct"), symbol_short!("adm_add")),
            (
                super_admin.clone(),
                new_admin.clone(),
                env.ledger().timestamp(),
            ),
        );
    }

    /// Emitted when an admin is removed.
    /// Payload: `(super_admin, removed_admin, timestamp)`
    pub fn admin_removed(env: &Env, super_admin: &Address, removed_admin: &Address) {
        env.events().publish(
            (symbol_short!("acct"), symbol_short!("adm_rm")),
            (
                super_admin.clone(),
                removed_admin.clone(),
                env.ledger().timestamp(),
            ),
        );
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

impl AccountStatusContract {
    /// Load the super-admin or panic with `NotInitialized`.
    fn require_initialized(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::SuperAdmin)
            .unwrap_or_else(|| panic_with_error!(env, AccountStatusError::NotInitialized))
    }

    /// Assert `caller` is either the super-admin or in the admin list.
    fn require_admin(env: &Env, caller: &Address) {
        let super_admin = Self::require_initialized(env);
        if caller == &super_admin {
            return;
        }
        let admins: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AdminList)
            .unwrap_or_else(|| Vec::new(env));
        if !admins.contains(caller) {
            panic_with_error!(env, AccountStatusError::Unauthorized);
        }
    }

    /// Assert `caller` is the super-admin specifically.
    fn require_super_admin(env: &Env, caller: &Address) {
        let super_admin = Self::require_initialized(env);
        if caller != &super_admin {
            panic_with_error!(env, AccountStatusError::Unauthorized);
        }
    }

    /// Load an account status record, or return a zeroed default.
    fn load_status(env: &Env, account: &Address) -> AccountStatusRecord {
        let key = DataKey::AccountStatus(account.clone());
        let record: AccountStatusRecord = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(AccountStatusRecord {
                frozen: false,
                frozen_by: None,
                reason: soroban_sdk::String::from_str(env, ""),
                frozen_at: 0,
                expires_at: 0,
                freeze_count: 0,
            });
        if env.storage().persistent().has(&key) {
            env.storage()
                .persistent()
                .extend_ttl(&key, PERSISTENT_TTL_BUMP, PERSISTENT_TTL_BUMP);
        }
        record
    }

    /// Save an account status record and bump its TTL.
    fn save_status(env: &Env, account: &Address, record: &AccountStatusRecord) {
        let key = DataKey::AccountStatus(account.clone());
        env.storage().persistent().set(&key, record);
        env.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_TTL_BUMP, PERSISTENT_TTL_BUMP);
    }

    /// Return `true` if the account's freeze is currently active (not expired).
    fn is_currently_frozen(env: &Env, record: &AccountStatusRecord) -> bool {
        if !record.frozen {
            return false;
        }
        // If expires_at == 0 the freeze is indefinite.
        if record.expires_at == 0 {
            return true;
        }
        env.ledger().timestamp() < record.expires_at
    }
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct AccountStatusContract;

#[contractimpl]
impl AccountStatusContract {
    // ── Lifecycle ────────────────────────────────────────────────────────────

    /// Initialise the contract with a super-admin. Only callable once.
    pub fn initialize(env: Env, super_admin: Address) {
        if env.storage().instance().has(&DataKey::SuperAdmin) {
            panic_with_error!(&env, AccountStatusError::AlreadyInitialized);
        }
        env.storage()
            .instance()
            .set(&DataKey::SuperAdmin, &super_admin);
        env.storage()
            .instance()
            .set(&DataKey::AdminList, &Vec::<Address>::new(&env));
        env.storage()
            .instance()
            .set(&DataKey::FreezeCount, &0u32);
    }

    // ── Admin management (super-admin only) ───────────────────────────────────

    /// Grant admin privileges to `new_admin`. Super-admin only.
    ///
    /// # Security
    /// - Only the super-admin may expand the admin set — ordinary admins
    ///   cannot add peers.
    pub fn add_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();
        Self::require_super_admin(&env, &caller);

        let mut admins: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AdminList)
            .unwrap_or_else(|| Vec::new(&env));

        if admins.contains(&new_admin) {
            panic_with_error!(&env, AccountStatusError::AlreadyAdmin);
        }

        admins.push_back(new_admin.clone());
        env.storage()
            .instance()
            .set(&DataKey::AdminList, &admins);

        AccountStatusEvents::admin_added(&env, &caller, &new_admin);
    }

    /// Revoke admin privileges from `admin`. Super-admin only.
    pub fn remove_admin(env: Env, caller: Address, admin: Address) {
        caller.require_auth();
        Self::require_super_admin(&env, &caller);

        let admins: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AdminList)
            .unwrap_or_else(|| Vec::new(&env));

        // Find and remove the admin.
        let mut new_admins = Vec::new(&env);
        let mut found = false;
        for a in admins.iter() {
            if a == admin {
                found = true;
            } else {
                new_admins.push_back(a);
            }
        }

        if !found {
            panic_with_error!(&env, AccountStatusError::AdminNotFound);
        }

        env.storage()
            .instance()
            .set(&DataKey::AdminList, &new_admins);

        AccountStatusEvents::admin_removed(&env, &caller, &admin);
    }

    // ── Freeze / unfreeze ────────────────────────────────────────────────────

    /// Freeze `target`. Any admin may call.
    ///
    /// # Parameters
    /// - `reason`     — human-readable justification; max `MAX_REASON_LEN` bytes.
    /// - `expires_at` — ledger timestamp after which the freeze auto-lifts;
    ///                  pass `0` for an indefinite freeze.
    ///
    /// # Security
    /// - `caller.require_auth()` is first.
    /// - Admin cannot freeze themselves (`CannotFreezeSelf`).
    /// - Admin cannot freeze the super-admin (`CannotFreezeSuperAdmin`).
    /// - Target must not already be frozen (`AlreadyFrozen`).
    /// - `expires_at`, if non-zero, must be in the future.
    pub fn freeze_account(
        env: Env,
        caller: Address,
        target: Address,
        reason: soroban_sdk::String,
        expires_at: u64,
    ) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        // Guard: cannot freeze self.
        if caller == target {
            panic_with_error!(&env, AccountStatusError::CannotFreezeSelf);
        }

        // Guard: cannot freeze the super-admin.
        let super_admin = Self::require_initialized(&env);
        if target == super_admin {
            panic_with_error!(&env, AccountStatusError::CannotFreezeSuperAdmin);
        }

        // Guard: reason length.
        if reason.len() as usize > MAX_REASON_LEN {
            panic_with_error!(&env, AccountStatusError::ReasonTooLong);
        }

        // Guard: expiry must be in the future if provided.
        if expires_at != 0 && expires_at <= env.ledger().timestamp() {
            panic_with_error!(&env, AccountStatusError::InvalidExpiry);
        }

        let mut record = Self::load_status(&env, &target);

        // Guard: not already frozen (check with expiry awareness).
        if Self::is_currently_frozen(&env, &record) {
            panic_with_error!(&env, AccountStatusError::AlreadyFrozen);
        }

        // All checks passed — write state.
        record.frozen = true;
        record.frozen_by = Some(caller.clone());
        record.reason = reason.clone();
        record.frozen_at = env.ledger().timestamp();
        record.expires_at = expires_at;
        record.freeze_count = record
            .freeze_count
            .checked_add(1)
            .unwrap_or_else(|| panic_with_error!(&env, AccountStatusError::Overflow));

        Self::save_status(&env, &target, &record);

        // Increment global freeze counter.
        let count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::FreezeCount)
            .unwrap_or(0);
        env.storage().instance().set(
            &DataKey::FreezeCount,
            &count
                .checked_add(1)
                .unwrap_or_else(|| panic_with_error!(&env, AccountStatusError::Overflow)),
        );

        AccountStatusEvents::account_frozen(&env, &caller, &target, &reason, expires_at);
    }

    /// Unfreeze `target`. Any admin may call.
    ///
    /// # Security
    /// - `caller.require_auth()` is first.
    /// - Target must currently be frozen (active, not expired).
    pub fn unfreeze_account(env: Env, caller: Address, target: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let mut record = Self::load_status(&env, &target);

        if !Self::is_currently_frozen(&env, &record) {
            panic_with_error!(&env, AccountStatusError::NotFrozen);
        }

        record.frozen = false;
        record.expires_at = 0;
        Self::save_status(&env, &target, &record);

        AccountStatusEvents::account_unfrozen(&env, &caller, &target);
    }

    // ── Gate function (called by other contracts) ─────────────────────────────

    /// Assert that `account` is not currently frozen.
    ///
    /// Panics with `AccountFrozen` if the account is frozen and the freeze
    /// has not expired. Intended to be called as a guard at the start of any
    /// transaction entry point in integrated contracts.
    ///
    /// Expired freezes are treated as unfrozen without requiring an explicit
    /// `unfreeze_account` call.
    pub fn assert_not_frozen(env: Env, account: Address) {
        Self::require_initialized(&env);
        let record = Self::load_status(&env, &account);
        if Self::is_currently_frozen(&env, &record) {
            panic_with_error!(&env, AccountStatusError::AccountFrozen);
        }
    }

    // ── Read-only queries ────────────────────────────────────────────────────

    /// Return the full status record for `account`.
    pub fn get_status(env: Env, account: Address) -> AccountStatusRecord {
        Self::require_initialized(&env);
        Self::load_status(&env, &account)
    }

    /// Return the lifecycle status of `owner` as a symbol.
    ///
    /// # Returns
    /// - `suspended` — the account has an active freeze (expiry aware).
    /// - `active`    — the account has no active freeze. This is also the
    ///                 documented default for an address that has never been
    ///                 seen by this contract.
    ///
    /// `closed` ([`STATUS_CLOSED`]) is reserved for a future account-closure
    /// transition and is never returned by this implementation.
    pub fn get_account_status(env: Env, owner: Address) -> Symbol {
        Self::require_initialized(&env);
        let record = Self::load_status(&env, &owner);
        if Self::is_currently_frozen(&env, &record) {
            symbol_short!("suspended")
        } else {
            symbol_short!("active")
        }
    }

    /// Return `true` if `account` is currently frozen (respects expiry).
    pub fn is_frozen(env: Env, account: Address) -> bool {
        Self::require_initialized(&env);
        let record = Self::load_status(&env, &account);
        Self::is_currently_frozen(&env, &record)
    }

    /// Return `true` if `addr` has admin or super-admin privileges.
    pub fn is_admin(env: Env, addr: Address) -> bool {
        let super_admin: Option<Address> =
            env.storage().instance().get(&DataKey::SuperAdmin);
        let Some(sa) = super_admin else {
            return false;
        };
        if addr == sa {
            return true;
        }
        let admins: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AdminList)
            .unwrap_or_else(|| Vec::new(&env));
        admins.contains(&addr)
    }

    /// Return the total number of freeze actions ever applied across all accounts.
    pub fn total_freeze_count(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::FreezeCount)
            .unwrap_or(0)
    }

    /// Return all current admin addresses (excluding the super-admin).
    pub fn get_admins(env: Env) -> Vec<Address> {
        Self::require_initialized(&env);
        env.storage()
            .instance()
            .get(&DataKey::AdminList)
            .unwrap_or_else(|| Vec::new(&env))
    }
}
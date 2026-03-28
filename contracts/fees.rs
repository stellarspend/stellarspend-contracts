use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, Address,
    Env,
};

/// Storage keys used by the fees contract.
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    /// Fee percentage stored in basis points (bps).
    /// The value is expected to be between 0 and 10_000 (100%).
    FeePercentage,
    /// Cumulative fees that have been collected through `deduct_fee`.
    TotalFeesCollected,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum FeeError {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
    InvalidPercentage = 4,
    InvalidAmount = 5,
    Overflow = 6,
    ValidationFailed = 7,
}

/// Events emitted by the fees contract.
pub struct FeeEvents;

impl FeeEvents {
    pub fn fee_deducted(env: &Env, payer: &Address, amount: i128, fee: i128) {
        let topics = (symbol_short!("fee"), symbol_short!("deducted"));
        env.events().publish(
            topics,
            (payer.clone(), amount, fee, env.ledger().timestamp()),
        );
    }

    pub fn config_updated(env: &Env, admin: &Address, percentage_bps: u32) {
        let topics = (symbol_short!("fee"), symbol_short!("cfg_upd"));
        env.events().publish(
            topics,
            (admin.clone(), percentage_bps, env.ledger().timestamp()),
        );
    }
}

/// Internal helpers — not exposed as contract entry points.
impl FeesContract {
    fn require_initialized(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(env, FeeError::NotInitialized))
    }

    fn require_admin(env: &Env, caller: &Address) {
        let admin = Self::require_initialized(env);
        if caller != &admin {
            panic_with_error!(env, FeeError::Unauthorized);
        }
    }
}

#[contract]
pub struct FeesContract;

#[contractimpl]
impl FeesContract {
    /// Initializes the fees contract with an admin and an initial percentage
    /// (in basis points, 0–10_000). Only callable once.
    ///
    /// # Security
    /// - Guard: `AlreadyInitialized` prevents re-initialization attacks.
    /// - `percentage_bps` is validated ≤ 10_000 before any state is written.
    pub fn initialize(env: Env, admin: Address, percentage_bps: u32) {
        // [SEC-FEES-01] Re-initialization guard: must be checked before any writes.
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&env, FeeError::AlreadyInitialized);
        }
        // [SEC-FEES-02] Validate percentage before committing state.
        if percentage_bps > 10_000 {
            panic_with_error!(&env, FeeError::InvalidPercentage);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::FeePercentage, &percentage_bps);
        env.storage()
            .instance()
            .set(&DataKey::TotalFeesCollected, &0i128);
    }

    /// Updates the fee percentage. Only the current admin may call.
    ///
    /// # Security
    /// - [SEC-FEES-03] `caller.require_auth()` is invoked *before* any storage
    ///   reads so the host can short-circuit unauthorized calls cheaply.
    /// - Admin check uses the centralized `require_admin` helper to avoid
    ///   inconsistent comparisons across call sites.
    pub fn set_percentage(env: Env, caller: Address, percentage_bps: u32) {
        // [SEC-FEES-03] Authenticate before reading sensitive state.
        caller.require_auth();
        Self::require_admin(&env, &caller);

        if percentage_bps > 10_000 {
            panic_with_error!(&env, FeeError::InvalidPercentage);
        }
        env.storage()
            .instance()
            .set(&DataKey::FeePercentage, &percentage_bps);
        FeeEvents::config_updated(&env, &caller, percentage_bps);
    }

    /// Returns the current fee percentage in basis points.
    /// Defaults to 0 when the contract has not yet been initialized.
    pub fn get_percentage(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::FeePercentage)
            .unwrap_or(0)
    }

    /// Calculates the fee for `amount` using the current percentage.
    ///
    /// # Security
    /// - [SEC-FEES-04] Rejects non-positive amounts to prevent zero-fee bypass.
    /// - [SEC-FEES-05] All arithmetic uses `checked_*` to trap overflow/underflow
    ///   and panics with the typed `Overflow` error instead of silent wrap.
    pub fn calculate_fee(env: Env, amount: i128) -> i128 {
        // [SEC-FEES-04] Reject non-positive amounts.
        if amount <= 0 {
            panic_with_error!(&env, FeeError::InvalidAmount);
        }
        let pct: u32 = Self::get_percentage(&env);
        // [SEC-FEES-05] Checked arithmetic throughout.
        let fee = amount
            .checked_mul(pct as i128)
            .unwrap_or_else(|| panic_with_error!(&env, FeeError::Overflow))
            .checked_div(10_000)
            .unwrap_or_else(|| panic_with_error!(&env, FeeError::Overflow));
        fee
    }

    /// Validates that the provided `fee_to_validate` matches the expected fee for `amount`.
    ///
    /// # Security
    /// - [SEC-FEES-08] Ensures deterministic fee validation against stored contract state.
    /// - Panics with `ValidationFailed` if the fees do not match exactly.
    pub fn validate_fee(env: Env, amount: i128, fee_to_validate: i128) {
        let expected_fee = Self::calculate_fee(env.clone(), amount);
        if expected_fee != fee_to_validate {
            panic_with_error!(&env, FeeError::ValidationFailed);
        }
    }

    /// Deducts the configured fee from `amount`.
    ///
    /// Returns `(net_amount, fee)` and updates the cumulative accounting.
    ///
    /// # Security
    /// - [SEC-FEES-06] `payer.require_auth()` is invoked first — no state
    ///   mutations can occur without authorization.
    /// - [SEC-FEES-07] `TotalFeesCollected` accumulation uses `checked_add` so
    ///   a saturated counter triggers `Overflow` rather than wrapping silently.
    /// - Requires the contract to be initialized; `calculate_fee` propagates
    ///   `NotInitialized` via `get_percentage` if called before `initialize`.
    pub fn deduct_fee(env: Env, payer: Address, amount: i128) -> (i128, i128) {
        // [SEC-FEES-06] Authenticate before any computation or state change.
        payer.require_auth();

        // Ensure contract is initialized before proceeding.
        Self::require_initialized(&env);

        let fee = Self::calculate_fee(&env, amount);

        // [SEC-FEES-07] Checked subtraction for net amount.
        let net = amount
            .checked_sub(fee)
            .unwrap_or_else(|| panic_with_error!(&env, FeeError::Overflow));

        let mut total: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalFeesCollected)
            .unwrap_or(0);

        // [SEC-FEES-07] Checked addition for running total.
        total = total
            .checked_add(fee)
            .unwrap_or_else(|| panic_with_error!(&env, FeeError::Overflow));

        env.storage()
            .instance()
            .set(&DataKey::TotalFeesCollected, &total);
        FeeEvents::fee_deducted(&env, &payer, amount, fee);
        (net, fee)
    }

    /// Returns cumulative fees collected since deployment.
    pub fn get_total_collected(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalFeesCollected)
            .unwrap_or(0)
    }
}

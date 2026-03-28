use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, Address,
    Env, Vec,
};
use crate::storage::{self, FeeConfigLog};

/// Represents a fee distribution recipient and their share.
#[derive(Clone, Debug)]
#[contracttype]
pub struct FeeRecipient {
    /// Address of the recipient
    pub address: Address,
    /// Share in basis points (bps). Must be 0–10_000.
    /// All recipients' shares must sum to 10_000 (100%).
    pub share_bps: u32,
}

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
    /// Per-user fee accrual tracking. Stores total fees paid by each user.
    UserFeesAccrued(Address),
    /// Fee distribution configuration. Stores vector of FeeRecipient.
    FeeDistribution,
    /// Cumulative fees distributed to a specific recipient.
    RecipientFeesAccumulated(Address),
    /// Minimum fee threshold. Fees cannot be less than this value.
    MinFee,
    /// Maximum fee threshold. Fees cannot exceed this value.
    MaxFee,
    /// Audit log for configuration changes.
    AuditLog,
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
    /// Refund amount is invalid (e.g., zero or negative).
    InvalidRefundAmount = 7,
    /// User has insufficient fee balance for the requested refund.
    InsufficientFeeBalance = 8,
    /// Distribution configuration is invalid (empty, exceeds 100%, or contains invalid shares).
    InvalidDistribution = 9,
    /// Total distribution shares do not equal 100% (10_000 bps).
    DistributionSumsToWrong = 10,
    /// No fee distribution has been configured yet.
    NoDistributionConfigured = 11,
    /// Min fee is negative or max fee is negative.
    InvalidFeeBound = 12,
    /// Max fee is less than min fee.
    InvalidFeeBoundRange = 13,
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

    pub fn fee_refunded(env: &Env, user: &Address, refund_amount: i128, reason: &str) {
        let topics = (symbol_short!("fee"), symbol_short!("refunded"));
        env.events().publish(
            topics,
            (user.clone(), refund_amount, reason, env.ledger().timestamp()),
        );
    }

    pub fn distribution_configured(env: &Env, admin: &Address, recipient_count: u32) {
        let topics = (symbol_short!("fee"), symbol_short!("dist_cfg"));
        env.events().publish(
            topics,
            (admin.clone(), recipient_count, env.ledger().timestamp()),
        );
    }

    pub fn fees_distributed(env: &Env, total_distributed: i128, recipient_count: u32) {
        let topics = (symbol_short!("fee"), symbol_short!("distributed"));
        env.events().publish(
            topics,
            (total_distributed, recipient_count, env.ledger().timestamp()),
        );
    }

    pub fn fee_bounds_configured(env: &Env, admin: &Address, min_fee: i128, max_fee: i128) {
        let topics = (symbol_short!("fee"), symbol_short!("bounds_cfg"));
        env.events().publish(
            topics,
            (admin.clone(), min_fee, max_fee, env.ledger().timestamp()),
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
        
        // Record initial configuration in audit log
        storage::write_audit_log(&env, 0, percentage_bps, admin);
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
        let previous_rate = Self::get_percentage(env.clone());
        env.storage()
            .instance()
            .set(&DataKey::FeePercentage, &percentage_bps);
        
        // Record update in audit log
        storage::write_audit_log(&env, previous_rate, percentage_bps, caller.clone());
        
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
    /// Applies min/max fee bounds if configured. The final fee will be:
    /// - At least min_fee (if configured)
    /// - At most max_fee (if configured)
    /// - Otherwise, fee_percentage * amount / 10000
    ///
    /// # Security
    /// - [SEC-FEES-04] Rejects non-positive amounts to prevent zero-fee bypass.
    /// - [SEC-FEES-05] All arithmetic uses `checked_*` to trap overflow/underflow
    ///   and panics with the typed `Overflow` error instead of silent wrap.
    /// - [SEC-FEES-18] Min/max fee bounds are applied to prevent unbounded fees
    ///   and ensure fees stay within configured ranges.
    pub fn calculate_fee(env: Env, amount: i128) -> i128 {
        // [SEC-FEES-04] Reject non-positive amounts.
        if amount <= 0 {
            panic_with_error!(&env, FeeError::InvalidAmount);
        }
        let pct: u32 = Self::get_percentage(&env);
        // [SEC-FEES-05] Checked arithmetic throughout.
        let mut fee = amount
            .checked_mul(pct as i128)
            .unwrap_or_else(|| panic_with_error!(&env, FeeError::Overflow))
            .checked_div(10_000)
            .unwrap_or_else(|| panic_with_error!(&env, FeeError::Overflow));

        // [SEC-FEES-18] Apply min/max fee bounds.
        let min_fee: i128 = env
            .storage()
            .instance()
            .get(&DataKey::MinFee)
            .unwrap_or(0);
        let max_fee: i128 = env
            .storage()
            .instance()
            .get(&DataKey::MaxFee)
            .unwrap_or(i128::MAX);

        if fee < min_fee {
            fee = min_fee;
        }
        if fee > max_fee {
            fee = max_fee;
        }

        fee
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
    /// - [SEC-FEES-08] Per-user fee tracking is updated with `checked_add` to
    ///   prevent overflow on per-user accumulation.
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

        // [SEC-FEES-08] Update per-user fee accrual tracking.
        let mut user_fees: i128 = env
            .storage()
            .instance()
            .get(&DataKey::UserFeesAccrued(payer.clone()))
            .unwrap_or(0);

        // [SEC-FEES-08] Checked addition for per-user total.
        user_fees = user_fees
            .checked_add(fee)
            .unwrap_or_else(|| panic_with_error!(&env, FeeError::Overflow));

        env.storage()
            .instance()
            .set(&DataKey::UserFeesAccrued(payer.clone()), &user_fees);

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

    /// Returns the total fees accrued by a specific user.
    ///
    /// Returns 0 if the user has not accrued any fees yet.
    ///
    /// # Arguments
    /// * `user` - The address of the user to query
    ///
    /// # Returns
    /// Total fees paid by the user in stroops (smallest unit)
    pub fn get_user_fees_accrued(env: Env, user: Address) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::UserFeesAccrued(user))
            .unwrap_or(0)
    }

    /// Refunds fees for a specific user.
    ///
    /// Only the admin can invoke this function. Validates that the refund amount
    /// does not exceed the user's accumulated fees. Updates both global and per-user
    /// fee balances.
    ///
    /// # Arguments
    /// * `caller` - The address requesting the refund (must be admin)
    /// * `user` - The user to whom fees are refunded
    /// * `refund_amount` - The amount to refund (must be positive)
    /// * `reason` - The reason for the refund (for audit trail)
    ///
    /// # Returns
    /// The refunded amount
    ///
    /// # Security
    /// - [SEC-FEES-09] `caller.require_auth()` is invoked first — admin-only refunds.
    /// - [SEC-FEES-10] `require_admin()` ensures only authorized admins can process
    ///   refunds, preventing unauthorized fee adjustments.
    /// - [SEC-FEES-11] Refund amount is validated as positive before any state mutation.
    /// - [SEC-FEES-12] User's fee balance is checked before refund — prevents negative
    ///   fee balances which would enable fee credit abuse.
    /// - [SEC-FEES-13] Checked arithmetic (`checked_sub`) prevents underflow when
    ///   reducing fee totals.
    pub fn refund_fee(
        env: Env,
        caller: Address,
        user: Address,
        refund_amount: i128,
        reason: &str,
    ) -> i128 {
        // [SEC-FEES-09] Authenticate before any computation or state change.
        caller.require_auth();

        // [SEC-FEES-10] Only admin can process refunds.
        Self::require_admin(&env, &caller);

        // [SEC-FEES-11] Validate refund amount is positive.
        if refund_amount <= 0 {
            panic_with_error!(&env, FeeError::InvalidRefundAmount);
        }

        // Ensure contract is initialized before proceeding.
        Self::require_initialized(&env);

        // [SEC-FEES-12] Check user has sufficient fee balance.
        let user_fees: i128 = env
            .storage()
            .instance()
            .get(&DataKey::UserFeesAccrued(user.clone()))
            .unwrap_or(0);

        if user_fees < refund_amount {
            panic_with_error!(&env, FeeError::InsufficientFeeBalance);
        }

        // [SEC-FEES-13] Deduct from global fee total using checked subtraction.
        let mut total: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalFeesCollected)
            .unwrap_or(0);

        total = total
            .checked_sub(refund_amount)
            .unwrap_or_else(|| panic_with_error!(&env, FeeError::Overflow));

        env.storage()
            .instance()
            .set(&DataKey::TotalFeesCollected, &total);

        // [SEC-FEES-13] Deduct from per-user fee balance using checked subtraction.
        let updated_user_fees = user_fees
            .checked_sub(refund_amount)
            .unwrap_or_else(|| panic_with_error!(&env, FeeError::Overflow));

        env.storage()
            .instance()
            .set(&DataKey::UserFeesAccrued(user.clone()), &updated_user_fees);

        FeeEvents::fee_refunded(&env, &user, refund_amount, reason);
        refund_amount
    }

    /// Sets the fee distribution configuration.
    ///
    /// Defines which recipients receive distributed fees and their respective shares.
    /// Only callable by the admin. Validates that:
    /// - Distribution is not empty
    /// - Each recipient has a valid share (0–10_000 bps)
    /// - All shares sum exactly to 10_000 (100%)
    ///
    /// # Arguments
    /// * `caller` - The address requesting configuration (must be admin)
    /// * `recipients` - Vector of FeeRecipient with address and share_bps
    ///
    /// # Security
    /// - [SEC-FEES-14] `caller.require_auth()` ensures only authorized admins can
    ///   configure distributions.
    /// - [SEC-FEES-15] Comprehensive validation prevents invalid distributions:
    ///   empty lists, invalid shares, or sums != 100%.
    pub fn set_distribution(env: Env, caller: Address, recipients: Vec<FeeRecipient>) {
        // [SEC-FEES-14] Authenticate before any state mutation.
        caller.require_auth();
        Self::require_admin(&env, &caller);

        // [SEC-FEES-15] Validate distribution is not empty.
        if recipients.len() == 0 {
            panic_with_error!(&env, FeeError::InvalidDistribution);
        }

        let mut total_bps: u32 = 0;
        for recipient in recipients.iter() {
            // [SEC-FEES-15] Validate each share is within valid range.
            if recipient.share_bps > 10_000 {
                panic_with_error!(&env, FeeError::InvalidDistribution);
            }
            // [SEC-FEES-15] Accumulate total and check for overflow.
            total_bps = total_bps
                .checked_add(recipient.share_bps)
                .unwrap_or_else(|| panic_with_error!(&env, FeeError::Overflow));
        }

        // [SEC-FEES-15] Ensure total equals exactly 100% (10_000 bps).
        if total_bps != 10_000 {
            panic_with_error!(&env, FeeError::DistributionSumsToWrong);
        }

        env.storage()
            .instance()
            .set(&DataKey::FeeDistribution, &recipients);
        FeeEvents::distribution_configured(&env, &caller, recipients.len() as u32);
    }

    /// Returns the current fee distribution configuration.
    ///
    /// # Returns
    /// Vector of FeeRecipient, or empty vector if no distribution configured
    pub fn get_distribution(env: Env) -> Vec<FeeRecipient> {
        env.storage()
            .instance()
            .get(&DataKey::FeeDistribution)
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Distributes accumulated fees to all configured recipients.
    ///
    /// Only callable by the admin. Requires that a valid distribution configuration
    /// has been set. Distributes fees according to each recipient's share percentage.
    ///
    /// # Returns
    /// Total amount distributed
    ///
    /// # Security
    /// - [SEC-FEES-14] `caller.require_auth()` ensures only authorized admins can
    ///   trigger distributions.
    /// - [SEC-FEES-16] Distribution must be configured before distribution can occur.
    /// - [SEC-FEES-17] All per-recipient distributions use checked arithmetic to
    ///   prevent overflow.
    pub fn distribute_fees(env: Env, caller: Address) -> i128 {
        // [SEC-FEES-14] Authenticate before any state mutation.
        caller.require_auth();
        Self::require_admin(&env, &caller);

        // [SEC-FEES-16] Check distribution is configured.
        let recipients: Vec<FeeRecipient> = env
            .storage()
            .instance()
            .get(&DataKey::FeeDistribution)
            .unwrap_or_else(|| panic_with_error!(&env, FeeError::NoDistributionConfigured));

        if recipients.len() == 0 {
            panic_with_error!(&env, FeeError::NoDistributionConfigured);
        }

        // Get current total fees to distribute
        let total_to_distribute: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalFeesCollected)
            .unwrap_or(0);

        // If no fees to distribute, return early
        if total_to_distribute <= 0 {
            return 0;
        }

        let mut total_distributed: i128 = 0;

        // Distribute to each recipient according to their share
        for recipient in recipients.iter() {
            // [SEC-FEES-17] Calculate recipient's share using checked arithmetic.
            let recipient_share: i128 = total_to_distribute
                .checked_mul(recipient.share_bps as i128)
                .unwrap_or_else(|| panic_with_error!(&env, FeeError::Overflow))
                .checked_div(10_000)
                .unwrap_or_else(|| panic_with_error!(&env, FeeError::Overflow));

            // [SEC-FEES-17] Accumulate recipient's fees.
            let mut recipient_fees: i128 = env
                .storage()
                .instance()
                .get(&DataKey::RecipientFeesAccumulated(recipient.address.clone()))
                .unwrap_or(0);

            recipient_fees = recipient_fees
                .checked_add(recipient_share)
                .unwrap_or_else(|| panic_with_error!(&env, FeeError::Overflow));

            env.storage()
                .instance()
                .set(&DataKey::RecipientFeesAccumulated(recipient.address.clone()), &recipient_fees);

            total_distributed = total_distributed
                .checked_add(recipient_share)
                .unwrap_or_else(|| panic_with_error!(&env, FeeError::Overflow));
        }

        // Reset total fees collected after distribution
        env.storage()
            .instance()
            .set(&DataKey::TotalFeesCollected, &0i128);

        FeeEvents::fees_distributed(&env, total_distributed, recipients.len() as u32);
        total_distributed
    }

    /// Returns the cumulative fees accumulated by a specific recipient.
    ///
    /// # Arguments
    /// * `recipient` - The recipient address to query
    ///
    /// # Returns
    /// Total fees accumulated for the recipient
    pub fn get_recipient_fees_accumulated(env: Env, recipient: Address) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::RecipientFeesAccumulated(recipient))
            .unwrap_or(0)
    }

    /// Sets the minimum and maximum fee bounds.
    ///
    /// Fees calculated from percentage will be bounded to stay within [min_fee, max_fee].
    /// Only callable by the admin. Validates that:
    /// - Both bounds are non-negative
    /// - max_fee is >= min_fee
    ///
    /// # Arguments
    /// * `caller` - The address requesting configuration (must be admin)
    /// * `min_fee` - Minimum fee threshold (must be >= 0)
    /// * `max_fee` - Maximum fee threshold (must be >= min_fee)
    ///
    /// # Security
    /// - [SEC-FEES-19] `caller.require_auth()` ensures only authorized admins can
    ///   configure fee bounds.
    /// - [SEC-FEES-20] Comprehensive validation prevents invalid bounds:
    ///   negative values or inverted ranges.
    pub fn set_fee_bounds(env: Env, caller: Address, min_fee: i128, max_fee: i128) {
        // [SEC-FEES-19] Authenticate before any state mutation.
        caller.require_auth();
        Self::require_admin(&env, &caller);

        // [SEC-FEES-20] Validate both bounds are non-negative.
        if min_fee < 0 || max_fee < 0 {
            panic_with_error!(&env, FeeError::InvalidFeeBound);
        }

        // [SEC-FEES-20] Validate max >= min.
        if max_fee < min_fee {
            panic_with_error!(&env, FeeError::InvalidFeeBoundRange);
        }

        env.storage().instance().set(&DataKey::MinFee, &min_fee);
        env.storage().instance().set(&DataKey::MaxFee, &max_fee);
        FeeEvents::fee_bounds_configured(&env, &caller, min_fee, max_fee);
    }

    /// Returns the minimum fee threshold.
    ///
    /// # Returns
    /// Minimum fee in stroops, or 0 if not configured
    pub fn get_min_fee(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::MinFee)
            .unwrap_or(0)
    }

    /// Returns the maximum fee threshold.
    ///
    /// # Returns
    /// Maximum fee in stroops, or i128::MAX if not configured
    pub fn get_max_fee(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::MaxFee)
            .unwrap_or(i128::MAX)
    }

    /// Returns the history of configuration changes.
    pub fn get_audit_log(env: Env) -> Vec<FeeConfigLog> {
        storage::get_audit_log(&env)
    }
}

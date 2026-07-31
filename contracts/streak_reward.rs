//! Streak Rewards Contract
//!
//! Rewards users for making consistent daily savings deposits.
//!
//! # How it works
//!
//! 1. The contract is initialised once with an admin, a reward token address,
//!    and configurable reward tiers.
//! 2. Users call `record_deposit` each day they make a qualifying deposit.
//!    The contract advances (or resets) their streak counter.
//! 3. When a streak milestone is hit (7 / 30 / 100 days), `claim_reward` pays
//!    the user the corresponding bonus and stamps the claim so it cannot be
//!    collected again for the same milestone epoch.
//!
//! # Security properties
//!
//! - One deposit per user per UTC day (keyed by day-number, not timestamp).
//! - Streak reset when a day is missed; no back-dating allowed.
//! - Duplicate reward claims rejected via a per-(user, epoch) claim stamp.
//! - All arithmetic is checked; overflow panics with a typed error.
//! - `require_auth` is always the first statement in mutating entry points.

#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracterror, contracttype, panic_with_error, symbol_short,
    token, Address, Env,
};

// ── Constants ────────────────────────────────────────────────────────────────

/// Seconds in one UTC day.
const SECONDS_PER_DAY: u64 = 86_400;

/// Streak milestone: weekly bonus tier.
pub const STREAK_TIER_WEEK: u32 = 7;
/// Streak milestone: monthly bonus tier.
pub const STREAK_TIER_MONTH: u32 = 30;
/// Streak milestone: century bonus tier.
pub const STREAK_TIER_CENTURY: u32 = 100;

/// Ledger TTL bump for persistent user records (~1 year in ledgers at ~5s/ledger).
const PERSISTENT_TTL_BUMP: u32 = 6_307_200;

// ── Storage keys ─────────────────────────────────────────────────────────────

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    /// Contract-wide configuration (instance storage).
    Config,
    /// Per-user streak record (persistent storage).
    UserStreak(Address),
    /// Duplicate-claim guard: (user, milestone_epoch) → bool (persistent storage).
    ///
    /// `milestone_epoch` = streak_length / milestone_tier.  Each time a user
    /// completes another full tier cycle a new epoch is opened, so replaying
    /// the same milestone across cycles is blocked.
    ClaimStamp(Address, u32, u32), // (user, tier, epoch)
}

// ── Types ─────────────────────────────────────────────────────────────────────

/// Contract-wide configuration stored in instance storage.
#[derive(Clone)]
#[contracttype]
pub struct StreakConfig {
    /// The administrator address; may update reward amounts and tiers.
    pub admin: Address,
    /// SAC-compatible reward token address.
    pub reward_token: Address,
    /// Bonus paid (in token stroops) at the WEEK milestone.
    pub reward_week: i128,
    /// Bonus paid (in token stroops) at the MONTH milestone.
    pub reward_month: i128,
    /// Bonus paid (in token stroops) at the CENTURY milestone.
    pub reward_century: i128,
}

/// Per-user streak state stored in persistent storage.
#[derive(Clone)]
#[contracttype]
pub struct UserStreakRecord {
    /// Current unbroken streak in days (incremented on each qualifying deposit).
    pub streak_days: u32,
    /// UTC day-number (timestamp / SECONDS_PER_DAY) of the last recorded deposit.
    /// Used to detect same-day duplicates and missed days.
    pub last_deposit_day: u64,
    /// Cumulative lifetime deposits recorded for this user.
    pub total_deposits: u32,
    /// Cumulative bonus tokens claimed by this user (in stroops).
    pub total_rewards_claimed: i128,
}

// ── Errors ────────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum StreakError {
    /// Contract has not been initialised.
    NotInitialized = 1,
    /// Contract has already been initialised.
    AlreadyInitialized = 2,
    /// Caller is not the admin.
    Unauthorized = 3,
    /// A deposit has already been recorded for this user today.
    AlreadyDepositedToday = 4,
    /// No milestone reward is available to claim for this user.
    NoRewardAvailable = 5,
    /// This milestone reward has already been claimed for the current cycle.
    AlreadyClaimed = 6,
    /// Reward amount configuration is invalid (must be > 0).
    InvalidRewardAmount = 7,
    /// Arithmetic overflow detected.
    Overflow = 8,
}

// ── Events ────────────────────────────────────────────────────────────────────

pub struct StreakEvents;

impl StreakEvents {
    /// Emitted every time a deposit is successfully recorded.
    ///
    /// Payload: `(user, new_streak_days, day_number, timestamp)`
    pub fn deposit_recorded(env: &Env, user: &Address, streak_days: u32, day: u64) {
        env.events().publish(
            (symbol_short!("streak"), symbol_short!("deposit")),
            (user.clone(), streak_days, day, env.ledger().timestamp()),
        );
    }

    /// Emitted when a streak is broken (missed day detected).
    ///
    /// Payload: `(user, broken_streak_days, timestamp)`
    pub fn streak_broken(env: &Env, user: &Address, broken_streak: u32) {
        env.events().publish(
            (symbol_short!("streak"), symbol_short!("broken")),
            (user.clone(), broken_streak, env.ledger().timestamp()),
        );
    }

    /// Emitted when a milestone bonus is successfully paid out.
    ///
    /// Payload: `(user, tier, epoch, amount, timestamp)`
    pub fn reward_claimed(env: &Env, user: &Address, tier: u32, epoch: u32, amount: i128) {
        env.events().publish(
            (symbol_short!("streak"), symbol_short!("reward")),
            (user.clone(), tier, epoch, amount, env.ledger().timestamp()),
        );
    }

    /// Emitted when the admin updates reward configuration.
    ///
    /// Payload: `(admin, reward_week, reward_month, reward_century, timestamp)`
    pub fn config_updated(
        env: &Env,
        admin: &Address,
        reward_week: i128,
        reward_month: i128,
        reward_century: i128,
    ) {
        env.events().publish(
            (symbol_short!("streak"), symbol_short!("cfg_upd")),
            (
                admin.clone(),
                reward_week,
                reward_month,
                reward_century,
                env.ledger().timestamp(),
            ),
        );
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

impl StreakRewardsContract {
    /// Load config or panic with `NotInitialized`.
    fn load_config(env: &Env) -> StreakConfig {
        env.storage()
            .instance()
            .get(&DataKey::Config)
            .unwrap_or_else(|| panic_with_error!(env, StreakError::NotInitialized))
    }

    /// Assert caller is admin.
    fn require_admin(env: &Env, caller: &Address) {
        let config = Self::load_config(env);
        if caller != &config.admin {
            panic_with_error!(env, StreakError::Unauthorized);
        }
    }

    /// Convert a ledger timestamp to a UTC day-number.
    fn day_number(timestamp: u64) -> u64 {
        timestamp / SECONDS_PER_DAY
    }

    /// Compute the epoch number for a given streak length and tier.
    ///
    /// e.g. streak=14, tier=7 → epoch 1 (second full week completed).
    fn milestone_epoch(streak_days: u32, tier: u32) -> u32 {
        // Integer division: how many full tiers fit in the current streak.
        streak_days / tier
    }

    /// Load or create a user streak record, bumping its TTL.
    fn load_or_default_streak(env: &Env, user: &Address) -> UserStreakRecord {
        let key = DataKey::UserStreak(user.clone());
        let record: UserStreakRecord = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(UserStreakRecord {
                streak_days: 0,
                last_deposit_day: 0,
                total_deposits: 0,
                total_rewards_claimed: 0,
            });
        // Extend TTL so long-lived streaks are not evicted.
        env.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_TTL_BUMP, PERSISTENT_TTL_BUMP);
        record
    }

    /// Save a user streak record and bump its TTL.
    fn save_streak(env: &Env, user: &Address, record: &UserStreakRecord) {
        let key = DataKey::UserStreak(user.clone());
        env.storage().persistent().set(&key, record);
        env.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_TTL_BUMP, PERSISTENT_TTL_BUMP);
    }

    /// Return the reward amount for a given tier, or `None` if not a milestone.
    fn reward_for_tier(config: &StreakConfig, tier: u32) -> Option<i128> {
        match tier {
            STREAK_TIER_WEEK => Some(config.reward_week),
            STREAK_TIER_MONTH => Some(config.reward_month),
            STREAK_TIER_CENTURY => Some(config.reward_century),
            _ => None,
        }
    }

    /// Determine which milestone tiers (if any) the user has just crossed with
    /// their new streak value. Returns a list of eligible (tier, epoch) pairs.
    fn pending_milestones(streak_days: u32) -> [Option<u32>; 3] {
        let tiers = [STREAK_TIER_WEEK, STREAK_TIER_MONTH, STREAK_TIER_CENTURY];
        let mut result = [None; 3];
        for (i, &tier) in tiers.iter().enumerate() {
            if streak_days > 0 && streak_days % tier == 0 {
                result[i] = Some(tier);
            }
        }
        result
    }
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct StreakRewardsContract;

#[contractimpl]
impl StreakRewardsContract {
    // ── Admin / lifecycle ────────────────────────────────────────────────────

    /// Initialise the contract. Only callable once.
    ///
    /// `reward_token` must be a SAC-compatible token contract that has
    /// pre-authorised this contract address to transfer rewards to users.
    ///
    /// All reward amounts are in the token's smallest unit (stroops).
    pub fn initialize(
        env: Env,
        admin: Address,
        reward_token: Address,
        reward_week: i128,
        reward_month: i128,
        reward_century: i128,
    ) {
        // Re-init guard — checked before any writes.
        if env.storage().instance().has(&DataKey::Config) {
            panic_with_error!(&env, StreakError::AlreadyInitialized);
        }
        // Validate all reward amounts are positive.
        if reward_week <= 0 || reward_month <= 0 || reward_century <= 0 {
            panic_with_error!(&env, StreakError::InvalidRewardAmount);
        }
        env.storage().instance().set(
            &DataKey::Config,
            &StreakConfig {
                admin,
                reward_token,
                reward_week,
                reward_month,
                reward_century,
            },
        );
    }

    /// Update reward amounts. Admin only.
    ///
    /// Can be used to adjust incentives as token value changes without
    /// redeploying the contract.
    pub fn update_rewards(
        env: Env,
        caller: Address,
        reward_week: i128,
        reward_month: i128,
        reward_century: i128,
    ) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        if reward_week <= 0 || reward_month <= 0 || reward_century <= 0 {
            panic_with_error!(&env, StreakError::InvalidRewardAmount);
        }

        let mut config = Self::load_config(&env);
        config.reward_week = reward_week;
        config.reward_month = reward_month;
        config.reward_century = reward_century;
        env.storage().instance().set(&DataKey::Config, &config);

        StreakEvents::config_updated(&env, &caller, reward_week, reward_month, reward_century);
    }

    // ── Core user actions ────────────────────────────────────────────────────

    /// Record a qualifying savings deposit for `user`.
    ///
    /// - Increments the streak if called on the day immediately following
    ///   the last recorded deposit.
    /// - Resets the streak to 1 if a day was missed.
    /// - Rejects the call if the user has already deposited today
    ///   (`AlreadyDepositedToday`).
    ///
    /// Returns the user's streak length after this deposit.
    ///
    /// # Security
    /// - `user.require_auth()` is the first operation.
    /// - Day-number comparison uses ledger timestamp ÷ SECONDS_PER_DAY, not
    ///   a user-supplied value, preventing time-manipulation attacks.
    pub fn record_deposit(env: Env, user: Address) -> u32 {
        user.require_auth();
        Self::load_config(&env); // Ensure initialized.

        let today = Self::day_number(env.ledger().timestamp());
        let mut record = Self::load_or_default_streak(&env, &user);

        // ── Duplicate deposit guard ──────────────────────────────────────────
        if record.last_deposit_day == today && record.streak_days > 0 {
            panic_with_error!(&env, StreakError::AlreadyDepositedToday);
        }

        // ── Streak logic ─────────────────────────────────────────────────────
        let prev_streak = record.streak_days;

        if record.streak_days == 0 {
            // First ever deposit for this user.
            record.streak_days = 1;
        } else {
            let days_since_last = today.saturating_sub(record.last_deposit_day);
            match days_since_last {
                1 => {
                    // Consecutive day — extend streak.
                    record.streak_days = record
                        .streak_days
                        .checked_add(1)
                        .unwrap_or_else(|| panic_with_error!(&env, StreakError::Overflow));
                }
                0 => {
                    // Same day already handled above; should be unreachable here.
                    panic_with_error!(&env, StreakError::AlreadyDepositedToday);
                }
                _ => {
                    // Missed one or more days — break streak.
                    StreakEvents::streak_broken(&env, &user, prev_streak);
                    record.streak_days = 1;
                }
            }
        }

        record.last_deposit_day = today;
        record.total_deposits = record
            .total_deposits
            .checked_add(1)
            .unwrap_or_else(|| panic_with_error!(&env, StreakError::Overflow));

        Self::save_streak(&env, &user, &record);
        StreakEvents::deposit_recorded(&env, &user, record.streak_days, today);

        record.streak_days
    }

    /// Claim a milestone bonus reward for `user`.
    ///
    /// `tier` must be one of `STREAK_TIER_WEEK` (7), `STREAK_TIER_MONTH` (30),
    /// or `STREAK_TIER_CENTURY` (100).
    ///
    /// The user must have a current streak that is a positive multiple of
    /// `tier`. Each (user, tier, epoch) triple may only be claimed once.
    ///
    /// Returns the bonus amount paid out.
    ///
    /// # Security
    /// - `user.require_auth()` is the first operation.
    /// - Duplicate claim prevention: a `ClaimStamp(user, tier, epoch)` key is
    ///   written to persistent storage before the token transfer, following the
    ///   checks-effects-interactions pattern.
    /// - `epoch` is derived from on-chain state; the user cannot pass a
    ///   fabricated epoch to replay an old claim.
    pub fn claim_reward(env: Env, user: Address, tier: u32) -> i128 {
        user.require_auth();
        let config = Self::load_config(&env);

        // Validate tier and look up reward amount.
        let reward_amount =
            Self::reward_for_tier(&config, tier).unwrap_or_else(|| {
                panic_with_error!(&env, StreakError::NoRewardAvailable)
            });

        let record = Self::load_or_default_streak(&env, &user);

        // User must have a streak that is a positive multiple of the tier.
        if record.streak_days == 0 || record.streak_days % tier != 0 {
            panic_with_error!(&env, StreakError::NoRewardAvailable);
        }

        // Derive the epoch from on-chain streak state.
        let epoch = Self::milestone_epoch(record.streak_days, tier);

        // ── Duplicate claim guard (checks-effects-interactions) ──────────────
        let stamp_key = DataKey::ClaimStamp(user.clone(), tier, epoch);
        if env.storage().persistent().has(&stamp_key) {
            panic_with_error!(&env, StreakError::AlreadyClaimed);
        }
        // Write the stamp BEFORE the token transfer.
        env.storage().persistent().set(&stamp_key, &true);
        env.storage().persistent().extend_ttl(
            &stamp_key,
            PERSISTENT_TTL_BUMP,
            PERSISTENT_TTL_BUMP,
        );

        // ── Update running reward total ───────────────────────────────────────
        let mut updated = record.clone();
        updated.total_rewards_claimed = updated
            .total_rewards_claimed
            .checked_add(reward_amount)
            .unwrap_or_else(|| panic_with_error!(&env, StreakError::Overflow));
        Self::save_streak(&env, &user, &updated);

        // ── Transfer reward token to user ─────────────────────────────────────
        let token_client = token::Client::new(&env, &config.reward_token);
        token_client.transfer(&env.current_contract_address(), &user, &reward_amount);

        StreakEvents::reward_claimed(&env, &user, tier, epoch, reward_amount);

        reward_amount
    }

    // ── Read-only queries ─────────────────────────────────────────────────────

    /// Return the current streak count for `owner`, or 0 if no record exists.
    pub fn get_streak_count(env: Env, owner: Address) -> u32 {
        let key = DataKey::UserStreak(owner);
        env.storage()
            .persistent()
            .get::<_, UserStreakRecord>(&key)
            .map(|record| record.streak_days)
            .unwrap_or(0)
    }

    /// Return the current streak record for `user`, or a zeroed default.
    pub fn get_streak(env: Env, user: Address) -> UserStreakRecord {
        Self::load_or_default_streak(&env, &user)
    }

    /// Return `true` if the user has already claimed the reward for the given
    /// tier at their current milestone epoch.
    pub fn is_claimed(env: Env, user: Address, tier: u32) -> bool {
        let record = Self::load_or_default_streak(&env, &user);
        if record.streak_days == 0 || record.streak_days % tier != 0 {
            return false;
        }
        let epoch = Self::milestone_epoch(record.streak_days, tier);
        let stamp_key = DataKey::ClaimStamp(user, tier, epoch);
        env.storage().persistent().has(&stamp_key)
    }

    /// Return the current contract configuration.
    pub fn get_config(env: Env) -> StreakConfig {
        Self::load_config(&env)
    }
}
#[cfg(test)]
mod streak_tests {
    use super::*;
    use soroban_sdk::{Address, Env, testutils::Ledger};

    fn make_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(86_400); // day 1
        env
    }

    #[test]
    fn test_streak_starts_at_one() {
        let env = make_env();
        let contract_id = env.register(StreakRewardsContract, ());
        let client = StreakRewardsContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let token = Address::generate(&env);
        client.initialize(&admin, &token, &10i128, &50i128, &200i128);
        let user = Address::generate(&env);
        let streak = client.record_deposit(&user);
        assert_eq!(streak, 1);
    }

    #[test]
    fn test_streak_increments_next_day() {
        let env = make_env();
        let contract_id = env.register(StreakRewardsContract, ());
        let client = StreakRewardsContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let token = Address::generate(&env);
        client.initialize(&admin, &token, &10i128, &50i128, &200i128);
        let user = Address::generate(&env);
        client.record_deposit(&user);
        env.ledger().set_timestamp(86_400 * 2);
        let streak = client.record_deposit(&user);
        assert_eq!(streak, 2);
    }

    #[test]
    fn test_streak_resets_on_missed_day() {
        let env = make_env();
        let contract_id = env.register(StreakRewardsContract, ());
        let client = StreakRewardsContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let token = Address::generate(&env);
        client.initialize(&admin, &token, &10i128, &50i128, &200i128);
        let user = Address::generate(&env);
        client.record_deposit(&user);
        // Skip a day (day 3)
        env.ledger().set_timestamp(86_400 * 3);
        let streak = client.record_deposit(&user);
        assert_eq!(streak, 1);
    }

    #[test]
    fn test_get_streak_count() {
        let env = make_env();
        let contract_id = env.register(StreakRewardsContract, ());
        let client = StreakRewardsContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let token = Address::generate(&env);
        client.initialize(&admin, &token, &10i128, &50i128, &200i128);
        let user = Address::generate(&env);
        let unused_user = Address::generate(&env);

        assert_eq!(client.get_streak_count(&unused_user), 0);

        client.record_deposit(&user);
        assert_eq!(client.get_streak_count(&user), 1);

        env.ledger().set_timestamp(86_400 * 2);
        client.record_deposit(&user);
        assert_eq!(client.get_streak_count(&user), 2);
    }
}

use soroban_sdk::{contracttype, Address, Env, Symbol};

pub const MAX_BATCH_SIZE: u32 = 100;
pub const MAX_FEE_BPS: u32 = 10_000;

/// Default fee basis points (5% = 500 bps)
pub const DEFAULT_FEE_BPS: u32 = 500;

/// Default minimum fee (0)
pub const DEFAULT_MIN_FEE: i128 = 0;

/// Default maximum fee (1,000,000)
pub const DEFAULT_MAX_FEE: i128 = 1_000_000;

/// Default lock state at initialization.
pub const DEFAULT_LOCKED: bool = false;

/// Panic message used when a required initialization key is missing.
pub const NOT_INITIALIZED_MSG: &str = "Contract not initialized";

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct BatchFeeResult {
    pub batch_size: u32,
    pub total_amount: i128,
    pub cycle: u64,
    pub pending_fees: i128,
}

/// Canonical registry of all storage keys used by the fee contract.
///
/// Every read or write to contract storage MUST go through a variant of this
/// enum (typically via the helper functions defined below). New storage slots
/// must be added here so that there is a single source of truth for which
/// keys exist and how they are scoped.
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    // --- Configuration (instance storage, set at init / by admin) ---
    /// Contract administrator address.
    Admin,
    /// Address of the fee-bearing token.
    Token,
    /// Treasury address that receives released fees.
    Treasury,
    /// Current fee rate in basis points.
    FeeBps,
    /// Minimum absolute fee charged per collection.
    MinFee,
    /// Maximum absolute fee charged per collection.
    MaxFee,
    /// Lock flag that pauses configuration changes.
    IsLocked,

    // --- Runtime state (instance storage, mutated by contract logic) ---
    /// Current fee cycle identifier.
    CurrentCycle,
    /// Total fees held in escrow across all cycles.
    EscrowBalance,
    /// Lifetime sum of fees collected.
    TotalCollected,
    /// Lifetime sum of fees released to treasury.
    TotalReleased,
    /// Lifetime count of batch calls.
    TotalBatchCalls,

    // --- Per-cycle data (persistent storage) ---
    /// Pending fees keyed by cycle id.
    PendingFees(u64),

    // --- Per-user data (persistent storage) ---
    /// Last activity timestamp keyed by user address.
    UserActivity(Address),
    /// User tier symbol keyed by user address.
    UserTier(Address),
}

pub fn has_admin(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

pub fn write_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

pub fn read_admin(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .expect(NOT_INITIALIZED_MSG)
}

pub fn write_token(env: &Env, token: &Address) {
    env.storage().instance().set(&DataKey::Token, token);
}

pub fn read_token(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::Token)
        .expect(NOT_INITIALIZED_MSG)
}

pub fn write_treasury(env: &Env, treasury: &Address) {
    env.storage().instance().set(&DataKey::Treasury, treasury);
}

pub fn read_treasury(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::Treasury)
        .expect(NOT_INITIALIZED_MSG)
}

pub fn write_fee_bps(env: &Env, fee_bps: u32) {
    env.storage().instance().set(&DataKey::FeeBps, &fee_bps);
}

pub fn read_fee_bps(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::FeeBps)
        .expect(NOT_INITIALIZED_MSG)
}

pub fn write_min_fee(env: &Env, min_fee: i128) {
    env.storage().instance().set(&DataKey::MinFee, &min_fee);
}

pub fn read_min_fee(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::MinFee)
        .unwrap_or(DEFAULT_MIN_FEE)
}

pub fn write_max_fee(env: &Env, max_fee: i128) {
    env.storage().instance().set(&DataKey::MaxFee, &max_fee);
}

pub fn read_max_fee(env: &Env) -> i128 {
    env.storage().instance().get(&DataKey::MaxFee).unwrap_or(DEFAULT_MAX_FEE)
}

pub fn write_locked(env: &Env, is_locked: bool) {
    env.storage().instance().set(&DataKey::IsLocked, &is_locked);
}

pub fn read_locked(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::IsLocked)
        .unwrap_or(DEFAULT_LOCKED)
}

pub fn write_current_cycle(env: &Env, cycle: u64) {
    env.storage().instance().set(&DataKey::CurrentCycle, &cycle);
}

pub fn read_current_cycle(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::CurrentCycle)
        .expect(NOT_INITIALIZED_MSG)
}

pub fn read_pending_fees(env: &Env, cycle: u64) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::PendingFees(cycle))
        .unwrap_or(0)
}

pub fn write_pending_fees(env: &Env, cycle: u64, amount: i128) {
    env.storage()
        .persistent()
        .set(&DataKey::PendingFees(cycle), &amount);
}

pub fn add_pending_fees(env: &Env, cycle: u64, amount: i128) -> Option<i128> {
    let updated = read_pending_fees(env, cycle).checked_add(amount)?;
    write_pending_fees(env, cycle, updated);
    Some(updated)
}

pub fn clear_pending_fees(env: &Env, cycle: u64) {
    write_pending_fees(env, cycle, 0);
}

pub fn read_escrow_balance(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::EscrowBalance)
        .unwrap_or(0)
}

pub fn add_escrow_balance(env: &Env, amount: i128) -> Option<i128> {
    let updated = read_escrow_balance(env).checked_add(amount)?;
    env.storage()
        .instance()
        .set(&DataKey::EscrowBalance, &updated);
    Some(updated)
}

pub fn sub_escrow_balance(env: &Env, amount: i128) -> Option<i128> {
    let updated = read_escrow_balance(env).checked_sub(amount)?;
    env.storage()
        .instance()
        .set(&DataKey::EscrowBalance, &updated);
    Some(updated)
}

pub fn read_total_collected(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::TotalCollected)
        .unwrap_or(0)
}

pub fn add_total_collected(env: &Env, amount: i128) -> Option<i128> {
    let updated = read_total_collected(env).checked_add(amount)?;
    env.storage()
        .instance()
        .set(&DataKey::TotalCollected, &updated);
    Some(updated)
}

pub fn read_total_released(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::TotalReleased)
        .unwrap_or(0)
}

pub fn add_total_released(env: &Env, amount: i128) -> Option<i128> {
    let updated = read_total_released(env).checked_add(amount)?;
    env.storage()
        .instance()
        .set(&DataKey::TotalReleased, &updated);
    Some(updated)
}

pub fn read_total_batch_calls(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::TotalBatchCalls)
        .unwrap_or(0)
}

pub fn add_batch_call(env: &Env) -> Option<u64> {
    let updated = read_total_batch_calls(env).checked_add(1)?;
    env.storage()
        .instance()
        .set(&DataKey::TotalBatchCalls, &updated);
    Some(updated)
}

pub fn read_last_active(env: &Env, user: &Address) -> u64 {
    env.storage()
        .persistent()
        .get(&DataKey::UserActivity(user.clone()))
        .unwrap_or(0)
}

pub fn write_last_active(env: &Env, user: &Address, timestamp: u64) {
    env.storage()
        .persistent()
        .set(&DataKey::UserActivity(user.clone()), &timestamp);
}

// ---------------------------------------------------------------------------
// User tier storage helpers
// ---------------------------------------------------------------------------

/// Predefined valid tier symbols.
pub const TIER_BRONZE: &str = "bronze";
pub const TIER_SILVER: &str = "silver";
pub const TIER_GOLD: &str = "gold";
pub const TIER_PLATINUM: &str = "platinum";

/// Returns true if `tier` is one of the predefined valid tiers.
pub fn is_valid_tier(env: &Env, tier: &Symbol) -> bool {
    let bronze = Symbol::new(env, TIER_BRONZE);
    let silver = Symbol::new(env, TIER_SILVER);
    let gold = Symbol::new(env, TIER_GOLD);
    let platinum = Symbol::new(env, TIER_PLATINUM);
    *tier == bronze || *tier == silver || *tier == gold || *tier == platinum
}

pub fn write_user_tier(env: &Env, user: &Address, tier: &Symbol) {
    env.storage()
        .persistent()
        .set(&DataKey::UserTier(user.clone()), tier);
}

pub fn read_user_tier(env: &Env, user: &Address) -> Option<Symbol> {
    env.storage()
        .persistent()
        .get(&DataKey::UserTier(user.clone()))
}

pub fn remove_user_tier(env: &Env, user: &Address) {
    env.storage()
        .persistent()
        .remove(&DataKey::UserTier(user.clone()));
}

/// Check if fee configuration exists (has admin and fee_bps set)
pub fn has_fee_config(env: &Env) -> bool {
    has_admin(env) && env.storage().instance().has(&DataKey::FeeBps)
}

use soroban_sdk::{contracttype, Address, Env, Symbol};

pub const MAX_BATCH_SIZE: u32 = 100;
pub const MAX_FEE_BPS: u32 = 10_000;

/// Default fee basis points (5% = 500 bps)
pub const DEFAULT_FEE_BPS: u32 = 500;

/// Default minimum fee (0)
pub const DEFAULT_MIN_FEE: i128 = 0;

/// Default maximum fee (1,000,000)
pub const DEFAULT_MAX_FEE: i128 = 1_000_000;

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct BatchFeeResult {
    pub batch_size: u32,
    pub total_amount: i128,
    pub cycle: u64,
    pub pending_fees: i128,
}

// ─── Consolidated Config Struct (Storage Optimization #484) ──────────────────
//
// Previously, 8 separate instance-storage keys were used for:
//   Admin, Token, Treasury, FeeBps, MinFee, MaxFee, IsLocked, CurrentCycle.
//
// Consolidating them into a single FeeConfig struct reduces instance-storage
// reads from up to 8 individual operations to 1, significantly lowering
// Soroban storage I/O overhead and rent costs.

#[derive(Clone)]
#[contracttype]
pub struct FeeConfig {
    pub admin: Address,
    pub token: Address,
    pub treasury: Address,
    pub fee_bps: u32,
    pub min_fee: i128,
    pub max_fee: i128,
    pub is_locked: bool,
    pub current_cycle: u64,
}

// ─── Consolidated Stats Struct (Storage Optimization #484) ───────────────────
//
// Previously, 4 separate instance-storage keys were used for:
//   EscrowBalance, TotalCollected, TotalReleased, TotalBatchCalls.
//
// Consolidating them into a single FeeStats struct reduces reads from 4 to 1.

#[derive(Clone)]
#[contracttype]
pub struct FeeStats {
    pub escrow_balance: i128,
    pub total_collected: i128,
    pub total_released: i128,
    pub total_batch_calls: u64,
}

// ─── Storage Keys ────────────────────────────────────────────────────────────

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    /// Consolidated fee configuration (Admin, Token, Treasury, FeeBps, MinFee,
    /// MaxFee, IsLocked, CurrentCycle).
    FeeConfig,
    /// Consolidated fee statistics (EscrowBalance, TotalCollected,
    /// TotalReleased, TotalBatchCalls).
    FeeStats,
    /// Per-cycle pending fees.
    PendingFees(u64),
    /// Per-user last activity timestamp.
    UserActivity(Address),
    /// Per-user fee tier.
    UserTier(Address),
}

// ─── Config Helpers ──────────────────────────────────────────────────────────

fn read_config(env: &Env) -> FeeConfig {
    env.storage()
        .instance()
        .get(&DataKey::FeeConfig)
        .expect("Contract not initialized")
}

fn write_config(env: &Env, config: &FeeConfig) {
    env.storage().instance().set(&DataKey::FeeConfig, config);
}

fn read_stats(env: &Env) -> FeeStats {
    env.storage()
        .instance()
        .get(&DataKey::FeeStats)
        .unwrap_or(FeeStats {
            escrow_balance: 0,
            total_collected: 0,
            total_released: 0,
            total_batch_calls: 0,
        })
}

fn write_stats(env: &Env, stats: &FeeStats) {
    env.storage().instance().set(&DataKey::FeeStats, stats);
}

// ─── Public config accessors ─────────────────────────────────────────────────

pub fn has_admin(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::FeeConfig)
}

pub fn write_admin(env: &Env, admin: &Address) {
    let mut config = read_config(env);
    config.admin = admin.clone();
    write_config(env, &config);
}

pub fn read_admin(env: &Env) -> Address {
    read_config(env).admin
}

pub fn write_token(env: &Env, token: &Address) {
    let mut config = read_config(env);
    config.token = token.clone();
    write_config(env, &config);
}

pub fn read_token(env: &Env) -> Address {
    read_config(env).token
}

pub fn write_treasury(env: &Env, treasury: &Address) {
    let mut config = read_config(env);
    config.treasury = treasury.clone();
    write_config(env, &config);
}

pub fn read_treasury(env: &Env) -> Address {
    read_config(env).treasury
}

pub fn write_fee_bps(env: &Env, fee_bps: u32) {
    let mut config = read_config(env);
    config.fee_bps = fee_bps;
    write_config(env, &config);
}

pub fn read_fee_bps(env: &Env) -> u32 {
    if has_admin(env) {
        read_config(env).fee_bps
    } else {
        DEFAULT_FEE_BPS
    }
}

pub fn write_min_fee(env: &Env, min_fee: i128) {
    let mut config = read_config(env);
    config.min_fee = min_fee;
    write_config(env, &config);
}

pub fn read_min_fee(env: &Env) -> i128 {
    if has_admin(env) {
        read_config(env).min_fee
    } else {
        DEFAULT_MIN_FEE
    }
}

pub fn write_max_fee(env: &Env, max_fee: i128) {
    let mut config = read_config(env);
    config.max_fee = max_fee;
    write_config(env, &config);
}

pub fn read_max_fee(env: &Env) -> i128 {
    if has_admin(env) {
        read_config(env).max_fee
    } else {
        DEFAULT_MAX_FEE
    }
}

pub fn write_locked(env: &Env, is_locked: bool) {
    let mut config = read_config(env);
    config.is_locked = is_locked;
    write_config(env, &config);
}

pub fn read_locked(env: &Env) -> bool {
    read_config(env).is_locked
}

pub fn write_current_cycle(env: &Env, cycle: u64) {
    let mut config = read_config(env);
    config.current_cycle = cycle;
    write_config(env, &config);
}

pub fn read_current_cycle(env: &Env) -> u64 {
    read_config(env).current_cycle
}

// ─── Stats helpers ───────────────────────────────────────────────────────────

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
    read_stats(env).escrow_balance
}

pub fn add_escrow_balance(env: &Env, amount: i128) -> Option<i128> {
    let mut stats = read_stats(env);
    let updated = stats.escrow_balance.checked_add(amount)?;
    stats.escrow_balance = updated;
    write_stats(env, &stats);
    Some(updated)
}

pub fn sub_escrow_balance(env: &Env, amount: i128) -> Option<i128> {
    let mut stats = read_stats(env);
    let updated = stats.escrow_balance.checked_sub(amount)?;
    stats.escrow_balance = updated;
    write_stats(env, &stats);
    Some(updated)
}

pub fn read_total_collected(env: &Env) -> i128 {
    read_stats(env).total_collected
}

pub fn add_total_collected(env: &Env, amount: i128) -> Option<i128> {
    let mut stats = read_stats(env);
    let updated = stats.total_collected.checked_add(amount)?;
    stats.total_collected = updated;
    write_stats(env, &stats);
    Some(updated)
}

pub fn read_total_released(env: &Env) -> i128 {
    read_stats(env).total_released
}

pub fn add_total_released(env: &Env, amount: i128) -> Option<i128> {
    let mut stats = read_stats(env);
    let updated = stats.total_released.checked_add(amount)?;
    stats.total_released = updated;
    write_stats(env, &stats);
    Some(updated)
}

pub fn read_total_batch_calls(env: &Env) -> u64 {
    read_stats(env).total_batch_calls
}

pub fn add_batch_call(env: &Env) -> Option<u64> {
    let mut stats = read_stats(env);
    let updated = stats.total_batch_calls.checked_add(1)?;
    stats.total_batch_calls = updated;
    write_stats(env, &stats);
    Some(updated)
}

// ─── User activity helpers ───────────────────────────────────────────────────

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

// ─── User tier helpers ───────────────────────────────────────────────────────

pub const TIER_BRONZE: &str = "bronze";
pub const TIER_SILVER: &str = "silver";
pub const TIER_GOLD: &str = "gold";
pub const TIER_PLATINUM: &str = "platinum";

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

/// Check if fee configuration exists (has FeeConfig set).
pub fn has_fee_config(env: &Env) -> bool {
    has_admin(env)
}

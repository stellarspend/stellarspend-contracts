use soroban_sdk::{contracttype, Address, Env};

pub const MAX_BATCH_SIZE: u32 = 100;
pub const MAX_FEE_BPS: u32 = 10_000;

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct BatchFeeResult {
    pub batch_size: u32,
    pub total_amount: i128,
    pub cycle: u64,
    pub pending_fees: i128,
}

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    Token,
    Treasury,
    FeeBps,
    MinFee,
    IsLocked,
    CurrentCycle,
    EscrowBalance,
    TotalCollected,
    TotalReleased,
    TotalBatchCalls,
    PendingFees(u64),
    UserActivity(Address),
    UserFeeOverride(Address),
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
        .expect("Contract not initialized")
}

pub fn write_token(env: &Env, token: &Address) {
    env.storage().instance().set(&DataKey::Token, token);
}

pub fn read_token(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::Token)
        .expect("Contract not initialized")
}

pub fn write_treasury(env: &Env, treasury: &Address) {
    env.storage().instance().set(&DataKey::Treasury, treasury);
}

pub fn read_treasury(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::Treasury)
        .expect("Contract not initialized")
}

pub fn write_fee_bps(env: &Env, fee_bps: u32) {
    env.storage().instance().set(&DataKey::FeeBps, &fee_bps);
}

pub fn read_fee_bps(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::FeeBps)
        .expect("Contract not initialized")
}

pub fn write_min_fee(env: &Env, min_fee: i128) {
    env.storage().instance().set(&DataKey::MinFee, &min_fee);
}

pub fn read_min_fee(env: &Env) -> i128 {
    env.storage().instance().get(&DataKey::MinFee).unwrap_or(0)
}

pub fn write_locked(env: &Env, is_locked: bool) {
    env.storage().instance().set(&DataKey::IsLocked, &is_locked);
}

pub fn read_locked(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::IsLocked)
        .unwrap_or(false)
}

pub fn write_current_cycle(env: &Env, cycle: u64) {
    env.storage().instance().set(&DataKey::CurrentCycle, &cycle);
}

pub fn read_current_cycle(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::CurrentCycle)
        .expect("Contract not initialized")
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

pub fn read_user_fee_override(env: &Env, user: &Address) -> Option<u32> {
    env.storage()
        .instance()
        .get(&DataKey::UserFeeOverride(user.clone()))
}

pub fn write_user_fee_override(env: &Env, user: &Address, fee_bps: u32) {
    env.storage()
        .instance()
        .set(&DataKey::UserFeeOverride(user.clone()), &fee_bps);
}

pub fn remove_user_fee_override(env: &Env, user: &Address) {
    env.storage()
        .instance()
        .remove(&DataKey::UserFeeOverride(user.clone()));
}

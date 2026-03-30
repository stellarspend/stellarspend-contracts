#![no_std]

mod decay;
mod escrow;
mod storage;
mod validation;

use soroban_sdk::{contract, contractimpl, panic_with_error, symbol_short, Address, Env, Vec};

use crate::decay::{calculate_fee_decay, DECAY_RATE, MIN_FEE, MAX_FEE};
use crate::escrow::{
    collect_batch_to_escrow, collect_to_escrow, release_cycle_fees, rollover_cycle_fees,
};
use crate::storage::{
    has_admin, read_admin, read_current_cycle, read_escrow_balance, read_fee_bps, read_last_active,
    read_locked, read_min_fee, read_pending_fees, read_token, read_total_batch_calls,
    read_total_collected, read_total_released, read_treasury, read_user_fee_override,
    remove_user_fee_override, write_admin, write_current_cycle, write_fee_bps, write_last_active,
    write_locked, write_min_fee, write_token, write_treasury, write_user_fee_override,
};
pub use crate::storage::{BatchFeeResult, DataKey, MAX_BATCH_SIZE, MAX_FEE_BPS};
use crate::validation::{validate_fee_bps_or_panic, validate_min_fee_or_panic};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum FeeContractError {
    NotInitialized = 1,
    Unauthorized = 2,
    Locked = 3,
    InvalidAmount = 4,
    EmptyBatch = 5,
    BatchTooLarge = 6,
    Overflow = 7,
    InsufficientEscrow = 8,
    InvalidCycle = 9,
    InvalidConfig = 10,
    NoPendingFees = 11,
}

impl From<FeeContractError> for soroban_sdk::Error {
    fn from(value: FeeContractError) -> Self {
        soroban_sdk::Error::from_contract_error(value as u32)
    }
}

pub struct FeeEvents;

impl FeeEvents {
    pub fn fee_escrowed(env: &Env, payer: &Address, amount: i128, cycle: u64) {
        let topics = (symbol_short!("fee"), symbol_short!("escrowed"));
        env.events().publish(topics, (payer.clone(), amount, cycle));
    }

    pub fn fee_batched(
        env: &Env,
        payer: &Address,
        total_amount: i128,
        batch_size: u32,
        cycle: u64,
    ) {
        let topics = (symbol_short!("fee"), symbol_short!("batched"));
        env.events()
            .publish(topics, (payer.clone(), total_amount, batch_size, cycle));
    }

    pub fn fee_released(env: &Env, cycle: u64, amount: i128, treasury: &Address) {
        let topics = (symbol_short!("fee"), symbol_short!("released"));
        env.events()
            .publish(topics, (cycle, amount, treasury.clone()));
    }

    pub fn fee_rolled(env: &Env, from_cycle: u64, to_cycle: u64, amount: i128) {
        let topics = (symbol_short!("fee"), symbol_short!("rollover"));
        env.events().publish(topics, (from_cycle, to_cycle, amount));
    }

    pub fn locked(env: &Env) {
        let topics = (symbol_short!("fee"), symbol_short!("locked"));
        env.events().publish(topics, ());
    }

    pub fn unlocked(env: &Env) {
        let topics = (symbol_short!("fee"), symbol_short!("unlocked"));
        env.events().publish(topics, ());
    }

    pub fn fee_bps_updated(env: &Env, fee_bps: u32) {
        let topics = (symbol_short!("fee"), symbol_short!("config"));
        env.events()
            .publish(topics, (symbol_short!("bps"), fee_bps));
    }

    pub fn treasury_updated(env: &Env, treasury: &Address) {
        let topics = (symbol_short!("fee"), symbol_short!("config"));
        env.events()
            .publish(topics, (symbol_short!("treasury"), treasury.clone()));
    }

    pub fn min_fee_updated(env: &Env, min_fee: i128) {
        let topics = (symbol_short!("fee"), symbol_short!("config"));
        env.events()
            .publish(topics, (symbol_short!("min_fee"), min_fee));
    }

    pub fn user_fee_override_set(env: &Env, user: &Address, fee_bps: u32) {
        let topics = (symbol_short!("fee"), symbol_short!("override"));
        env.events()
            .publish(topics, (symbol_short!("set"), user.clone(), fee_bps));
    }

    pub fn user_fee_override_removed(env: &Env, user: &Address) {
        let topics = (symbol_short!("fee"), symbol_short!("override"));
        env.events()
            .publish(topics, (symbol_short!("remove"), user.clone()));
    }
}

#[contract]
pub struct FeeContract;

#[contractimpl]
impl FeeContract {
    pub fn initialize(
        env: Env,
        admin: Address,
        token: Address,
        treasury: Address,
        fee_bps: u32,
        initial_cycle: u64,
    ) {
        if has_admin(&env) {
            panic!("Contract already initialized");
        }
        if initial_cycle == 0 {
            panic_with_error!(&env, FeeContractError::InvalidConfig);
        }
        if !validate_fee_bps_or_panic(&env, fee_bps) {
            panic_with_error!(&env, FeeContractError::InvalidConfig);
        }

        write_admin(&env, &admin);
        write_token(&env, &token);
        write_treasury(&env, &treasury);
        write_fee_bps(&env, fee_bps);
        write_locked(&env, false);
        write_current_cycle(&env, initial_cycle);
    }

    pub fn collect_fee(env: Env, payer: Address, amount: i128) -> i128 {
        payer.require_auth();
        
        let last_active = read_last_active(&env, &payer);
        let current_time = env.ledger().timestamp();
        let decayed_amount = calculate_fee_decay(&env, amount, last_active, current_time);

        let pending = collect_to_escrow(&env, &payer, decayed_amount);
        
        write_last_active(&env, &payer, current_time);
        
        FeeEvents::fee_escrowed(&env, &payer, decayed_amount, read_current_cycle(&env));
        pending
    }

    pub fn collect_fee_batch(env: Env, payer: Address, amounts: Vec<i128>) -> BatchFeeResult {
        payer.require_auth();

        let batch_size = amounts.len();
        if batch_size == 0 {
            panic_with_error!(&env, FeeContractError::EmptyBatch);
        }
        if batch_size > MAX_BATCH_SIZE {
            panic_with_error!(&env, FeeContractError::BatchTooLarge);
        }

        let last_active = read_last_active(&env, &payer);
        let current_time = env.ledger().timestamp();

        let mut decayed_amounts = Vec::new(&env);
        for amount in amounts.iter() {
            decayed_amounts.push_back(calculate_fee_decay(&env, amount, last_active, current_time));
        }

        let result = collect_batch_to_escrow(&env, &payer, &decayed_amounts);
        
        write_last_active(&env, &payer, current_time);

        FeeEvents::fee_batched(
            &env,
            &payer,
            result.total_amount,
            result.batch_size,
            result.cycle,
        );
        result
    }

    pub fn update_activity(env: Env, user: Address) {
        user.require_auth();
        write_last_active(&env, &user, env.ledger().timestamp());
    }

    pub fn get_last_active(env: Env, user: Address) -> u64 {
        read_last_active(&env, &user)
    }

    pub fn release_fees(env: Env, admin: Address, cycle: u64) -> i128 {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        let released = release_cycle_fees(&env, cycle);
        FeeEvents::fee_released(&env, cycle, released, &read_treasury(&env));
        released
    }

    pub fn rollover_fees(env: Env, admin: Address, next_cycle: u64) -> i128 {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        let current_cycle = read_current_cycle(&env);
        if next_cycle <= current_cycle {
            panic_with_error!(&env, FeeContractError::InvalidCycle);
        }

        let rolled = rollover_cycle_fees(&env, current_cycle, next_cycle);
        write_current_cycle(&env, next_cycle);
        FeeEvents::fee_rolled(&env, current_cycle, next_cycle, rolled);
        rolled
    }

    pub fn lock(env: Env, admin: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        write_locked(&env, true);
        FeeEvents::locked(&env);
    }

    pub fn unlock(env: Env, admin: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        write_locked(&env, false);
        FeeEvents::unlocked(&env);
    }

    pub fn set_fee_bps(env: Env, admin: Address, fee_bps: u32) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        Self::require_unlocked(&env);

        validate_fee_bps_or_panic(&env, fee_bps);

        write_fee_bps(&env, fee_bps);
        FeeEvents::fee_bps_updated(&env, fee_bps);
    }

    pub fn set_treasury(env: Env, admin: Address, treasury: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        Self::require_unlocked(&env);

        write_treasury(&env, &treasury);
        FeeEvents::treasury_updated(&env, &treasury);
    }

    pub fn set_min_fee(env: Env, admin: Address, min_fee: i128) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        Self::require_unlocked(&env);

        validate_min_fee_or_panic(&env, min_fee);

        write_min_fee(&env, min_fee);
        FeeEvents::min_fee_updated(&env, min_fee);
    }

    pub fn get_admin(env: Env) -> Address {
        read_admin(&env)
    }

    pub fn get_token(env: Env) -> Address {
        read_token(&env)
    }

    pub fn get_treasury(env: Env) -> Address {
        read_treasury(&env)
    }

    pub fn get_fee_bps(env: Env) -> u32 {
        read_fee_bps(&env)
    }

    pub fn get_min_fee(env: Env) -> i128 {
        read_min_fee(&env)
    }

    pub fn is_locked(env: Env) -> bool {
        read_locked(&env)
    }

    pub fn get_current_cycle(env: Env) -> u64 {
        read_current_cycle(&env)
    }

    pub fn get_escrow_balance(env: Env) -> i128 {
        read_escrow_balance(&env)
    }

    pub fn get_pending_fees(env: Env, cycle: u64) -> i128 {
        read_pending_fees(&env, cycle)
    }

    pub fn get_total_collected(env: Env) -> i128 {
        read_total_collected(&env)
    }

    pub fn get_total_released(env: Env) -> i128 {
        read_total_released(&env)
    }

    pub fn get_total_batch_calls(env: Env) -> u64 {
        read_total_batch_calls(&env)
    }

    /// Preview the total fees for a batch of operations without mutating state.
    ///
    /// This is a view/read method intended for clients to estimate the aggregate fee
    /// they will be charged when submitting a batch via `collect_fee_batch`. It performs
    /// identical validations (non-empty, size cap, per-item minimum and positivity) but
    /// does not transfer tokens or write to storage.
    ///
    /// Validations mirror `collect_fee_batch`:
    /// - Batch must be non-empty and not exceed `MAX_BATCH_SIZE`
    /// - Each item must be positive and meet the configured `min_fee`
    ///
    /// Returns the sum of all amounts if valid.
    pub fn preview_batch_fee(env: Env, _user: Address, amounts: Vec<i128>) -> i128 {
        let batch_size = amounts.len();
        if batch_size == 0 {
            panic_with_error!(&env, FeeContractError::EmptyBatch);
        }
        if batch_size > MAX_BATCH_SIZE {
            panic_with_error!(&env, FeeContractError::BatchTooLarge);
        }

        let min_fee = read_min_fee(&env);
        let mut total: i128 = 0;
        for amount in amounts.iter() {
            if amount <= 0 {
                panic_with_error!(&env, FeeContractError::InvalidAmount);
            }
            if amount < min_fee {
                panic_with_error!(&env, FeeContractError::InvalidAmount);
            }
            total = total
                .checked_add(amount)
                .unwrap_or_else(|| panic_with_error!(&env, FeeContractError::Overflow));
        }
        total
    }

    /// Calculate the fee for a given amount, checking user-specific overrides first.
    /// If a user has an override, it takes precedence over the global fee_bps.
    pub fn calculate_fee(env: Env, user: Address, amount: i128) -> i128 {
        if amount <= 0 {
            panic_with_error!(&env, FeeContractError::InvalidAmount);
        }

        let fee_bps = read_user_fee_override(&env, &user).unwrap_or_else(|| read_fee_bps(&env));

        let min_fee = read_min_fee(&env);
        let raw_fee = amount
            .checked_mul(fee_bps as i128)
            .unwrap_or_else(|| panic_with_error!(&env, FeeContractError::Overflow))
            .checked_div(10_000)
            .unwrap_or_else(|| panic_with_error!(&env, FeeContractError::Overflow));

        if raw_fee < min_fee {
            min_fee
        } else {
            raw_fee
        }
    }

    /// Set a user-specific fee override. Only admin can call.
    pub fn set_user_fee_override(env: Env, admin: Address, user: Address, fee_bps: u32) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        Self::require_unlocked(&env);

        validate_fee_bps_or_panic(&env, fee_bps);

        write_user_fee_override(&env, &user, fee_bps);
        FeeEvents::user_fee_override_set(&env, &user, fee_bps);
    }

    /// Remove a user-specific fee override. Only admin can call.
    pub fn remove_user_fee_override(env: Env, admin: Address, user: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        Self::require_unlocked(&env);

        remove_user_fee_override(&env, &user);
        FeeEvents::user_fee_override_removed(&env, &user);
    }

    /// Get the effective fee rate for a user (override if exists, otherwise global).
    pub fn get_user_fee_bps(env: Env, user: Address) -> u32 {
        read_user_fee_override(&env, &user).unwrap_or_else(|| read_fee_bps(&env))
    }

    /// Validate a configuration tuple. Returns true or panics on invalid inputs.
    ///
    /// Current checks:
    /// - `fee_bps` within [0, MAX_FEE_BPS]
    /// - `min_fee` >= 0
    /// Extend this as new fee knobs are added.
    pub fn validate_config(env: Env, fee_bps: u32, min_fee: i128) -> bool {
        validate_fee_bps_or_panic(&env, fee_bps);
        validate_min_fee_or_panic(&env, min_fee);
        true
    }

    fn require_admin(env: &Env, caller: &Address) {
        if !has_admin(env) {
            panic_with_error!(env, FeeContractError::NotInitialized);
        }

        let admin = read_admin(env);
        if admin != *caller {
            panic_with_error!(env, FeeContractError::Unauthorized);
        }
    }

    fn require_unlocked(env: &Env) {
        if read_locked(env) {
            panic_with_error!(env, FeeContractError::Locked);
        }
    }
}

#![no_std]

extern crate alloc;

mod auth;
mod decay;
mod escrow;
mod events;
mod fee_validation;
mod reconciliation;
mod storage;
mod utils;
mod validation;

#[cfg(test)]
mod test;

use soroban_sdk::{contract, contractimpl, panic_with_error, Address, Env, Symbol, Vec};

use crate::auth::require_admin;
use crate::decay::calculate_fee_decay;
use crate::escrow::{
    collect_batch_to_escrow, collect_to_escrow, release_cycle_fees, rollover_cycle_fees,
};
use crate::events::{ConfigEvents, FeeEvents, TierEvents};
use crate::fee_validation::validate_fee_percentage_bounds;
use crate::reconciliation::reconcile;
pub use crate::reconciliation::ReconciliationResult;
use crate::storage::{
    has_admin, is_valid_tier, read_admin, read_current_cycle, read_escrow_balance, read_fee_bps,
    read_last_active, read_locked, read_max_fee, read_min_fee, read_pending_fees, read_token,
    read_total_batch_calls, read_total_collected, read_total_released, read_treasury,
    read_user_tier, remove_user_tier as delete_user_tier, write_current_cycle, write_fee_bps,
    write_last_active, write_locked, write_max_fee, write_min_fee, write_treasury, write_user_tier,
    FeeConfig, FeeStats, DEFAULT_FEE_BPS, DEFAULT_MAX_FEE, DEFAULT_MIN_FEE,
};
pub use crate::storage::{BatchFeeResult, DataKey, MAX_BATCH_SIZE, MAX_FEE_BPS};
use crate::validation::{
    validate_amount_positive_or_panic, validate_max_fee_or_panic, validate_min_fee_or_panic,
};

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
    InvalidTier = 12,
}

impl From<FeeContractError> for soroban_sdk::Error {
    fn from(value: FeeContractError) -> Self {
        soroban_sdk::Error::from_contract_error(value as u32)
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
        validate_fee_percentage_bounds(&env, fee_bps);

        // Storage optimization (#484): write FeeConfig once instead of 6
        // separate storage operations.
        let config = FeeConfig {
            admin,
            token,
            treasury,
            fee_bps,
            min_fee: DEFAULT_MIN_FEE,
            max_fee: DEFAULT_MAX_FEE,
            is_locked: false,
            current_cycle: initial_cycle,
        };
        env.storage().instance().set(&DataKey::FeeConfig, &config);

        // Initialize stats.
        let stats = FeeStats {
            escrow_balance: 0,
            total_collected: 0,
            total_released: 0,
            total_batch_calls: 0,
        };
        env.storage().instance().set(&DataKey::FeeStats, &stats);
    }

    /// Initializes the contract with default fee configuration:
    /// - Fee: 3.00% (300 BPS)
    /// - Initial Cycle: 1
    ///
    /// # Storage Optimization (#484)
    /// Writes FeeConfig and FeeStats in 2 operations instead of 10+ separate
    /// storage writes, reducing Soroban storage I/O overhead.
    pub fn init(env: Env, admin: Address, token: Address, treasury: Address) {
        Self::initialize(env, admin, token, treasury, 300, 1);
    }

    pub fn collect_fee(env: Env, payer: Address, amount: i128) -> i128 {
        Self::require_initialized(&env);
        payer.require_auth();
        validate_amount_positive_or_panic(&env, amount);

        let last_active = read_last_active(&env, &payer);
        let current_time = env.ledger().timestamp();
        let decayed_amount = calculate_fee_decay(&env, amount, last_active, current_time);

        let pending = collect_to_escrow(&env, &payer, decayed_amount);

        write_last_active(&env, &payer, current_time);

        FeeEvents::fee_collected(&env, &payer, amount);
        FeeEvents::fee_escrowed(&env, &payer, decayed_amount, read_current_cycle(&env));
        pending
    }

    pub fn collect_fee_batch(env: Env, payer: Address, amounts: Vec<i128>) -> BatchFeeResult {
        Self::require_initialized(&env);
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
        let mut total_original_amount: i128 = 0;
        for amount in amounts.iter() {
            validate_amount_positive_or_panic(&env, amount);
            total_original_amount = total_original_amount
                .checked_add(amount)
                .unwrap_or_else(|| panic_with_error!(&env, FeeContractError::Overflow));
            decayed_amounts.push_back(calculate_fee_decay(&env, amount, last_active, current_time));
        }

        let result = collect_batch_to_escrow(&env, &payer, &decayed_amounts);

        write_last_active(&env, &payer, current_time);

        FeeEvents::fee_collected(&env, &payer, total_original_amount);
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
        Self::require_initialized(&env);
        user.require_auth();
        write_last_active(&env, &user, env.ledger().timestamp());
    }

    pub fn get_last_active(env: Env, user: Address) -> u64 {
        Self::require_initialized(&env);
        read_last_active(&env, &user)
    }

    pub fn release_fees(env: Env, _admin: Address, cycle: u64) -> i128 {
        require_admin(&env, &_admin);

        let released = release_cycle_fees(&env, cycle);
        FeeEvents::fee_released(&env, cycle, released, &read_treasury(&env));
        released
    }

    pub fn rollover_fees(env: Env, _admin: Address, next_cycle: u64) -> i128 {
        require_admin(&env, &_admin);

        let current_cycle = read_current_cycle(&env);
        if next_cycle <= current_cycle {
            panic_with_error!(&env, FeeContractError::InvalidCycle);
        }

        let rolled = rollover_cycle_fees(&env, current_cycle, next_cycle);
        write_current_cycle(&env, next_cycle);
        FeeEvents::fee_rolled(&env, current_cycle, next_cycle, rolled);
        rolled
    }

    pub fn lock(env: Env, _admin: Address) {
        require_admin(&env, &_admin);

        write_locked(&env, true);
        FeeEvents::locked(&env);
    }

    pub fn unlock(env: Env, _admin: Address) {
        require_admin(&env, &_admin);

        write_locked(&env, false);
        FeeEvents::unlocked(&env);
    }

    pub fn set_fee_bps(env: Env, _admin: Address, fee_bps: u32) {
        require_admin(&env, &_admin);
        Self::require_unlocked(&env);

        validate_fee_percentage_bounds(&env, fee_bps);

        write_fee_bps(&env, fee_bps);
        FeeEvents::fee_bps_updated(&env, fee_bps);
    }

    pub fn set_treasury(env: Env, _admin: Address, treasury: Address) {
        require_admin(&env, &_admin);
        Self::require_unlocked(&env);

        write_treasury(&env, &treasury);
        FeeEvents::treasury_updated(&env, &treasury);
    }

    pub fn set_min_fee(env: Env, _admin: Address, min_fee: i128) {
        require_admin(&env, &_admin);
        Self::require_unlocked(&env);

        validate_min_fee_or_panic(&env, min_fee);

        write_min_fee(&env, min_fee);
        FeeEvents::min_fee_updated(&env, min_fee);
    }

    pub fn set_max_fee(env: Env, _admin: Address, max_fee: i128) {
        require_admin(&env, &_admin);
        Self::require_unlocked(&env);

        let min_fee = read_min_fee(&env);
        validate_max_fee_or_panic(&env, max_fee, min_fee);

        write_max_fee(&env, max_fee);
    }

    /// Resets fee configuration to default values. Admin-only.
    /// Restores:
    /// - fee_bps to DEFAULT_FEE_BPS (500 = 5%)
    /// - min_fee to DEFAULT_MIN_FEE (0)
    /// Emits a reset event with the restored default values.
    pub fn reset_fee_config(env: Env, admin: Address) {
        Self::require_admin(&env, &admin);
        Self::require_unlocked(&env);

        write_fee_bps(&env, DEFAULT_FEE_BPS);
        write_min_fee(&env, DEFAULT_MIN_FEE);

        ConfigEvents::fee_reset(&env, &admin);
    }

    pub fn set_user_tier(env: Env, admin: Address, user: Address, tier: Symbol) {
        require_admin(&env, &admin);
        Self::require_unlocked(&env);
        if !is_valid_tier(&env, &tier) {
            panic_with_error!(&env, FeeContractError::InvalidTier);
        }

        write_user_tier(&env, &user, &tier);
        TierEvents::tier_set(&env, &admin, &user, &tier);
    }

    pub fn get_user_tier(env: Env, user: Address) -> Option<Symbol> {
        Self::require_initialized(&env);
        read_user_tier(&env, &user)
    }

    pub fn remove_user_tier(env: Env, admin: Address, user: Address) {
        require_admin(&env, &admin);
        Self::require_unlocked(&env);

        delete_user_tier(&env, &user);
        TierEvents::tier_removed(&env, &admin, &user);
    }

    pub fn calculate_fee_amount(env: Env, amount: i128, fee_bps: u32) -> i128 {
        if amount < 0 {
            panic_with_error!(&env, FeeContractError::InvalidAmount);
        }
        if fee_bps > MAX_FEE_BPS {
            panic_with_error!(&env, FeeContractError::InvalidConfig);
        }

        amount
            .checked_mul(fee_bps as i128)
            .and_then(|value| value.checked_div(10_000))
            .unwrap_or_else(|| panic_with_error!(&env, FeeContractError::Overflow))
    }

    pub fn validate_config(_env: Env, fee_bps: u32, min_fee: i128) -> bool {
        if fee_bps > MAX_FEE_BPS || min_fee < 0 {
            return false;
        }
        true
    }

    pub fn get_admin(env: Env) -> Address {
        Self::require_initialized(&env);
        read_admin(&env)
    }

    pub fn get_token(env: Env) -> Address {
        Self::require_initialized(&env);
        read_token(&env)
    }

    pub fn get_treasury(env: Env) -> Address {
        Self::require_initialized(&env);
        read_treasury(&env)
    }

    pub fn get_fee_bps(env: Env) -> u32 {
        read_fee_bps(&env)
    }

    pub fn get_min_fee(env: Env) -> i128 {
        read_min_fee(&env)
    }

    pub fn get_max_fee(env: Env) -> i128 {
        read_max_fee(&env)
    }

    pub fn is_locked(env: Env) -> bool {
        Self::require_initialized(&env);
        read_locked(&env)
    }

    pub fn get_current_cycle(env: Env) -> u64 {
        Self::require_initialized(&env);
        read_current_cycle(&env)
    }

    pub fn get_escrow_balance(env: Env) -> i128 {
        Self::require_initialized(&env);
        read_escrow_balance(&env)
    }

    /// Returns the current total fee balance stored in the contract.
    /// This is an alias for get_escrow_balance() for clarity.
    pub fn get_fee_balance(env: Env) -> i128 {
        read_escrow_balance(&env)
    }

    pub fn get_pending_fees(env: Env, cycle: u64) -> i128 {
        Self::require_initialized(&env);
        read_pending_fees(&env, cycle)
    }

    pub fn get_total_collected(env: Env) -> i128 {
        Self::require_initialized(&env);
        read_total_collected(&env)
    }

    pub fn get_total_released(env: Env) -> i128 {
        Self::require_initialized(&env);
        read_total_released(&env)
    }

    pub fn get_total_batch_calls(env: Env) -> u64 {
        Self::require_initialized(&env);
        read_total_batch_calls(&env)
    }

    pub fn get_reconciliation_status(env: Env) -> ReconciliationResult {
        Self::require_initialized(&env);
        reconcile(&env)
    }

    pub fn reconcile_fees(env: Env, admin: Address) -> ReconciliationResult {
        require_admin(&env, &admin);

        let result = reconcile(&env);
        if result.is_reconciled {
            FeeEvents::fee_reconciled(&env, result.stored_balance, result.calculated_balance);
        } else {
            FeeEvents::fee_discrepancy(
                &env,
                result.stored_balance,
                result.calculated_balance,
                result.discrepancy,
            );
        }
        result
    }

    pub fn preview_batch_fee(env: Env, _payer: Address, amounts: Vec<i128>) -> i128 {
        Self::require_initialized(&env);

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
            validate_amount_positive_or_panic(&env, amount);
            if amount < min_fee {
                panic_with_error!(&env, FeeContractError::InvalidAmount);
            }
            total = total
                .checked_add(amount)
                .unwrap_or_else(|| panic_with_error!(&env, FeeContractError::Overflow));
        }
        total
    }

    fn require_unlocked(env: &Env) {
        if read_locked(env) {
            panic_with_error!(env, FeeContractError::Locked);
        }
    }

    fn require_initialized(env: &Env) {
        if !has_admin(env) {
            panic_with_error!(env, FeeContractError::NotInitialized);
        }
    }

    fn require_admin(env: &Env, admin: &Address) {
        require_admin(env, admin);
    }
}

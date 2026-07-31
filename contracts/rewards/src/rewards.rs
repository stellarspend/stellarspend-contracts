//! Core reward account management logic.
//!
//! This module contains the business logic for reward account lifecycle
//! operations. Contract entry points in `lib.rs` delegate to these functions
//! after performing authorisation checks.

use soroban_sdk::{Address, Env};

use crate::events::{emit_account_initialized, emit_reward_credited, emit_reward_debited};
use crate::storage::{
    append_reward_index, get_account_stats, get_lifetime_claimed, get_lifetime_earned,
    get_reward_account, get_reward_balance, get_reward_tx_counter, set_account_stats,
    set_lifetime_claimed, set_lifetime_earned, set_reward_account, set_reward_balance,
    set_reward_transaction, set_reward_tx_counter,
};
use crate::types::{
    RewardAccount, RewardAccountStats, RewardStatus, RewardTransaction, RewardType,
};
use crate::validation::{
    validate_account_not_registered, validate_account_registered, validate_contract_initialized,
    validate_reward_amount, validate_sufficient_balance,
};
use crate::RewardsError;

/// Registers a new reward account for `participant` with zeroed default values.
///
/// Creates and persists a [`RewardAccount`] record together with the individual
/// scalar storage entries (`RewardBalance`, `LifetimeEarned`, `LifetimeClaimed`)
/// so that every storage helper returns a consistent `0` immediately after
/// registration.
///
/// # Errors
/// - `NotInitialized` — contract has not been initialised.
/// - `AccountAlreadyRegistered` — an account already exists for `participant`.
pub fn register_reward_account(env: &Env, participant: &Address) -> Result<(), RewardsError> {
    validate_contract_initialized(env)?;
    validate_account_not_registered(env, participant)?;

    let now = env.ledger().sequence() as u64;

    let account = RewardAccount {
        owner: participant.clone(),
        balance: 0,
        lifetime_earned: 0,
        lifetime_claimed: 0,
        created_at: now,
        last_updated: now,
    };

    set_reward_account(env, participant, &account);
    set_reward_balance(env, participant, 0);
    set_lifetime_earned(env, participant, 0);
    set_lifetime_claimed(env, participant, 0);
    set_account_stats(
        env,
        participant,
        &RewardAccountStats {
            total_earned: 0,
            total_redeemed: 0,
            total_transactions: 0,
            last_reward_timestamp: 0,
        },
    );

    emit_account_initialized(env, participant);

    Ok(())
}

/// Credits `amount` reward points to `participant`'s account.
///
/// Atomically updates the claimable balance, the lifetime-earned total, and the
/// [`RewardAccount`] metadata. A [`RewardTransaction`] record is created with
/// status `Confirmed` and the counter is advanced. Both balance and
/// lifetime-earned use `checked_add` to prevent `i128` overflow.
///
/// # Errors
/// - `NotInitialized` — contract has not been initialised.
/// - `AccountNotFound` — `participant` has no reward account.
/// - `InvalidAmount` — `amount` is zero or negative.
/// - `Overflow` — adding `amount` would overflow the balance or lifetime total.
pub fn credit_reward(
    env: &Env,
    participant: &Address,
    amount: i128,
    reward_type: RewardType,
) -> Result<RewardTransaction, RewardsError> {
    validate_contract_initialized(env)?;
    validate_account_registered(env, participant)?;
    validate_reward_amount(amount)?;

    let current_balance = get_reward_balance(env, participant);
    let current_lifetime = get_lifetime_earned(env, participant);

    let new_balance = current_balance
        .checked_add(amount)
        .ok_or(RewardsError::Overflow)?;
    let new_lifetime_earned = current_lifetime
        .checked_add(amount)
        .ok_or(RewardsError::Overflow)?;

    let now = env.ledger().sequence() as u64;

    let mut account = get_reward_account(env, participant).ok_or(RewardsError::AccountNotFound)?;
    account.balance = new_balance;
    account.lifetime_earned = new_lifetime_earned;
    account.last_updated = now;

    set_reward_account(env, participant, &account);
    set_reward_balance(env, participant, new_balance);
    set_lifetime_earned(env, participant, new_lifetime_earned);

    let mut stats = get_account_stats(env, participant);
    stats.total_earned = new_lifetime_earned;
    stats.total_transactions = stats
        .total_transactions
        .checked_add(1)
        .ok_or(RewardsError::Overflow)?;
    stats.last_reward_timestamp = now;
    set_account_stats(env, participant, &stats);

    let tx_id = get_reward_tx_counter(env);
    let tx = RewardTransaction {
        id: tx_id,
        recipient: participant.clone(),
        amount,
        reward_type,
        status: RewardStatus::Confirmed,
        created_at: now,
        updated_at: 0,
    };
    set_reward_transaction(env, tx_id, &tx);
    set_reward_tx_counter(env, tx_id + 1);
    append_reward_index(env, participant, tx_id);

    emit_reward_credited(env, participant, amount, tx_id);

    Ok(tx)
}

/// Returns the current unclaimed reward balance for `owner`.
///
/// Returns `0` if no reward account exists or the balance is zero.
pub fn get_rewards_balance(env: &Env, owner: &Address) -> i128 {
    get_reward_balance(env, owner)
}

/// Debits `amount` reward points from `participant`'s account.
///
/// Atomically reduces the claimable balance, increments the lifetime-claimed
/// total, and persists a [`RewardTransaction`] with status `Claimed`. The
/// counter is advanced and a `reward_debited` event is emitted.
///
/// # Errors
/// - `NotInitialized` — contract has not been initialised.
/// - `AccountNotFound` — `participant` has no reward account.
/// - `InvalidAmount` — `amount` is zero or negative.
/// - `InsufficientBalance` — `amount` exceeds the current claimable balance.
/// - `Overflow` — incrementing lifetime_claimed would overflow `i128`.
pub fn debit_reward(
    env: &Env,
    participant: &Address,
    amount: i128,
    reward_type: RewardType,
) -> Result<RewardTransaction, RewardsError> {
    validate_contract_initialized(env)?;
    validate_account_registered(env, participant)?;
    validate_reward_amount(amount)?;

    let current_balance = get_reward_balance(env, participant);
    validate_sufficient_balance(current_balance, amount)?;

    let current_lifetime_claimed = get_lifetime_claimed(env, participant);

    let new_balance = current_balance
        .checked_sub(amount)
        .ok_or(RewardsError::Overflow)?;
    let new_lifetime_claimed = current_lifetime_claimed
        .checked_add(amount)
        .ok_or(RewardsError::Overflow)?;

    let now = env.ledger().sequence() as u64;

    let mut account = get_reward_account(env, participant).ok_or(RewardsError::AccountNotFound)?;
    account.balance = new_balance;
    account.lifetime_claimed = new_lifetime_claimed;
    account.last_updated = now;

    set_reward_account(env, participant, &account);
    set_reward_balance(env, participant, new_balance);
    set_lifetime_claimed(env, participant, new_lifetime_claimed);

    let mut stats = get_account_stats(env, participant);
    stats.total_redeemed = new_lifetime_claimed;
    stats.total_transactions = stats
        .total_transactions
        .checked_add(1)
        .ok_or(RewardsError::Overflow)?;
    set_account_stats(env, participant, &stats);

    let tx_id = get_reward_tx_counter(env);
    let tx = RewardTransaction {
        id: tx_id,
        recipient: participant.clone(),
        amount,
        reward_type,
        status: RewardStatus::Claimed,
        created_at: now,
        updated_at: now,
    };
    set_reward_transaction(env, tx_id, &tx);
    set_reward_tx_counter(env, tx_id + 1);

    emit_reward_debited(env, participant, amount, tx_id);
    env.events().publish(
        ("rewards", "reward_credited"),
        (participant.clone(), amount, tx_id),
    );

    Ok(tx)
}

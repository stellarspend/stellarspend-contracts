#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, token, Address, Env, Symbol,
};
pub mod fee_events;

mod events;
use events::{
    emit_initialize, emit_stake, emit_unstake,
    InitializeEventData, StakeEventData, UnstakeEventData,
};

#[cfg(test)]
mod test {
    use super::fee_events::*;
    use soroban_sdk::{Env, Address};

    #[test]
    fn test_fee_event_logging() {
        let env = Env::default();
        let user = Address::generate(&env);

        log_fee_collected(&env, user.clone(), 500);

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let event = &events[0];

        let (logged_user, amount, _timestamp): (Address, i128, u64) =
            event.data.clone().try_into().unwrap();

        assert_eq!(logged_user, user);
        assert_eq!(amount, 500);
    }
}
// ─── Storage Keys ─────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Contract-level configuration
    Config,
    /// Per-user staked balance:  DataKey::Stake(Address)
    Stake(Address),
    /// Per-user last-stake timestamp (for reward calculation)
    StakeTs(Address),
}

// ─── Contract State ───────────────────────────────────────────────────────────

/// Persistent contract configuration stored in ledger state.
#[contracttype]
#[derive(Clone, Debug)]
pub struct Config {
    /// Address allowed to call admin-only functions
    pub admin: Address,
    /// The token this contract accepts for staking
    pub token: Address,
    /// Annual reward rate in basis points (e.g. 1200 = 12 %)
    pub reward_rate: u32,
    /// Minimum tokens a user must stake in a single call
    pub min_stake: i128,
}

// ─── Contract ─────────────────────────────────────────────────────────────────

#[contract]
pub struct StakingContract;

#[contractimpl]
impl StakingContract {
    // ── Initialize ────────────────────────────────────────────────────────────

    /// Set up the contract for the first time.
    ///
    /// Must be called exactly once. Subsequent calls will panic because
    /// `Config` is already present in storage.
    ///
    /// Emits: `InitializeEvent`
    pub fn initialize(
        env:         Env,
        admin:       Address,
        token:       Address,
        reward_rate: u32,
        min_stake:   i128,
    ) {
        // Ensure idempotency — initialise only once
        if env.storage().instance().has(&DataKey::Config) {
            panic!("contract already initialised");
        }

        admin.require_auth();

        assert!(reward_rate > 0, "reward_rate must be greater than zero");
        assert!(min_stake   > 0, "min_stake must be greater than zero");

        let config = Config {
            admin:       admin.clone(),
            token,
            reward_rate,
            min_stake,
        };

        env.storage().instance().set(&DataKey::Config, &config);

        emit_initialize(
            &env,
            InitializeEventData {
                admin,
                reward_rate,
                min_stake,
                timestamp: env.ledger().timestamp(),
            },
        );
    }

    // ── Stake ─────────────────────────────────────────────────────────────────

    /// Lock `amount` tokens into the staking contract.
    ///
    /// Transfers tokens from `staker` → contract, then updates on-chain balance.
    ///
    /// Emits: `StakeEvent`
    pub fn stake(env: Env, staker: Address, amount: i128) {
        staker.require_auth();

        let config = Self::get_config(&env);

        assert!(
            amount >= config.min_stake,
            "amount is below the minimum stake"
        );

        // Transfer tokens from staker to this contract
        let token_client = token::Client::new(&env, &config.token);
        token_client.transfer(&staker, &env.current_contract_address(), &amount);

        // Update staker's on-chain balance
        let prev: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Stake(staker.clone()))
            .unwrap_or(0);

        let total = prev + amount;

        env.storage()
            .persistent()
            .set(&DataKey::Stake(staker.clone()), &total);

        // Record the timestamp used to calculate future rewards
        env.storage()
            .persistent()
            .set(&DataKey::StakeTs(staker.clone()), &env.ledger().timestamp());

        emit_stake(
            &env,
            StakeEventData {
                staker,
                amount,
                total,
                timestamp: env.ledger().timestamp(),
            },
        );
    }

    // ── Unstake ───────────────────────────────────────────────────────────────

    /// Unlock `amount` tokens and distribute accrued rewards.
    ///
    /// Transfers (principal + reward) from contract → `staker`.
    ///
    /// Emits: `UnstakeEvent`
    pub fn unstake(env: Env, staker: Address, amount: i128) {
        staker.require_auth();

        let config = Self::get_config(&env);

        assert!(amount > 0, "unstake amount must be greater than zero");

        // Fetch current staked balance
        let current: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Stake(staker.clone()))
            .unwrap_or(0);

        assert!(current >= amount, "insufficient staked balance");

        // Calculate reward based on time elapsed and reward_rate
        let reward = Self::calculate_reward(&env, &staker, amount, &config);

        let remaining = current - amount;
        let payout    = amount + reward;

        // Update on-chain balance before external call (checks-effects-interactions)
        env.storage()
            .persistent()
            .set(&DataKey::Stake(staker.clone()), &remaining);

        // Reset stake timestamp — reward clock restarts for remaining balance
        if remaining > 0 {
            env.storage()
                .persistent()
                .set(&DataKey::StakeTs(staker.clone()), &env.ledger().timestamp());
        } else {
            env.storage()
                .persistent()
                .remove(&DataKey::StakeTs(staker.clone()));
        }

        // Transfer principal + reward back to staker
        let token_client = token::Client::new(&env, &config.token);
        token_client.transfer(&env.current_contract_address(), &staker, &payout);

        emit_unstake(
            &env,
            UnstakeEventData {
                staker,
                amount,
                reward,
                remaining,
                timestamp: env.ledger().timestamp(),
            },
        );
    }

    // ── Views ─────────────────────────────────────────────────────────────────

    /// Return the staked balance for a given address.
    pub fn get_stake(env: Env, staker: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Stake(staker))
            .unwrap_or(0)
    }

    /// Return the current contract configuration.
    pub fn get_config(env: &Env) -> Config {
        env.storage()
            .instance()
            .get(&DataKey::Config)
            .expect("contract not initialised — call initialize() first")
    }

    // ── Private Helpers ───────────────────────────────────────────────────────

    /// Simple time-weighted reward formula:
    ///   reward = amount × (reward_rate / 10_000) × (elapsed_seconds / seconds_per_year)
    ///
    /// Returns 0 if no stake timestamp is recorded.
    fn calculate_reward(
        env:    &Env,
        staker: &Address,
        amount: i128,
        config: &Config,
    ) -> i128 {
        let stake_ts: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::StakeTs(staker.clone()))
            .unwrap_or(env.ledger().timestamp());

        let now     = env.ledger().timestamp();
        let elapsed = now.saturating_sub(stake_ts) as i128;

        const SECONDS_PER_YEAR: i128 = 365 * 24 * 60 * 60;

        // reward_rate is in basis points: divide by 10_000
        (amount * config.reward_rate as i128 * elapsed)
            / (10_000 * SECONDS_PER_YEAR)
    }
}
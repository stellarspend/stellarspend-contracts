//! Rate limit logic for wallet transaction frequency.

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env};

const DEFAULT_LIMIT: u32 = 5; // Default max transactions per window
const DEFAULT_WINDOW_SECONDS: u64 = 3600; // 1 hour window
const DEFAULT_WARN_THRESHOLD: u32 = 4; // Default warning threshold (before hard limit)

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum RateLimitError {
    Exceeded = 1,
    SoftLimitReached = 2,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct RateLimitConfig {
    pub max_tx: u32,
    pub window: u64,
    pub warn_threshold: u32,
    pub allow_overspend: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_tx: DEFAULT_LIMIT,
            window: DEFAULT_WINDOW_SECONDS,
            warn_threshold: DEFAULT_WARN_THRESHOLD,
            allow_overspend: true,
        }
    }
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Config,
    RateLimitCount(Address, u64), // (wallet, window_start)
}

#[contract]
pub struct RateLimitContract;

#[contractimpl]
impl RateLimitContract {
    /// Checks and enforces rate limit for a wallet address.
    /// Emits a warning event when the soft threshold is reached.
    /// Enforces hard limit and returns an error when exceeded.
    pub fn check_and_record(env: Env, wallet: Address) -> Result<(), RateLimitError> {
        let config: RateLimitConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .unwrap_or(RateLimitConfig::default());

        let now = env.ledger().timestamp();
        let window_start = now - (now % config.window);
        let key = DataKey::RateLimitCount(wallet.clone(), window_start);
        let count: u32 = env.storage().persistent().get(&key).unwrap_or(0);

        if count >= config.max_tx {
            env.events()
                .publish(("rate_limit_exceeded", wallet.clone()), count);
            return Err(RateLimitError::Exceeded);
        }

        if count >= config.warn_threshold {
            // Emit warning event; if overspend is allowed we still record the tx.
            env.events()
                .publish(("rate_limit_warning", wallet.clone()), count);
            if !config.allow_overspend {
                return Err(RateLimitError::SoftLimitReached);
            }
        }

        env.storage().persistent().set(&key, &(count + 1));
        Ok(())
    }

    pub fn get_rate_limit_remaining(env: Env, wallet: Address) -> u32 {
        let config: RateLimitConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .unwrap_or(RateLimitConfig::default());

        let now = env.ledger().timestamp();
        let window_start = now - (now % config.window);
        let key = DataKey::RateLimitCount(wallet.clone(), window_start);
        let count: u32 = env.storage().persistent().get(&key).unwrap_or(0);

        if count >= config.max_tx {
            0
        } else {
            config.max_tx - count
        }
    }

    /// Updates rate limit config. Caller must be authorized (require_auth).
    pub fn set_config(
        env: Env,
        caller: Address,
        max_tx: u32,
        window: u64,
        warn_threshold: u32,
        allow_overspend: bool,
    ) {
        caller.require_auth();

        let cfg = RateLimitConfig {
            max_tx,
            window,
            warn_threshold,
            allow_overspend,
        };
        env.storage().persistent().set(&DataKey::Config, &cfg);
        env.events()
            .publish(("rate_limit_config", caller), cfg.max_tx);
    }
}

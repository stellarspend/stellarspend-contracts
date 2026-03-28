use soroban_sdk::{Env};

#[derive(Clone)]
pub struct FeeWindow {
    pub start: u64,   // ledger timestamp start
    pub end: u64,     // ledger timestamp end
    pub fee_rate: u32 // basis points (e.g., 100 = 1%)
}

#[derive(Clone)]
pub struct FeeConfig {
    pub default_fee_rate: u32,
    pub windows: Vec<FeeWindow>,
}

pub fn calculate_fee(env: &Env, amount: i128, config: &FeeConfig) -> i128 {
    let now = env.ledger().timestamp();

    let mut fee_rate = config.default_fee_rate;
    for window in &config.windows {
        if now >= window.start && now <= window.end {
            fee_rate = window.fee_rate;
            break;
        }
    }

    (amount * fee_rate as i128) / 10_000 // basis points calculation
}

pub fn validate_windows(windows: &[FeeWindow]) -> bool {
    for w in windows {
        if w.start >= w.end {
            return false;
        }
    }
    true
}

use soroban_sdk::{Env, contractimpl};
use crate::fee::{FeeConfig, calculate_fee};

pub struct FeeContract;

#[contractimpl]
impl FeeContract {
    pub fn simulate_fee(env: Env, amount: i128, user: soroban_sdk::Address) -> i128 {
        // Read-only: fetch config, calculate fee, return estimate
        let config: FeeConfig = env.storage().persistent().get(&"fee_config").unwrap();
        calculate_fee(&env, amount, &config)
    }

    pub fn get_fee(env: Env, amount: i128) -> i128 {
        let config: FeeConfig = env.storage().persistent().get(&"fee_config").unwrap();
        calculate_fee(&env, amount, &config)
    }
}

use soroban_sdk::Env;

pub fn safe_multiply(amount: i128, rate: u32) -> Option<i128> {
    amount.checked_mul(rate as i128)
}

pub fn safe_divide(value: i128, divisor: i128) -> Option<i128> {
    value.checked_div(divisor)
}

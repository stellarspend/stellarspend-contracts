//! Overspend penalty — calculates a penalty fee when spending exceeds the budget limit.

#![no_std]

use soroban_sdk::{contracttype, symbol_short, Address, Env};

/// Penalty rate in basis points (default 5% = 500 bps).
pub const DEFAULT_PENALTY_BPS: u32 = 500;

#[contracttype]
#[derive(Clone)]
enum PenaltyKey { Rate }

/// Sets the penalty rate in basis points.
pub fn set_penalty_rate(env: &Env, rate_bps: u32) {
    env.storage().instance().set(&PenaltyKey::Rate, &rate_bps);
}

/// Returns the configured penalty rate (basis points).
pub fn get_penalty_rate(env: &Env) -> u32 {
    env.storage().instance().get(&PenaltyKey::Rate).unwrap_or(DEFAULT_PENALTY_BPS)
}

/// Calculates the penalty amount for an overspend.
/// `overspend` is the amount by which spending exceeded the limit.
pub fn calculate_penalty(env: &Env, overspend: i128) -> i128 {
    if overspend <= 0 { return 0; }
    let rate = get_penalty_rate(env) as i128;
    overspend * rate / 10_000
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    #[test]
    fn default_penalty_is_five_percent() {
        let env = Env::default();
        let penalty = calculate_penalty(&env, 1000);
        assert_eq!(penalty, 50); // 5% of 1000
    }

    #[test]
    fn zero_overspend_yields_no_penalty() {
        let env = Env::default();
        assert_eq!(calculate_penalty(&env, 0), 0);
    }
}
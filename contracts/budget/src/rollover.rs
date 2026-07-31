// PLACE THIS FILE AT: contracts/budget/src/rollover.rs
// Resolves issue #82 — Create unused-budget rollover to next period
//
// After adding this file, wire it into contracts/budget/src/lib.rs with:
//   mod rollover;
//   pub use rollover::{set_rollover_rate_bps, calculate_rollover, apply_rollover};
//
// NOTE: this crate currently has multiple candidate entry points
// (lib.rs, lib_main.rs, lib_feature.rs, libs.rs — see issue #2). Wire this
// into whichever one is confirmed canonical; do not wire into all of them.

use soroban_sdk::{contracttype, Address, Env, Symbol};

const ROLLOVER_RATE_KEY: Symbol = Symbol::short("rlovr_bps");

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BudgetPeriod {
    pub allocated: i128,
    pub spent: i128,
    pub rolled_in: i128,
}

/// Admin-only: set the rollover percentage (0-10_000 basis points, i.e.
/// 0%-100%) applied when a budget period closes.
/// TODO: add an admin-check guard (see contracts/admin/) before exposing
/// this publicly.
pub fn set_rollover_rate_bps(env: &Env, owner: &Address, rate_bps: u32) {
    if rate_bps > 10_000 {
        panic!("rollover rate must be between 0 and 10000 basis points");
    }
    env.storage()
        .persistent()
        .set(&(ROLLOVER_RATE_KEY, owner.clone()), &rate_bps);
}

fn get_rollover_rate_bps(env: &Env, owner: &Address) -> u32 {
    env.storage()
        .persistent()
        .get::<_, u32>(&(ROLLOVER_RATE_KEY, owner.clone()))
        .unwrap_or(0)
}

/// Calculates how much of the unused balance from a closing period should
/// carry forward, given the owner's configured rollover rate.
pub fn calculate_rollover(env: &Env, owner: &Address, closing_period: &BudgetPeriod) -> i128 {
    let unused = closing_period.allocated - closing_period.spent;
    if unused <= 0 {
        return 0;
    }

    let rate_bps = get_rollover_rate_bps(env, owner) as i128;
    // Integer division rounds down — funds are never over-credited.
    (unused * rate_bps) / 10_000
}

/// Applies the calculated rollover amount onto a freshly-created next-period
/// BudgetPeriod, returning the updated period.
///
/// TODO: call this from wherever the existing reset_budget logic
/// (contracts/budget — see closed issue #307) currently zeroes out the
/// period, replacing the hard reset with this rollover-aware version.
pub fn apply_rollover(
    env: &Env,
    owner: &Address,
    closing_period: &BudgetPeriod,
    next_allocated: i128,
) -> BudgetPeriod {
    let rolled_in = calculate_rollover(env, owner, closing_period);

    BudgetPeriod {
        allocated: next_allocated,
        spent: 0,
        rolled_in,
    }
}

// ---------------------------------------------------------------------------
// Tests — extend per the issue's acceptance criteria (0%, 50%, 100%).
// Run with: cargo test -p budget -- rollover
// ---------------------------------------------------------------------------
#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    fn period(allocated: i128, spent: i128) -> BudgetPeriod {
        BudgetPeriod {
            allocated,
            spent,
            rolled_in: 0,
        }
    }

    #[test]
    fn zero_percent_rollover_carries_nothing() {
        let env = Env::default();
        let owner = Address::generate(&env);
        set_rollover_rate_bps(&env, &owner, 0);

        let closing = period(1000, 600); // 400 unused
        assert_eq!(calculate_rollover(&env, &owner, &closing), 0);
    }

    #[test]
    fn fifty_percent_rollover_carries_half() {
        let env = Env::default();
        let owner = Address::generate(&env);
        set_rollover_rate_bps(&env, &owner, 5_000);

        let closing = period(1000, 600); // 400 unused
        assert_eq!(calculate_rollover(&env, &owner, &closing), 200);
    }

    #[test]
    fn hundred_percent_rollover_carries_all() {
        let env = Env::default();
        let owner = Address::generate(&env);
        set_rollover_rate_bps(&env, &owner, 10_000);

        let closing = period(1000, 600); // 400 unused
        assert_eq!(calculate_rollover(&env, &owner, &closing), 400);
    }

    #[test]
    fn overspent_period_rolls_over_nothing() {
        let env = Env::default();
        let owner = Address::generate(&env);
        set_rollover_rate_bps(&env, &owner, 10_000);

        let closing = period(1000, 1500); // overspent, no unused funds
        assert_eq!(calculate_rollover(&env, &owner, &closing), 0);
    }

    #[test]
    #[should_panic(expected = "rollover rate must be between 0 and 10000")]
    fn rejects_invalid_rate_above_100_percent() {
        let env = Env::default();
        let owner = Address::generate(&env);
        set_rollover_rate_bps(&env, &owner, 10_001);
    }
}

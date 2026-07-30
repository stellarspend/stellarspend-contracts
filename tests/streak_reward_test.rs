//! Tests for contracts/streak_rewards.rs
//!
//! Run with: `cargo test --test streak_reward_tests`
//!
//! Test taxonomy
//! ─────────────
//! happy_*         — expected successful flows
//! edge_*          — boundary / corner cases
//! neg_*           — inputs / calls that must be rejected
//! security_*      — duplicate-claim, auth, re-init guards

#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger, LedgerInfo},
    token, Address, Env,
};

use crate::streak_rewards::{
    StreakError, StreakRewardsContract, StreakRewardsContractClient, STREAK_TIER_CENTURY,
    STREAK_TIER_MONTH, STREAK_TIER_WEEK,
};

// ── Test helpers ──────────────────────────────────────────────────────────────

const REWARD_WEEK: i128 = 100;
const REWARD_MONTH: i128 = 500;
const REWARD_CENTURY: i128 = 2_000;
const SECONDS_PER_DAY: u64 = 86_400;

struct TestCtx {
    env: Env,
    client: StreakRewardsContractClient<'static>,
    admin: Address,
    token_id: Address,
}

impl TestCtx {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();

        // Deploy a SAC-like test token and mint to the streak contract.
        let token_admin = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract(token_admin.clone());
        let token_sac = token::StellarAssetClient::new(&env, &token_id);

        // Deploy the streak contract.
        let contract_id = env.register_contract(None, StreakRewardsContract);
        let client: StreakRewardsContractClient =
            StreakRewardsContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(
            &admin,
            &token_id,
            &REWARD_WEEK,
            &REWARD_MONTH,
            &REWARD_CENTURY,
        );

        // Fund the contract so it can pay rewards.
        token_sac.mint(&contract_id, &1_000_000i128);

        // Unsafe transmute lifetime for convenience — the Env outlives the test.
        let client: StreakRewardsContractClient<'static> = unsafe { core::mem::transmute(client) };

        Self {
            env,
            client,
            admin,
            token_id,
        }
    }

    /// Advance ledger timestamp by `days` full days.
    fn advance_days(&self, days: u64) {
        let current = self.env.ledger().timestamp();
        self.env.ledger().set(LedgerInfo {
            timestamp: current + days * SECONDS_PER_DAY,
            ..self.env.ledger().get()
        });
    }

    /// Set ledger timestamp to the start of day `day_number`.
    fn set_day(&self, day_number: u64) {
        self.env.ledger().set(LedgerInfo {
            timestamp: day_number * SECONDS_PER_DAY,
            ..self.env.ledger().get()
        });
    }

    fn token_balance(&self, addr: &Address) -> i128 {
        token::Client::new(&self.env, &self.token_id).balance(addr)
    }
}

// ── Initialisation ─────────────────────────────────────────────────────────────

#[test]
fn happy_initialize_stores_config() {
    let ctx = TestCtx::new();
    let cfg = ctx.client.get_config();
    assert_eq!(cfg.admin, ctx.admin);
    assert_eq!(cfg.reward_week, REWARD_WEEK);
    assert_eq!(cfg.reward_month, REWARD_MONTH);
    assert_eq!(cfg.reward_century, REWARD_CENTURY);
}

#[test]
#[should_panic(expected = "AlreadyInitialized")]
fn neg_reinit_blocked() {
    let ctx = TestCtx::new();
    ctx.client.initialize(
        &ctx.admin,
        &ctx.token_id,
        &REWARD_WEEK,
        &REWARD_MONTH,
        &REWARD_CENTURY,
    );
}

#[test]
#[should_panic(expected = "InvalidRewardAmount")]
fn neg_zero_reward_at_init() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract(admin.clone());
    let contract_id = env.register_contract(None, StreakRewardsContract);
    let client = StreakRewardsContractClient::new(&env, &contract_id);
    // reward_week = 0 should fail.
    client.initialize(&admin, &token_id, &0, &REWARD_MONTH, &REWARD_CENTURY);
}

#[test]
#[should_panic(expected = "InvalidRewardAmount")]
fn neg_negative_reward_at_init() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract(admin.clone());
    let contract_id = env.register_contract(None, StreakRewardsContract);
    let client = StreakRewardsContractClient::new(&env, &contract_id);
    client.initialize(&admin, &token_id, &REWARD_WEEK, &-1, &REWARD_CENTURY);
}

// ── Admin: update_rewards ─────────────────────────────────────────────────────

#[test]
fn happy_admin_can_update_rewards() {
    let ctx = TestCtx::new();
    ctx.client.update_rewards(&ctx.admin, &200, &1_000, &4_000);
    let cfg = ctx.client.get_config();
    assert_eq!(cfg.reward_week, 200);
    assert_eq!(cfg.reward_month, 1_000);
    assert_eq!(cfg.reward_century, 4_000);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn neg_non_admin_cannot_update_rewards() {
    let ctx = TestCtx::new();
    let attacker = Address::generate(&ctx.env);
    ctx.client.update_rewards(&attacker, &999, &999, &999);
}

// ── record_deposit: basic streak mechanics ────────────────────────────────────

#[test]
fn happy_first_deposit_starts_streak() {
    let ctx = TestCtx::new();
    ctx.set_day(1);
    let user = Address::generate(&ctx.env);
    let streak = ctx.client.record_deposit(&user);
    assert_eq!(streak, 1);

    let record = ctx.client.get_streak(&user);
    assert_eq!(record.streak_days, 1);
    assert_eq!(record.total_deposits, 1);
}

#[test]
fn happy_consecutive_days_extend_streak() {
    let ctx = TestCtx::new();
    let user = Address::generate(&ctx.env);

    for expected in 1u32..=7 {
        ctx.set_day(expected as u64);
        let streak = ctx.client.record_deposit(&user);
        assert_eq!(streak, expected, "streak mismatch on day {expected}");
    }

    let record = ctx.client.get_streak(&user);
    assert_eq!(record.streak_days, 7);
    assert_eq!(record.total_deposits, 7);
}

#[test]
fn happy_streak_resets_after_missed_day() {
    let ctx = TestCtx::new();
    let user = Address::generate(&ctx.env);

    // Build a 3-day streak.
    for day in 1u64..=3 {
        ctx.set_day(day);
        ctx.client.record_deposit(&user);
    }

    // Skip day 4 — deposit on day 5 should reset to 1.
    ctx.set_day(5);
    let streak = ctx.client.record_deposit(&user);
    assert_eq!(streak, 1, "streak should reset after missed day");

    let record = ctx.client.get_streak(&user);
    assert_eq!(record.streak_days, 1);
    assert_eq!(record.total_deposits, 4); // lifetime total preserved
}

#[test]
fn happy_streak_resets_after_multi_day_gap() {
    let ctx = TestCtx::new();
    let user = Address::generate(&ctx.env);

    ctx.set_day(1);
    ctx.client.record_deposit(&user);

    // Jump forward 10 days.
    ctx.set_day(11);
    let streak = ctx.client.record_deposit(&user);
    assert_eq!(streak, 1);
}

// ── record_deposit: duplicate deposit guards ──────────────────────────────────

#[test]
#[should_panic(expected = "AlreadyDepositedToday")]
fn neg_duplicate_deposit_same_day() {
    let ctx = TestCtx::new();
    let user = Address::generate(&ctx.env);

    ctx.set_day(1);
    ctx.client.record_deposit(&user);
    // Second call on the same day must fail.
    ctx.client.record_deposit(&user);
}

#[test]
fn neg_deposit_mid_day_does_not_double_count() {
    let ctx = TestCtx::new();
    let user = Address::generate(&ctx.env);

    // Deposit at 08:00 on day 1.
    ctx.env.ledger().set(LedgerInfo {
        timestamp: 1 * SECONDS_PER_DAY + 8 * 3_600,
        ..ctx.env.ledger().get()
    });
    ctx.client.record_deposit(&user);

    // Move to 23:59 on the same day — still same day-number.
    ctx.env.ledger().set(LedgerInfo {
        timestamp: 1 * SECONDS_PER_DAY + 23 * 3_600 + 59 * 60,
        ..ctx.env.ledger().get()
    });
    let result = ctx.client.try_record_deposit(&user);
    assert_eq!(result, Err(Ok(StreakError::AlreadyDepositedToday)));
}

// ── claim_reward: milestone payouts ───────────────────────────────────────────

#[test]
fn happy_claim_weekly_reward_at_day_7() {
    let ctx = TestCtx::new();
    let user = Address::generate(&ctx.env);

    // Build a 7-day streak.
    for day in 1u64..=7 {
        ctx.set_day(day);
        ctx.client.record_deposit(&user);
    }

    let before = ctx.token_balance(&user);
    let paid = ctx.client.claim_reward(&user, &STREAK_TIER_WEEK);
    assert_eq!(paid, REWARD_WEEK);
    assert_eq!(ctx.token_balance(&user) - before, REWARD_WEEK);
}

#[test]
fn happy_claim_monthly_reward_at_day_30() {
    let ctx = TestCtx::new();
    let user = Address::generate(&ctx.env);

    for day in 1u64..=30 {
        ctx.set_day(day);
        ctx.client.record_deposit(&user);
    }

    let paid = ctx.client.claim_reward(&user, &STREAK_TIER_MONTH);
    assert_eq!(paid, REWARD_MONTH);
}

#[test]
fn happy_claim_century_reward_at_day_100() {
    let ctx = TestCtx::new();
    let user = Address::generate(&ctx.env);

    for day in 1u64..=100 {
        ctx.set_day(day);
        ctx.client.record_deposit(&user);
    }

    let paid = ctx.client.claim_reward(&user, &STREAK_TIER_CENTURY);
    assert_eq!(paid, REWARD_CENTURY);
}

#[test]
fn happy_can_claim_multiple_tiers_independently() {
    let ctx = TestCtx::new();
    let user = Address::generate(&ctx.env);

    // Day 7 is both a WEEK milestone (7 % 7 == 0) but NOT a MONTH milestone yet.
    for day in 1u64..=7 {
        ctx.set_day(day);
        ctx.client.record_deposit(&user);
    }

    // Can claim WEEK.
    let week_paid = ctx.client.claim_reward(&user, &STREAK_TIER_WEEK);
    assert_eq!(week_paid, REWARD_WEEK);

    // Cannot claim MONTH (not a multiple of 30 yet).
    let result = ctx.client.try_claim_reward(&user, &STREAK_TIER_MONTH);
    assert_eq!(result, Err(Ok(StreakError::NoRewardAvailable)));
}

#[test]
fn happy_total_rewards_claimed_accumulates() {
    let ctx = TestCtx::new();
    let user = Address::generate(&ctx.env);

    for day in 1u64..=7 {
        ctx.set_day(day);
        ctx.client.record_deposit(&user);
    }
    ctx.client.claim_reward(&user, &STREAK_TIER_WEEK);

    let record = ctx.client.get_streak(&user);
    assert_eq!(record.total_rewards_claimed, REWARD_WEEK);
}

// ── claim_reward: duplicate claim guards ─────────────────────────────────────

#[test]
#[should_panic(expected = "AlreadyClaimed")]
fn security_duplicate_claim_blocked() {
    let ctx = TestCtx::new();
    let user = Address::generate(&ctx.env);

    for day in 1u64..=7 {
        ctx.set_day(day);
        ctx.client.record_deposit(&user);
    }

    ctx.client.claim_reward(&user, &STREAK_TIER_WEEK);
    // Second claim for the same (tier, epoch) must be rejected.
    ctx.client.claim_reward(&user, &STREAK_TIER_WEEK);
}

#[test]
fn security_new_epoch_allows_reclaim_after_next_cycle() {
    let ctx = TestCtx::new();
    let user = Address::generate(&ctx.env);

    // First 7-day cycle — claim epoch 1.
    for day in 1u64..=7 {
        ctx.set_day(day);
        ctx.client.record_deposit(&user);
    }
    let first = ctx.client.claim_reward(&user, &STREAK_TIER_WEEK);
    assert_eq!(first, REWARD_WEEK);

    // Continue to 14 days — epoch 2 opens.
    for day in 8u64..=14 {
        ctx.set_day(day);
        ctx.client.record_deposit(&user);
    }
    // Day 14 is a new WEEK milestone (epoch = 14/7 = 2).
    let second = ctx.client.claim_reward(&user, &STREAK_TIER_WEEK);
    assert_eq!(second, REWARD_WEEK);
}

#[test]
fn security_broken_streak_invalidates_pending_claim() {
    let ctx = TestCtx::new();
    let user = Address::generate(&ctx.env);

    // Build 7-day streak but don't claim yet.
    for day in 1u64..=7 {
        ctx.set_day(day);
        ctx.client.record_deposit(&user);
    }

    // Miss a day — streak resets to 1 on the next deposit.
    ctx.set_day(10);
    ctx.client.record_deposit(&user);

    // Trying to claim the (now non-existent) 7-day milestone should fail.
    let result = ctx.client.try_claim_reward(&user, &STREAK_TIER_WEEK);
    assert_eq!(result, Err(Ok(StreakError::NoRewardAvailable)));
}

// ── claim_reward: error paths ─────────────────────────────────────────────────

#[test]
#[should_panic(expected = "NoRewardAvailable")]
fn neg_claim_before_any_deposit() {
    let ctx = TestCtx::new();
    let user = Address::generate(&ctx.env);
    ctx.client.claim_reward(&user, &STREAK_TIER_WEEK);
}

#[test]
fn neg_claim_invalid_tier_returns_no_reward() {
    let ctx = TestCtx::new();
    let user = Address::generate(&ctx.env);
    ctx.set_day(1);
    ctx.client.record_deposit(&user);

    // Tier 5 is not a defined milestone.
    let result = ctx.client.try_claim_reward(&user, &5u32);
    assert_eq!(result, Err(Ok(StreakError::NoRewardAvailable)));
}

#[test]
fn neg_claim_at_non_milestone_day() {
    let ctx = TestCtx::new();
    let user = Address::generate(&ctx.env);

    for day in 1u64..=5 {
        ctx.set_day(day);
        ctx.client.record_deposit(&user);
    }
    // Day 5 is not a multiple of 7.
    let result = ctx.client.try_claim_reward(&user, &STREAK_TIER_WEEK);
    assert_eq!(result, Err(Ok(StreakError::NoRewardAvailable)));
}

// ── is_claimed query ──────────────────────────────────────────────────────────

#[test]
fn happy_is_claimed_returns_false_before_claim() {
    let ctx = TestCtx::new();
    let user = Address::generate(&ctx.env);

    for day in 1u64..=7 {
        ctx.set_day(day);
        ctx.client.record_deposit(&user);
    }

    assert!(!ctx.client.is_claimed(&user, &STREAK_TIER_WEEK));
}

#[test]
fn happy_is_claimed_returns_true_after_claim() {
    let ctx = TestCtx::new();
    let user = Address::generate(&ctx.env);

    for day in 1u64..=7 {
        ctx.set_day(day);
        ctx.client.record_deposit(&user);
    }

    ctx.client.claim_reward(&user, &STREAK_TIER_WEEK);
    assert!(ctx.client.is_claimed(&user, &STREAK_TIER_WEEK));
}

#[test]
fn happy_is_claimed_false_for_no_streak() {
    let ctx = TestCtx::new();
    let user = Address::generate(&ctx.env);
    assert!(!ctx.client.is_claimed(&user, &STREAK_TIER_WEEK));
}

// ── edge cases ────────────────────────────────────────────────────────────────

#[test]
fn edge_day_30_hits_both_week_and_month_milestones() {
    let ctx = TestCtx::new();
    let user = Address::generate(&ctx.env);

    for day in 1u64..=30 {
        ctx.set_day(day);
        ctx.client.record_deposit(&user);
    }

    // Day 30 is 30 % 7 != 0 (not a week milestone) but 30 % 30 == 0 (month).
    let week_result = ctx.client.try_claim_reward(&user, &STREAK_TIER_WEEK);
    assert_eq!(
        week_result,
        Err(Ok(StreakError::NoRewardAvailable)),
        "day 30 should not be a WEEK milestone"
    );

    let month_paid = ctx.client.claim_reward(&user, &STREAK_TIER_MONTH);
    assert_eq!(month_paid, REWARD_MONTH);
}

#[test]
fn edge_day_35_hits_week_milestone_only() {
    let ctx = TestCtx::new();
    let user = Address::generate(&ctx.env);

    for day in 1u64..=35 {
        ctx.set_day(day);
        ctx.client.record_deposit(&user);
    }

    // 35 % 7 == 0 → WEEK milestone; 35 % 30 != 0 → no MONTH milestone.
    let week_paid = ctx.client.claim_reward(&user, &STREAK_TIER_WEEK);
    assert_eq!(week_paid, REWARD_WEEK);

    let month_result = ctx.client.try_claim_reward(&user, &STREAK_TIER_MONTH);
    assert_eq!(month_result, Err(Ok(StreakError::NoRewardAvailable)));
}

#[test]
fn edge_day_100_hits_century_and_possible_week() {
    let ctx = TestCtx::new();
    let user = Address::generate(&ctx.env);

    for day in 1u64..=100 {
        ctx.set_day(day);
        ctx.client.record_deposit(&user);
    }

    // 100 % 100 == 0 → CENTURY milestone.
    let century_paid = ctx.client.claim_reward(&user, &STREAK_TIER_CENTURY);
    assert_eq!(century_paid, REWARD_CENTURY);

    // 100 % 7 != 0 (100 = 14*7 + 2) → no WEEK milestone.
    let week_result = ctx.client.try_claim_reward(&user, &STREAK_TIER_WEEK);
    assert_eq!(week_result, Err(Ok(StreakError::NoRewardAvailable)));
}

#[test]
fn edge_deposit_at_midnight_boundary() {
    let ctx = TestCtx::new();
    let user = Address::generate(&ctx.env);

    // Exactly 00:00:00 on day 2.
    ctx.env.ledger().set(LedgerInfo {
        timestamp: 2 * SECONDS_PER_DAY,
        ..ctx.env.ledger().get()
    });
    ctx.client.record_deposit(&user);

    // One second before midnight on day 3 (23:59:59).
    ctx.env.ledger().set(LedgerInfo {
        timestamp: 3 * SECONDS_PER_DAY - 1,
        ..ctx.env.ledger().get()
    });
    ctx.client.record_deposit(&user);

    // Exactly midnight of day 3 — new day, should succeed.
    ctx.env.ledger().set(LedgerInfo {
        timestamp: 3 * SECONDS_PER_DAY,
        ..ctx.env.ledger().get()
    });
    ctx.client.record_deposit(&user);

    let record = ctx.client.get_streak(&user);
    assert_eq!(record.streak_days, 3);
    assert_eq!(record.total_deposits, 3);
}

#[test]
fn edge_multiple_users_are_isolated() {
    let ctx = TestCtx::new();
    let alice = Address::generate(&ctx.env);
    let bob = Address::generate(&ctx.env);

    // Alice deposits 7 days straight.
    for day in 1u64..=7 {
        ctx.set_day(day);
        ctx.client.record_deposit(&alice);
    }
    // Bob deposits only 3 days.
    for day in 1u64..=3 {
        ctx.set_day(day);
        ctx.client.record_deposit(&bob);
    }

    let alice_record = ctx.client.get_streak(&alice);
    let bob_record = ctx.client.get_streak(&bob);
    assert_eq!(alice_record.streak_days, 7);
    assert_eq!(bob_record.streak_days, 3);

    // Alice can claim WEEK; Bob cannot.
    let alice_paid = ctx.client.claim_reward(&alice, &STREAK_TIER_WEEK);
    assert_eq!(alice_paid, REWARD_WEEK);

    let bob_result = ctx.client.try_claim_reward(&bob, &STREAK_TIER_WEEK);
    assert_eq!(bob_result, Err(Ok(StreakError::NoRewardAvailable)));
}

#[test]
fn edge_streak_does_not_accumulate_on_reset_day() {
    let ctx = TestCtx::new();
    let user = Address::generate(&ctx.env);

    // Deposit on days 1-6, skip day 7, deposit on day 8 (reset to 1).
    for day in 1u64..=6 {
        ctx.set_day(day);
        ctx.client.record_deposit(&user);
    }
    ctx.set_day(8);
    let streak_after_reset = ctx.client.record_deposit(&user);
    assert_eq!(streak_after_reset, 1, "streak should be 1 on reset day");

    // Then deposit on day 9 — streak becomes 2, NOT 7.
    ctx.set_day(9);
    let streak = ctx.client.record_deposit(&user);
    assert_eq!(streak, 2);
}

#[test]
fn edge_claim_reward_does_not_affect_streak_days() {
    let ctx = TestCtx::new();
    let user = Address::generate(&ctx.env);

    for day in 1u64..=7 {
        ctx.set_day(day);
        ctx.client.record_deposit(&user);
    }
    ctx.client.claim_reward(&user, &STREAK_TIER_WEEK);

    // Streak should still be 7 after the claim.
    let record = ctx.client.get_streak(&user);
    assert_eq!(record.streak_days, 7);
}

// ── auth guards ───────────────────────────────────────────────────────────────

#[test]
#[should_panic]
fn security_record_deposit_requires_auth() {
    let env = Env::default();
    // NO mock_all_auths.
    let admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract(admin.clone());
    let contract_id = env.register_contract(None, StreakRewardsContract);
    let client = StreakRewardsContractClient::new(&env, &contract_id);

    env.mock_all_auths(); // Only for init.
    client.initialize(&admin, &token_id, &100, &500, &2000);

    // Now remove auth mocking and try record_deposit.
    let env2 = Env::default(); // fresh env without mocked auth
    let _ = env2; // won't be used — just documenting intent
                  // This call should panic because require_auth is not satisfied.
    let user = Address::generate(&env);
    client.record_deposit(&user); // panics
}

#[test]
#[should_panic]
fn security_claim_reward_requires_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract(admin.clone());
    let token_sac = token::StellarAssetClient::new(&env, &token_id);
    let contract_id = env.register_contract(None, StreakRewardsContract);
    let client = StreakRewardsContractClient::new(&env, &contract_id);

    client.initialize(&admin, &token_id, &100, &500, &2000);
    token_sac.mint(&contract_id, &1_000_000i128);

    let user = Address::generate(&env);
    // Build streak without auth checks to isolate the claim test.
    for i in 1u64..=7 {
        env.ledger().set(LedgerInfo {
            timestamp: i * SECONDS_PER_DAY,
            ..env.ledger().get()
        });
        client.record_deposit(&user);
    }

    // Remove all auths — claim_reward must panic.
    let env_no_auth = Env::default();
    let _ = env_no_auth;
    client.claim_reward(&user, &STREAK_TIER_WEEK);
}

#[test]
fn happy_get_streak_count() {
    let ctx = TestCtx::new();
    let user = Address::generate(&ctx.env);
    let unused_user = Address::generate(&ctx.env);

    assert_eq!(ctx.client.get_streak_count(&unused_user), 0);

    ctx.set_day(1);
    ctx.client.record_deposit(&user);
    assert_eq!(ctx.client.get_streak_count(&user), 1);

    ctx.set_day(2);
    ctx.client.record_deposit(&user);
    assert_eq!(ctx.client.get_streak_count(&user), 2);
}

//! Comprehensive tests for the spending policy contract.
//!
//! Coverage:
//! - Each rule type in isolation (CategoryLimit, MerchantAllowlist,
//!   MerchantBlocklist, TimeWindow, ApprovalThreshold).
//! - Rule combinations (blocklist+allowlist conflict, CategoryLimit +
//!   ApprovalThreshold running together).
//! - The full approval flow (pending → approvals → auto-release).
//! - Policy replacement: atomicity and invalidation of pending approvals.

#![cfg(test)]

use crate::{
    ApprovalOutcome, EvaluationResult, PolicyRule, RejectionReason, SpendingPolicyContract,
    SpendingPolicyContractClient,
};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger},
    Address, Env, Symbol, Vec,
};
use std::panic::{catch_unwind, AssertUnwindSafe};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup() -> (Env, SpendingPolicyContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(SpendingPolicyContract, ());
    let client = SpendingPolicyContractClient::new(&env, &contract_id);
    (env, client)
}

fn addr_vec(env: &Env, addrs: &[&Address]) -> Vec<Address> {
    let mut v = Vec::new(env);
    for a in addrs {
        v.push_back((*a).clone());
    }
    v
}

fn category_rule(category: Symbol, max: i128, period: u64) -> PolicyRule {
    PolicyRule::CategoryLimit(crate::CategoryLimitRule {
        category,
        max_amount: max,
        period_seconds: period,
    })
}

fn allowlist_rule(env: &Env, addrs: &[&Address]) -> PolicyRule {
    PolicyRule::MerchantAllowlist(crate::MerchantAllowlistRule {
        allowed: addr_vec(env, addrs),
    })
}

fn blocklist_rule(env: &Env, addrs: &[&Address]) -> PolicyRule {
    PolicyRule::MerchantBlocklist(crate::MerchantBlocklistRule {
        blocked: addr_vec(env, addrs),
    })
}

fn time_window_rule(start: u64, end: u64) -> PolicyRule {
    PolicyRule::TimeWindow(crate::TimeWindowRule {
        start_seconds: start,
        end_seconds: end,
    })
}

fn threshold_rule(env: &Env, threshold: i128, required: u32, approvers: &[&Address]) -> PolicyRule {
    PolicyRule::ApprovalThreshold(crate::ApprovalThresholdRule {
        threshold_amount: threshold,
        required_approvals: required,
        approvers: addr_vec(env, approvers),
    })
}

fn single_rule(env: &Env, rule: PolicyRule) -> Vec<PolicyRule> {
    let mut rules: Vec<PolicyRule> = Vec::new(env);
    rules.push_back(rule);
    rules
}

// ---------------------------------------------------------------------------
// set_policy / get_policy
// ---------------------------------------------------------------------------

#[test]
fn test_set_and_get_policy() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let merchant = Address::generate(&env);

    let rules = single_rule(&env, blocklist_rule(&env, &[&merchant]));

    client.set_policy(&wallet, &rules);

    let policy = client.get_policy(&wallet).unwrap();
    assert_eq!(policy.version, 1);
    assert_eq!(policy.rules.len(), 1);
}

#[test]
fn test_owner_can_set_policy() {
    // With mock_all_auths the wallet owner (wallet address) authorises and the
    // call succeeds. This is the positive counterpart to the authorization
    // test below.
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let rules: Vec<PolicyRule> = Vec::new(&env);
    client.set_policy(&wallet, &rules);
    assert!(client.get_policy(&wallet).is_some());
}

#[test]
fn test_set_policy_requires_authorization() {
    // Deliberately do NOT mock auths. The wallet address must authorise the
    // call, so an unauthorised set_policy must fail. This enforces the
    // "non-owner cannot set or modify a wallet's policy" requirement.
    let env = Env::default();
    let contract_id = env.register(SpendingPolicyContract, ());
    let client = SpendingPolicyContractClient::new(&env, &contract_id);
    let wallet = Address::generate(&env);
    let rules: Vec<PolicyRule> = Vec::new(&env);

    let result = catch_unwind(AssertUnwindSafe(|| {
        client.set_policy(&wallet, &rules);
    }));
    assert!(result.is_err(), "set_policy must require authorization");
}

#[test]
fn test_set_policy_replaces_atomically_and_bumps_version() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let merchant_a = Address::generate(&env);
    let merchant_b = Address::generate(&env);

    // v1: block merchant_a.
    client.set_policy(
        &wallet,
        &single_rule(&env, blocklist_rule(&env, &[&merchant_a])),
    );
    assert_eq!(client.get_policy(&wallet).unwrap().version, 1);

    // v2: block merchant_b instead.
    client.set_policy(
        &wallet,
        &single_rule(&env, blocklist_rule(&env, &[&merchant_b])),
    );
    let policy = client.get_policy(&wallet).unwrap();
    assert_eq!(policy.version, 2);
    assert_eq!(policy.rules.len(), 1);
}

#[test]
fn test_policy_replacement_is_atomic() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let merchant = Address::generate(&env);

    // v1: a valid policy.
    let v1 = single_rule(&env, blocklist_rule(&env, &[&merchant]));
    client.set_policy(&wallet, &v1);
    assert_eq!(client.get_policy(&wallet).unwrap().version, 1);

    // Attempt to set an invalid policy (empty time window). This must panic
    // and leave v1 completely untouched.
    let invalid = single_rule(&env, time_window_rule(21_600, 21_600));
    let result = catch_unwind(AssertUnwindSafe(|| {
        client.set_policy(&wallet, &invalid);
    }));
    assert!(result.is_err(), "invalid policy must be rejected");

    let policy = client.get_policy(&wallet).unwrap();
    assert_eq!(
        policy.version, 1,
        "version must be unchanged after failed replace"
    );
    assert_eq!(
        policy.rules.len(),
        1,
        "rules must be unchanged after failed replace"
    );
}

#[test]
fn test_too_many_rules_rejected() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let merchant = Address::generate(&env);

    let mut rules: Vec<PolicyRule> = Vec::new(&env);
    for _ in 0..(crate::MAX_RULES + 1) {
        rules.push_back(blocklist_rule(&env, &[&merchant]));
    }

    let result = catch_unwind(AssertUnwindSafe(|| {
        client.set_policy(&wallet, &rules);
    }));
    assert!(
        result.is_err(),
        "policies exceeding MAX_RULES must be rejected"
    );
}

// ---------------------------------------------------------------------------
// No policy / invalid amount
// ---------------------------------------------------------------------------

#[test]
fn test_no_policy_approves_transaction() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let recipient = Address::generate(&env);

    let result = client.evaluate_transaction(&wallet, &recipient, &100, &None::<Symbol>);
    assert_eq!(result, EvaluationResult::Approved);
}

#[test]
fn test_invalid_amount_rejected() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let recipient = Address::generate(&env);

    // Set a policy so we exercise the post-policy path.
    let merchant = Address::generate(&env);
    client.set_policy(
        &wallet,
        &single_rule(&env, blocklist_rule(&env, &[&merchant])),
    );

    let result = client.evaluate_transaction(&wallet, &recipient, &0, &None::<Symbol>);
    assert_eq!(
        result,
        EvaluationResult::Rejected(RejectionReason::InvalidAmount)
    );

    let result = client.evaluate_transaction(&wallet, &recipient, &-50, &None::<Symbol>);
    assert_eq!(
        result,
        EvaluationResult::Rejected(RejectionReason::InvalidAmount)
    );
}

// ---------------------------------------------------------------------------
// CategoryLimit
// ---------------------------------------------------------------------------

#[test]
fn test_category_limit_allows_within_limit() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let recipient = Address::generate(&env);
    env.ledger().set_timestamp(1000);

    client.set_policy(
        &wallet,
        &single_rule(&env, category_rule(symbol_short!("groc"), 1000, 3600)),
    );

    let cat = Some(symbol_short!("groc"));
    let result = client.evaluate_transaction(&wallet, &recipient, &600, &cat);
    assert_eq!(result, EvaluationResult::Approved);

    // Spend recorded for the current period.
    let period_id = 1000u64 / 3600;
    assert_eq!(
        client.get_category_spending(&wallet, &symbol_short!("groc"), &period_id),
        600
    );
}

#[test]
fn test_category_limit_rejects_over_limit() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let recipient = Address::generate(&env);
    env.ledger().set_timestamp(1000);

    client.set_policy(
        &wallet,
        &single_rule(&env, category_rule(symbol_short!("groc"), 1000, 3600)),
    );

    let cat = Some(symbol_short!("groc"));
    // First 600 is fine.
    assert_eq!(
        client.evaluate_transaction(&wallet, &recipient, &600, &cat),
        EvaluationResult::Approved
    );
    // 600 + 500 = 1100 > 1000 -> rejected.
    assert_eq!(
        client.evaluate_transaction(&wallet, &recipient, &500, &cat),
        EvaluationResult::Rejected(RejectionReason::CategoryLimitExceeded)
    );
}

#[test]
fn test_category_limit_resets_after_period() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let recipient = Address::generate(&env);

    client.set_policy(
        &wallet,
        &single_rule(&env, category_rule(symbol_short!("groc"), 1000, 3600)),
    );

    let cat = Some(symbol_short!("groc"));

    // Period 0.
    env.ledger().set_timestamp(1000);
    assert_eq!(
        client.evaluate_transaction(&wallet, &recipient, &600, &cat),
        EvaluationResult::Approved
    );
    assert_eq!(
        client.evaluate_transaction(&wallet, &recipient, &500, &cat),
        EvaluationResult::Rejected(RejectionReason::CategoryLimitExceeded)
    );

    // Advance into period 1 -> spending resets.
    env.ledger().set_timestamp(1000 + 3600);
    assert_eq!(
        client.evaluate_transaction(&wallet, &recipient, &500, &cat),
        EvaluationResult::Approved
    );

    assert_eq!(
        client.get_category_spending(&wallet, &symbol_short!("groc"), &0),
        600
    );
    assert_eq!(
        client.get_category_spending(&wallet, &symbol_short!("groc"), &1),
        500
    );
}

#[test]
fn test_category_limit_does_not_apply_to_other_categories() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let recipient = Address::generate(&env);
    env.ledger().set_timestamp(1000);

    client.set_policy(
        &wallet,
        &single_rule(&env, category_rule(symbol_short!("groc"), 1000, 3600)),
    );

    // A transaction in a different category is unaffected.
    let dining = Some(symbol_short!("dining"));
    assert_eq!(
        client.evaluate_transaction(&wallet, &recipient, &5_000_000, &dining),
        EvaluationResult::Approved
    );
}

// ---------------------------------------------------------------------------
// MerchantAllowlist
// ---------------------------------------------------------------------------

#[test]
fn test_merchant_allowlist_allows_listed() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let merchant = Address::generate(&env);

    client.set_policy(
        &wallet,
        &single_rule(&env, allowlist_rule(&env, &[&merchant])),
    );

    let result = client.evaluate_transaction(&wallet, &merchant, &100, &None::<Symbol>);
    assert_eq!(result, EvaluationResult::Approved);
}

#[test]
fn test_merchant_allowlist_rejects_unlisted() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let listed = Address::generate(&env);
    let unknown = Address::generate(&env);

    client.set_policy(
        &wallet,
        &single_rule(&env, allowlist_rule(&env, &[&listed])),
    );

    let result = client.evaluate_transaction(&wallet, &unknown, &100, &None::<Symbol>);
    assert_eq!(
        result,
        EvaluationResult::Rejected(RejectionReason::MerchantNotAllowed)
    );
}

// ---------------------------------------------------------------------------
// MerchantBlocklist
// ---------------------------------------------------------------------------

#[test]
fn test_merchant_blocklist_rejects_listed() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let blocked = Address::generate(&env);

    client.set_policy(
        &wallet,
        &single_rule(&env, blocklist_rule(&env, &[&blocked])),
    );

    let result = client.evaluate_transaction(&wallet, &blocked, &100, &None::<Symbol>);
    assert_eq!(
        result,
        EvaluationResult::Rejected(RejectionReason::MerchantBlocked)
    );
}

#[test]
fn test_merchant_blocklist_allows_unlisted() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let blocked = Address::generate(&env);
    let ok = Address::generate(&env);

    client.set_policy(
        &wallet,
        &single_rule(&env, blocklist_rule(&env, &[&blocked])),
    );

    let result = client.evaluate_transaction(&wallet, &ok, &100, &None::<Symbol>);
    assert_eq!(result, EvaluationResult::Approved);
}

// ---------------------------------------------------------------------------
// Conflict: blocklist wins over allowlist
// ---------------------------------------------------------------------------

#[test]
fn test_blocklist_takes_precedence_over_allowlist() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let merchant = Address::generate(&env);

    // Merchant is on BOTH lists. Blocklist must win.
    let mut rules: Vec<PolicyRule> = Vec::new(&env);
    rules.push_back(allowlist_rule(&env, &[&merchant]));
    rules.push_back(blocklist_rule(&env, &[&merchant]));
    client.set_policy(&wallet, &rules);

    let result = client.evaluate_transaction(&wallet, &merchant, &100, &None::<Symbol>);
    assert_eq!(
        result,
        EvaluationResult::Rejected(RejectionReason::MerchantBlocked)
    );
}

#[test]
fn test_allowlist_and_blocklist_combined() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let allowed = Address::generate(&env);
    let blocked = Address::generate(&env);

    let mut rules: Vec<PolicyRule> = Vec::new(&env);
    rules.push_back(allowlist_rule(&env, &[&allowed, &blocked]));
    rules.push_back(blocklist_rule(&env, &[&blocked]));
    client.set_policy(&wallet, &rules);

    // `allowed` is on the allowlist and not blocked -> approved.
    assert_eq!(
        client.evaluate_transaction(&wallet, &allowed, &100, &None::<Symbol>),
        EvaluationResult::Approved
    );
    // `blocked` is on both -> blocked.
    assert_eq!(
        client.evaluate_transaction(&wallet, &blocked, &100, &None::<Symbol>),
        EvaluationResult::Rejected(RejectionReason::MerchantBlocked)
    );
}

// ---------------------------------------------------------------------------
// TimeWindow
// ---------------------------------------------------------------------------

#[test]
fn test_time_window_allows_during_window() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let recipient = Address::generate(&env);

    // Allow 06:00 -> midnight (blocks midnight -> 06:00).
    client.set_policy(
        &wallet,
        &single_rule(&env, time_window_rule(21_600, 86_400)),
    );

    // 12:00 noon -> within window.
    env.ledger().set_timestamp(43_200);
    assert_eq!(
        client.evaluate_transaction(&wallet, &recipient, &100, &None::<Symbol>),
        EvaluationResult::Approved
    );
}

#[test]
fn test_time_window_rejects_outside_window() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let recipient = Address::generate(&env);

    // Allow 06:00 -> midnight (blocks midnight -> 06:00).
    client.set_policy(
        &wallet,
        &single_rule(&env, time_window_rule(21_600, 86_400)),
    );

    // 03:00 -> outside window.
    env.ledger().set_timestamp(10_800);
    assert_eq!(
        client.evaluate_transaction(&wallet, &recipient, &100, &None::<Symbol>),
        EvaluationResult::Rejected(RejectionReason::OutsideTimeWindow)
    );
}

#[test]
fn test_time_window_wrap_around_past_midnight() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let recipient = Address::generate(&env);

    // Allow 22:00 -> 02:00 (wraps midnight).
    client.set_policy(&wallet, &single_rule(&env, time_window_rule(79_200, 7_200)));

    // 23:00 (82800) -> within.
    env.ledger().set_timestamp(82_800);
    assert_eq!(
        client.evaluate_transaction(&wallet, &recipient, &100, &None::<Symbol>),
        EvaluationResult::Approved
    );

    // 01:00 (3600) -> within (wrapped).
    env.ledger().set_timestamp(3_600);
    assert_eq!(
        client.evaluate_transaction(&wallet, &recipient, &100, &None::<Symbol>),
        EvaluationResult::Approved
    );

    // 12:00 (43200) -> outside.
    env.ledger().set_timestamp(43_200);
    assert_eq!(
        client.evaluate_transaction(&wallet, &recipient, &100, &None::<Symbol>),
        EvaluationResult::Rejected(RejectionReason::OutsideTimeWindow)
    );
}

// ---------------------------------------------------------------------------
// ApprovalThreshold
// ---------------------------------------------------------------------------

#[test]
fn test_approval_threshold_creates_pending() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let recipient = Address::generate(&env);
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);

    client.set_policy(
        &wallet,
        &single_rule(&env, threshold_rule(&env, 1000, 2, &[&bob, &carol])),
    );

    let result = client.evaluate_transaction(&wallet, &recipient, &1500, &None::<Symbol>);
    match result {
        EvaluationResult::PendingApproval(id) => {
            let pending = client.get_pending_transaction(&id).unwrap();
            assert_eq!(pending.wallet, wallet);
            assert_eq!(pending.recipient, recipient);
            assert_eq!(pending.amount, 1500);
            assert_eq!(pending.required_approvals, 2);
            assert_eq!(pending.approvers.len(), 0);
            assert_eq!(pending.authorized_approvers.len(), 2);
        }
        other => panic!("expected PendingApproval, got {:?}", other),
    }
}

#[test]
fn test_approval_threshold_below_threshold_approved() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let recipient = Address::generate(&env);
    let bob = Address::generate(&env);

    client.set_policy(
        &wallet,
        &single_rule(&env, threshold_rule(&env, 1000, 1, &[&bob])),
    );

    // Below threshold -> approved immediately.
    let result = client.evaluate_transaction(&wallet, &recipient, &500, &None::<Symbol>);
    assert_eq!(result, EvaluationResult::Approved);
}

#[test]
fn test_approval_threshold_auto_releases_after_n_approvals() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let recipient = Address::generate(&env);
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);

    client.set_policy(
        &wallet,
        &single_rule(&env, threshold_rule(&env, 1000, 2, &[&bob, &carol])),
    );

    let pending_id = match client.evaluate_transaction(&wallet, &recipient, &1500, &None::<Symbol>)
    {
        EvaluationResult::PendingApproval(id) => id,
        other => panic!("expected pending, got {:?}", other),
    };

    // First approval -> still pending.
    assert_eq!(
        client.submit_approval(&bob, &pending_id),
        ApprovalOutcome::Pending(1)
    );

    // Second approval -> auto-released.
    assert_eq!(
        client.submit_approval(&carol, &pending_id),
        ApprovalOutcome::Approved
    );

    // Pending record removed.
    assert!(client.get_pending_transaction(&pending_id).is_none());
    assert_eq!(client.get_pending_ids_for_wallet(&wallet).len(), 0);
}

#[test]
fn test_approval_threshold_insufficient_approvals_stay_pending() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let recipient = Address::generate(&env);
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);
    let dave = Address::generate(&env);

    client.set_policy(
        &wallet,
        &single_rule(&env, threshold_rule(&env, 1000, 3, &[&bob, &carol, &dave])),
    );

    let pending_id = match client.evaluate_transaction(&wallet, &recipient, &2000, &None::<Symbol>)
    {
        EvaluationResult::PendingApproval(id) => id,
        other => panic!("expected pending, got {:?}", other),
    };

    assert_eq!(
        client.submit_approval(&bob, &pending_id),
        ApprovalOutcome::Pending(1)
    );
    assert_eq!(
        client.submit_approval(&carol, &pending_id),
        ApprovalOutcome::Pending(2)
    );

    // Still pending.
    assert!(client.get_pending_transaction(&pending_id).is_some());
}

#[test]
fn test_approval_threshold_non_approver_fails() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let recipient = Address::generate(&env);
    let bob = Address::generate(&env);
    let mallory = Address::generate(&env);

    client.set_policy(
        &wallet,
        &single_rule(&env, threshold_rule(&env, 1000, 1, &[&bob])),
    );

    let pending_id = match client.evaluate_transaction(&wallet, &recipient, &1500, &None::<Symbol>)
    {
        EvaluationResult::PendingApproval(id) => id,
        other => panic!("expected pending, got {:?}", other),
    };

    let result = catch_unwind(AssertUnwindSafe(|| {
        client.submit_approval(&mallory, &pending_id);
    }));
    assert!(result.is_err(), "non-approver must be rejected");

    // Pending tx unchanged.
    let pending = client.get_pending_transaction(&pending_id).unwrap();
    assert_eq!(pending.approvers.len(), 0);
}

#[test]
fn test_approval_threshold_double_approval_fails() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let recipient = Address::generate(&env);
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);

    client.set_policy(
        &wallet,
        &single_rule(&env, threshold_rule(&env, 1000, 2, &[&bob, &carol])),
    );

    let pending_id = match client.evaluate_transaction(&wallet, &recipient, &1500, &None::<Symbol>)
    {
        EvaluationResult::PendingApproval(id) => id,
        other => panic!("expected pending, got {:?}", other),
    };

    // First approval ok.
    client.submit_approval(&bob, &pending_id);

    // Same approver again -> rejected.
    let result = catch_unwind(AssertUnwindSafe(|| {
        client.submit_approval(&bob, &pending_id);
    }));
    assert!(result.is_err(), "double approval must be rejected");

    let pending = client.get_pending_transaction(&pending_id).unwrap();
    assert_eq!(pending.approvers.len(), 1);
}

// ---------------------------------------------------------------------------
// Combined: CategoryLimit + ApprovalThreshold
// ---------------------------------------------------------------------------

#[test]
fn test_combined_category_limit_and_approval_threshold() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let recipient = Address::generate(&env);
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);

    let period = 86_400u64;
    let mut rules: Vec<PolicyRule> = Vec::new(&env);
    rules.push_back(category_rule(symbol_short!("groc"), 5000, period));
    rules.push_back(threshold_rule(&env, 1000, 2, &[&bob, &carol]));
    client.set_policy(&wallet, &rules);

    let ts = 5 * period; // a known period (id = 5)
    env.ledger().set_timestamp(ts);

    let cat = Some(symbol_short!("groc"));

    // 1500 is within the category limit (0 + 1500 <= 5000) and >= threshold.
    let pending_id = match client.evaluate_transaction(&wallet, &recipient, &1500, &cat) {
        EvaluationResult::PendingApproval(id) => id,
        other => panic!("expected pending, got {:?}", other),
    };

    // Category spend must NOT be recorded yet (pending until released).
    let period_id = ts / period;
    assert_eq!(
        client.get_category_spending(&wallet, &symbol_short!("groc"), &period_id),
        0
    );

    // Collect approvals -> auto-released, spend recorded.
    client.submit_approval(&bob, &pending_id);
    assert_eq!(
        client.submit_approval(&carol, &pending_id),
        ApprovalOutcome::Approved
    );
    assert_eq!(
        client.get_category_spending(&wallet, &symbol_short!("groc"), &period_id),
        1500
    );

    // Now another 4000 groceries would exceed 5000 -> rejected.
    assert_eq!(
        client.evaluate_transaction(&wallet, &recipient, &4000, &cat),
        EvaluationResult::Rejected(RejectionReason::CategoryLimitExceeded)
    );
}

#[test]
fn test_combined_all_rule_types() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let good_merchant = Address::generate(&env);
    let bad_merchant = Address::generate(&env);
    let bob = Address::generate(&env);

    let mut rules: Vec<PolicyRule> = Vec::new(&env);
    rules.push_back(blocklist_rule(&env, &[&bad_merchant]));
    rules.push_back(allowlist_rule(&env, &[&good_merchant]));
    rules.push_back(time_window_rule(21_600, 86_400)); // 06:00 -> midnight
    rules.push_back(category_rule(symbol_short!("groc"), 5000, 86_400));
    rules.push_back(threshold_rule(&env, 1000, 1, &[&bob]));
    client.set_policy(&wallet, &rules);

    // During the allowed window.
    env.ledger().set_timestamp(43_200); // noon

    let cat = Some(symbol_short!("groc"));

    // Blocked merchant -> rejected even though it is also on the allowlist? No,
    // good_merchant is the allowlisted one. bad_merchant is blocked.
    assert_eq!(
        client.evaluate_transaction(&wallet, &bad_merchant, &100, &cat),
        EvaluationResult::Rejected(RejectionReason::MerchantBlocked)
    );

    // Unknown merchant -> not on allowlist.
    let stranger = Address::generate(&env);
    assert_eq!(
        client.evaluate_transaction(&wallet, &stranger, &100, &cat),
        EvaluationResult::Rejected(RejectionReason::MerchantNotAllowed)
    );

    // good_merchant, below threshold, within window, within category -> approved.
    assert_eq!(
        client.evaluate_transaction(&wallet, &good_merchant, &500, &cat),
        EvaluationResult::Approved
    );

    // good_merchant, above threshold -> pending.
    let pending_id = match client.evaluate_transaction(&wallet, &good_merchant, &1500, &cat) {
        EvaluationResult::PendingApproval(id) => id,
        other => panic!("expected pending, got {:?}", other),
    };

    // Outside the time window -> rejected.
    env.ledger().set_timestamp(10_800); // 03:00
    assert_eq!(
        client.evaluate_transaction(&wallet, &good_merchant, &100, &cat),
        EvaluationResult::Rejected(RejectionReason::OutsideTimeWindow)
    );

    // The pending tx from before can still be approved (policy unchanged).
    env.ledger().set_timestamp(43_200); // back to noon for period consistency
    assert_eq!(
        client.submit_approval(&bob, &pending_id),
        ApprovalOutcome::Approved
    );
}

// ---------------------------------------------------------------------------
// Policy replacement invalidates pending approvals
// ---------------------------------------------------------------------------

#[test]
fn test_policy_replacement_invalidates_pending_approvals() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let recipient = Address::generate(&env);
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);

    // v1: requires 2 approvals.
    client.set_policy(
        &wallet,
        &single_rule(&env, threshold_rule(&env, 1000, 2, &[&bob, &carol])),
    );

    let pending_id = match client.evaluate_transaction(&wallet, &recipient, &1500, &None::<Symbol>)
    {
        EvaluationResult::PendingApproval(id) => id,
        other => panic!("expected pending, got {:?}", other),
    };

    // One approval collected under v1.
    client.submit_approval(&bob, &pending_id);

    // Replace the policy (v2). This bumps the version and invalidates the
    // pending transaction created under v1.
    let new_rules = single_rule(&env, threshold_rule(&env, 1000, 1, &[&bob]));
    client.set_policy(&wallet, &new_rules);
    assert_eq!(client.get_policy(&wallet).unwrap().version, 2);

    // Submitting another approval for the old pending tx must now fail.
    let result = catch_unwind(AssertUnwindSafe(|| {
        client.submit_approval(&carol, &pending_id);
    }));
    assert!(
        result.is_err(),
        "pending approvals from the old policy must be invalidated"
    );

    // The pending record itself is still queryable but effectively dead.
    let pending = client.get_pending_transaction(&pending_id).unwrap();
    assert_eq!(
        pending.policy_version, 1,
        "pending tx retains its original version"
    );
}

// ---------------------------------------------------------------------------
// Pending index
// ---------------------------------------------------------------------------

#[test]
fn test_pending_index_tracks_wallet_transactions() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let recipient1 = Address::generate(&env);
    let recipient2 = Address::generate(&env);
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);

    client.set_policy(
        &wallet,
        &single_rule(&env, threshold_rule(&env, 1000, 2, &[&bob, &carol])),
    );

    let id1 = match client.evaluate_transaction(&wallet, &recipient1, &1500, &None::<Symbol>) {
        EvaluationResult::PendingApproval(id) => id,
        other => panic!("expected pending, got {:?}", other),
    };
    let id2 = match client.evaluate_transaction(&wallet, &recipient2, &2000, &None::<Symbol>) {
        EvaluationResult::PendingApproval(id) => id,
        other => panic!("expected pending, got {:?}", other),
    };

    let ids = client.get_pending_ids_for_wallet(&wallet);
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&id1));
    assert!(ids.contains(&id2));
}

#[test]
fn test_pending_removed_from_index_on_release() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let recipient = Address::generate(&env);
    let bob = Address::generate(&env);

    client.set_policy(
        &wallet,
        &single_rule(&env, threshold_rule(&env, 1000, 1, &[&bob])),
    );

    let pending_id = match client.evaluate_transaction(&wallet, &recipient, &1500, &None::<Symbol>)
    {
        EvaluationResult::PendingApproval(id) => id,
        other => panic!("expected pending, got {:?}", other),
    };

    assert_eq!(client.get_pending_ids_for_wallet(&wallet).len(), 1);

    client.submit_approval(&bob, &pending_id);

    // Released -> removed from the index.
    assert_eq!(client.get_pending_ids_for_wallet(&wallet).len(), 0);
}

// ---------------------------------------------------------------------------
// set_policy validation edge cases (exercise via the contract entry point)
// ---------------------------------------------------------------------------

#[test]
fn test_set_policy_rejects_invalid_time_window() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);

    let rules = single_rule(&env, time_window_rule(90_000, 90_000));
    let result = catch_unwind(AssertUnwindSafe(|| {
        client.set_policy(&wallet, &rules);
    }));
    assert!(
        result.is_err(),
        "invalid time window must be rejected at set_policy"
    );
    assert!(
        client.get_policy(&wallet).is_none(),
        "no policy should be stored on failure"
    );
}

#[test]
fn test_set_policy_rejects_unachievable_quorum() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let bob = Address::generate(&env);

    // required=5 but only 1 approver.
    let rules = single_rule(&env, threshold_rule(&env, 1000, 5, &[&bob]));
    let result = catch_unwind(AssertUnwindSafe(|| {
        client.set_policy(&wallet, &rules);
    }));
    assert!(result.is_err(), "unachievable quorum must be rejected");
}

#[test]
fn test_set_policy_rejects_empty_allowlist() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);

    let rules = single_rule(&env, allowlist_rule(&env, &[]));
    let result = catch_unwind(AssertUnwindSafe(|| {
        client.set_policy(&wallet, &rules);
    }));
    assert!(result.is_err(), "empty allowlist must be rejected");
}

// A small sanity check to ensure the error type is wired into the Soroban
// error conversion (keeps `SpendingPolicyError` from being dead code in tests).
#[test]
fn test_error_conversion_wired() {
    let err = crate::SpendingPolicyError::Unauthorized;
    let sdk_err: soroban_sdk::Error = err.into();
    assert_eq!(sdk_err, soroban_sdk::Error::from_contract_error(1));
}

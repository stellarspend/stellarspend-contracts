#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

use crate::{
    errors::MultisigError,
    logic::{
        approve_proposal, configure_goal, create_proposal, execute_proposal, get_proposal,
        has_approved, is_configured, is_proposal_executed, requires_multisig, update_signers,
    },
    types::ProposalStatus,
};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Build an Env and three distinct signers.
fn setup() -> (Env, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);
    (env, a, b, c)
}

fn three_signers(env: &Env, a: &Address, b: &Address, c: &Address) -> Vec<Address> {
    let mut v = Vec::new(env);
    v.push_back(a.clone());
    v.push_back(b.clone());
    v.push_back(c.clone());
    v
}

const TTL: u64 = 3600; // 1 hour
const GOAL: u64 = 42;
const AMOUNT: i128 = 1_000_000;

// ---------------------------------------------------------------------------
// Configuration tests
// ---------------------------------------------------------------------------

#[test]
fn test_configure_and_query() {
    let (env, a, b, c) = setup();
    let signers = three_signers(&env, &a, &b, &c);
    configure_goal(&env, GOAL, signers, 2, TTL);

    assert!(is_configured(&env, GOAL));
    assert!(requires_multisig(&env, GOAL));
}

#[test]
fn test_unconfigured_goal_does_not_require_multisig() {
    let env = Env::default();
    env.mock_all_auths();
    assert!(!requires_multisig(&env, 999));
}

#[test]
#[should_panic]
fn test_invalid_threshold_zero() {
    let (env, a, b, c) = setup();
    let signers = three_signers(&env, &a, &b, &c);
    configure_goal(&env, GOAL, signers, 0, TTL); // threshold=0 is invalid
}

#[test]
#[should_panic]
fn test_invalid_threshold_exceeds_signers() {
    let (env, a, b, c) = setup();
    let signers = three_signers(&env, &a, &b, &c);
    configure_goal(&env, GOAL, signers, 4, TTL); // 4 > 3
}

#[test]
#[should_panic]
fn test_duplicate_signer_rejected() {
    let (env, a, _b, _c) = setup();
    let mut signers = Vec::new(&env);
    signers.push_back(a.clone());
    signers.push_back(a.clone()); // duplicate
    configure_goal(&env, GOAL, signers, 1, TTL);
}

#[test]
fn test_update_signers() {
    let (env, a, b, c) = setup();
    let signers = three_signers(&env, &a, &b, &c);
    configure_goal(&env, GOAL, signers, 2, TTL);

    // Replace with just a and b, threshold 2-of-2
    let mut new_signers = Vec::new(&env);
    new_signers.push_back(a.clone());
    new_signers.push_back(b.clone());
    update_signers(&env, GOAL, new_signers, 2);

    let cfg = crate::logic::get_config(&env, GOAL);
    assert_eq!(cfg.signers.len(), 2);
    assert_eq!(cfg.threshold, 2);
}

// ---------------------------------------------------------------------------
// Proposal creation tests
// ---------------------------------------------------------------------------

#[test]
fn test_create_proposal_returns_id_one() {
    let (env, a, b, c) = setup();
    let signers = three_signers(&env, &a, &b, &c);
    configure_goal(&env, GOAL, signers, 2, TTL);

    let id = create_proposal(&env, GOAL, a.clone(), AMOUNT);
    assert_eq!(id, 1);
}

#[test]
fn test_proposal_ids_increment() {
    let (env, a, b, c) = setup();
    let signers = three_signers(&env, &a, &b, &c);
    configure_goal(&env, GOAL, signers, 2, TTL);

    let id1 = create_proposal(&env, GOAL, a.clone(), AMOUNT);
    let id2 = create_proposal(&env, GOAL, b.clone(), AMOUNT);
    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
}

#[test]
#[should_panic]
fn test_non_signer_cannot_create_proposal() {
    let (env, a, b, c) = setup();
    let signers = three_signers(&env, &a, &b, &c);
    configure_goal(&env, GOAL, signers, 2, TTL);

    let outsider = Address::generate(&env);
    create_proposal(&env, GOAL, outsider, AMOUNT);
}

#[test]
#[should_panic]
fn test_zero_amount_rejected() {
    let (env, a, b, c) = setup();
    let signers = three_signers(&env, &a, &b, &c);
    configure_goal(&env, GOAL, signers, 2, TTL);
    create_proposal(&env, GOAL, a.clone(), 0);
}

#[test]
fn test_proposal_initial_status_is_pending() {
    let (env, a, b, c) = setup();
    let signers = three_signers(&env, &a, &b, &c);
    configure_goal(&env, GOAL, signers, 2, TTL);

    let id = create_proposal(&env, GOAL, a.clone(), AMOUNT);
    let p = get_proposal(&env, GOAL, id);
    assert_eq!(p.status, ProposalStatus::Pending);
    assert_eq!(p.approval_count, 0);
}

// ---------------------------------------------------------------------------
// Approval tests  (core acceptance criteria)
// ---------------------------------------------------------------------------

/// AC: A withdrawal proposal cannot execute until enough distinct signers approve.
#[test]
fn test_single_approval_does_not_execute_2_of_3() {
    let (env, a, b, c) = setup();
    let signers = three_signers(&env, &a, &b, &c);
    configure_goal(&env, GOAL, signers, 2, TTL);

    let id = create_proposal(&env, GOAL, a.clone(), AMOUNT);

    // Only A approves
    let count = approve_proposal(&env, GOAL, id, a.clone());
    assert_eq!(count, 1);

    let p = get_proposal(&env, GOAL, id);
    // Still Pending – quorum not reached
    assert_eq!(p.status, ProposalStatus::Pending);
    assert!(!is_proposal_executed(&env, GOAL, id));
}

/// AC: The same signer approving twice does not count twice.
#[test]
#[should_panic]
fn test_duplicate_approval_rejected() {
    let (env, a, b, c) = setup();
    let signers = three_signers(&env, &a, &b, &c);
    configure_goal(&env, GOAL, signers, 2, TTL);

    let id = create_proposal(&env, GOAL, a.clone(), AMOUNT);
    approve_proposal(&env, GOAL, id, a.clone());
    approve_proposal(&env, GOAL, id, a.clone()); // should panic DuplicateApproval
}

/// AC: Once quorum is met the proposal executes automatically.
#[test]
fn test_second_approval_auto_executes() {
    let (env, a, b, c) = setup();
    let signers = three_signers(&env, &a, &b, &c);
    configure_goal(&env, GOAL, signers, 2, TTL);

    let id = create_proposal(&env, GOAL, a.clone(), AMOUNT);
    approve_proposal(&env, GOAL, id, a.clone()); // 1 / 2
    approve_proposal(&env, GOAL, id, b.clone()); // 2 / 2 → auto-execute

    let p = get_proposal(&env, GOAL, id);
    assert_eq!(p.status, ProposalStatus::Executed);
    assert!(is_proposal_executed(&env, GOAL, id));
}

/// AC: Non-signers cannot approve proposals.
#[test]
#[should_panic]
fn test_non_signer_cannot_approve() {
    let (env, a, b, c) = setup();
    let signers = three_signers(&env, &a, &b, &c);
    configure_goal(&env, GOAL, signers, 2, TTL);

    let id = create_proposal(&env, GOAL, a.clone(), AMOUNT);
    let outsider = Address::generate(&env);
    approve_proposal(&env, GOAL, id, outsider);
}

/// AC: Attempting explicit execution before quorum is met panics.
#[test]
#[should_panic]
fn test_execute_before_quorum_panics() {
    let (env, a, b, c) = setup();
    let signers = three_signers(&env, &a, &b, &c);
    configure_goal(&env, GOAL, signers, 2, TTL);

    let id = create_proposal(&env, GOAL, a.clone(), AMOUNT);
    approve_proposal(&env, GOAL, id, a.clone()); // only 1 of 2
    execute_proposal(&env, GOAL, id, b.clone()); // should panic QuorumNotReached
}

/// AC: Proposals expire after a configurable window and cannot be executed afterward.
#[test]
#[should_panic]
fn test_expired_proposal_cannot_be_approved() {
    let (env, a, b, c) = setup();
    let signers = three_signers(&env, &a, &b, &c);
    // TTL of 100 seconds
    configure_goal(&env, GOAL, signers, 2, 100);

    let id = create_proposal(&env, GOAL, a.clone(), AMOUNT);

    // Fast-forward ledger past the TTL
    env.ledger().with_mut(|li| {
        li.timestamp = li.timestamp + 200;
    });

    // Should panic ProposalExpired
    approve_proposal(&env, GOAL, id, a.clone());
}

#[test]
#[should_panic]
fn test_expired_proposal_cannot_be_executed() {
    let (env, a, b, c) = setup();
    let signers = three_signers(&env, &a, &b, &c);
    configure_goal(&env, GOAL, signers, 1, 100); // threshold 1 so quorum is instant

    let id = create_proposal(&env, GOAL, a.clone(), AMOUNT);

    // Approve while still valid
    approve_proposal(&env, GOAL, id, a.clone()); // auto-executes already, but let's test the expiry path

    // For a clean expiry test: create a NEW proposal, advance time, then try explicit execute
    let id2 = create_proposal(&env, GOAL, b.clone(), AMOUNT);

    env.ledger().with_mut(|li| {
        li.timestamp = li.timestamp + 200;
    });

    execute_proposal(&env, GOAL, id2, a.clone()); // should panic ProposalExpired
}

/// Full 2-of-3 flow as specified in the acceptance criteria.
#[test]
fn test_2_of_3_full_flow() {
    let (env, a, b, c) = setup();
    let signers = three_signers(&env, &a, &b, &c);
    configure_goal(&env, GOAL, signers, 2, TTL);

    // Create proposal
    let id = create_proposal(&env, GOAL, a.clone(), AMOUNT);

    // Signer A approves – not executed yet
    approve_proposal(&env, GOAL, id, a.clone());
    assert!(!is_proposal_executed(&env, GOAL, id));
    assert!(has_approved(&env, GOAL, id, &a));
    assert!(!has_approved(&env, GOAL, id, &b));

    // Signer B approves – quorum reached, auto-execute
    approve_proposal(&env, GOAL, id, b.clone());
    assert!(is_proposal_executed(&env, GOAL, id));
    assert!(has_approved(&env, GOAL, id, &b));

    // C's approval would be a no-op (already executed) — not tested here to keep
    // the test focused; the AlreadyExecuted guard is covered separately.
}

/// AC: Cannot approve or execute an already-executed proposal.
#[test]
#[should_panic]
fn test_cannot_approve_executed_proposal() {
    let (env, a, b, c) = setup();
    let signers = three_signers(&env, &a, &b, &c);
    configure_goal(&env, GOAL, signers, 2, TTL);

    let id = create_proposal(&env, GOAL, a.clone(), AMOUNT);
    approve_proposal(&env, GOAL, id, a.clone());
    approve_proposal(&env, GOAL, id, b.clone()); // executes

    // C tries to add a third approval after execution
    approve_proposal(&env, GOAL, id, c.clone()); // should panic AlreadyExecuted
}

/// Verifies events are emitted (checks event count, not payloads).
#[test]
fn test_events_emitted_on_lifecycle() {
    let (env, a, b, c) = setup();
    env.mock_all_auths();
    let signers = three_signers(&env, &a, &b, &c);
    configure_goal(&env, GOAL, signers, 2, TTL);

    let id = create_proposal(&env, GOAL, a.clone(), AMOUNT);
    // 1 event: proposal_created

    approve_proposal(&env, GOAL, id, a.clone());
    // +1 event: proposal_approved (no execute yet)

    approve_proposal(&env, GOAL, id, b.clone());
    // +1 event: proposal_approved  +1 event: proposal_executed

    let events = env.events().all();
    assert_eq!(events.len(), 4);
}

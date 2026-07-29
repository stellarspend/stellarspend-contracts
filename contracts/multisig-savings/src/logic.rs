//! Core logic: configuration management, proposal lifecycle, approval collection.

use soroban_sdk::{panic_with_error, Address, Env, Vec};

use crate::errors::MultisigError;
use crate::events::MultisigEvents;
use crate::types::{DataKey, GoalMultisigConfig, ProposalStatus, WithdrawalProposal};

// ---------------------------------------------------------------------------
// Configuration helpers
// ---------------------------------------------------------------------------

/// Store multisig config for a goal. Caller must have already auth'd.
/// Validates: non-empty signers, threshold in [1, len], no duplicates.
pub fn configure_goal(
    env: &Env,
    goal_id: u64,
    signers: Vec<Address>,
    threshold: u32,
    proposal_ttl_seconds: u64,
) {
    validate_signer_config(env, &signers, threshold);

    let cfg = GoalMultisigConfig {
        signers,
        threshold,
        proposal_ttl_seconds,
    };
    env.storage()
        .persistent()
        .set(&DataKey::GoalConfig(goal_id), &cfg);
}

/// Return the config for a goal, panicking if not configured.
pub fn get_config(env: &Env, goal_id: u64) -> GoalMultisigConfig {
    env.storage()
        .persistent()
        .get(&DataKey::GoalConfig(goal_id))
        .unwrap_or_else(|| panic_with_error!(env, MultisigError::NotConfigured))
}

/// Returns `true` if multisig is configured for this goal.
pub fn is_configured(env: &Env, goal_id: u64) -> bool {
    env.storage()
        .persistent()
        .has(&DataKey::GoalConfig(goal_id))
}

/// Replace signers and threshold for an existing config.
pub fn update_signers(
    env: &Env,
    goal_id: u64,
    new_signers: Vec<Address>,
    new_threshold: u32,
) {
    validate_signer_config(env, &new_signers, new_threshold);
    let mut cfg = get_config(env, goal_id);
    cfg.signers = new_signers;
    cfg.threshold = new_threshold;
    env.storage()
        .persistent()
        .set(&DataKey::GoalConfig(goal_id), &cfg);
}

/// Update only the proposal TTL for an existing config.
pub fn update_ttl(env: &Env, goal_id: u64, proposal_ttl_seconds: u64) {
    let mut cfg = get_config(env, goal_id);
    cfg.proposal_ttl_seconds = proposal_ttl_seconds;
    env.storage()
        .persistent()
        .set(&DataKey::GoalConfig(goal_id), &cfg);
}

// ---------------------------------------------------------------------------
// Proposal creation
// ---------------------------------------------------------------------------

/// Create a new withdrawal proposal for a goal.
/// `proposer` must be in the signer set.
/// Returns the new proposal ID.
pub fn create_proposal(env: &Env, goal_id: u64, proposer: Address, amount: i128) -> u64 {
    proposer.require_auth();

    if amount <= 0 {
        panic_with_error!(env, MultisigError::InvalidAmount);
    }

    let cfg = get_config(env, goal_id);
    require_signer(env, &cfg, &proposer);

    let proposal_id = next_proposal_id(env, goal_id);
    let now = env.ledger().timestamp();
    let expires_at = now
        .checked_add(cfg.proposal_ttl_seconds)
        .unwrap_or_else(|| panic_with_error!(env, MultisigError::Overflow));

    let proposal = WithdrawalProposal {
        proposal_id,
        goal_id,
        proposer: proposer.clone(),
        amount,
        created_at: now,
        expires_at,
        approval_count: 0,
        status: ProposalStatus::Pending,
    };

    env.storage()
        .persistent()
        .set(&DataKey::Proposal(goal_id, proposal_id), &proposal);

    MultisigEvents::proposal_created(env, &proposal);

    proposal_id
}

// ---------------------------------------------------------------------------
// Approval collection
// ---------------------------------------------------------------------------

/// Record an approval from `signer` for the given proposal.
/// Auto-executes (marks Executed) if quorum is reached.
/// Returns the updated approval count.
pub fn approve_proposal(env: &Env, goal_id: u64, proposal_id: u64, signer: Address) -> u32 {
    signer.require_auth();

    let cfg = get_config(env, goal_id);
    require_signer(env, &cfg, &signer);

    let mut proposal = get_proposal(env, goal_id, proposal_id);

    // Guard: must be pending
    check_pending_or_panic(env, &proposal);

    // Guard: must not be expired
    let now = env.ledger().timestamp();
    if now >= proposal.expires_at {
        mark_expired(env, goal_id, proposal_id, &mut proposal);
        panic_with_error!(env, MultisigError::ProposalExpired);
    }

    // Guard: no duplicate approvals
    let approval_key = DataKey::Approval(goal_id, proposal_id, signer.clone());
    if env.storage().persistent().has(&approval_key) {
        panic_with_error!(env, MultisigError::DuplicateApproval);
    }

    // Record the approval
    env.storage().persistent().set(&approval_key, &true);
    let new_count = proposal
        .approval_count
        .checked_add(1)
        .unwrap_or_else(|| panic_with_error!(env, MultisigError::Overflow));
    proposal.approval_count = new_count;

    MultisigEvents::proposal_approved(env, goal_id, proposal_id, &signer, new_count, cfg.threshold);

    // Auto-execute when quorum is reached
    if new_count >= cfg.threshold {
        proposal.status = ProposalStatus::Executed;
        env.storage()
            .persistent()
            .set(&DataKey::Proposal(goal_id, proposal_id), &proposal);
        MultisigEvents::proposal_executed(env, goal_id, proposal_id, &signer, proposal.amount);
    } else {
        env.storage()
            .persistent()
            .set(&DataKey::Proposal(goal_id, proposal_id), &proposal);
    }

    new_count
}

// ---------------------------------------------------------------------------
// Explicit execution (for when auto-execute is not desired / manual path)
// ---------------------------------------------------------------------------

/// Execute an already-approved proposal explicitly.
/// Panics if quorum hasn't been reached, proposal is expired, or already executed.
pub fn execute_proposal(env: &Env, goal_id: u64, proposal_id: u64, executor: Address) {
    executor.require_auth();

    // executor must be a signer
    let cfg = get_config(env, goal_id);
    require_signer(env, &cfg, &executor);

    let mut proposal = get_proposal(env, goal_id, proposal_id);

    check_pending_or_panic(env, &proposal);

    let now = env.ledger().timestamp();
    if now >= proposal.expires_at {
        mark_expired(env, goal_id, proposal_id, &mut proposal);
        panic_with_error!(env, MultisigError::ProposalExpired);
    }

    if proposal.approval_count < cfg.threshold {
        panic_with_error!(env, MultisigError::QuorumNotReached);
    }

    proposal.status = ProposalStatus::Executed;
    env.storage()
        .persistent()
        .set(&DataKey::Proposal(goal_id, proposal_id), &proposal);

    MultisigEvents::proposal_executed(env, goal_id, proposal_id, &executor, proposal.amount);
}

// ---------------------------------------------------------------------------
// Query helpers (used by savings-goals for the withdrawal gate)
// ---------------------------------------------------------------------------

/// Returns the proposal. Panics if not found.
pub fn get_proposal(env: &Env, goal_id: u64, proposal_id: u64) -> WithdrawalProposal {
    env.storage()
        .persistent()
        .get(&DataKey::Proposal(goal_id, proposal_id))
        .unwrap_or_else(|| panic_with_error!(env, MultisigError::ProposalNotFound))
}

/// Returns true if the proposal has been executed (approved + executed).
pub fn is_proposal_executed(env: &Env, goal_id: u64, proposal_id: u64) -> bool {
    let proposal: Option<WithdrawalProposal> = env
        .storage()
        .persistent()
        .get(&DataKey::Proposal(goal_id, proposal_id));
    proposal
        .map(|p| p.status == ProposalStatus::Executed)
        .unwrap_or(false)
}

/// Returns true if the given address has approved the proposal.
pub fn has_approved(env: &Env, goal_id: u64, proposal_id: u64, signer: &Address) -> bool {
    env.storage()
        .persistent()
        .has(&DataKey::Approval(goal_id, proposal_id, signer.clone()))
}

/// Check whether a withdrawal amount requires multisig for this goal.
/// Returns false if the goal is not multisig-configured (plain withdrawal is fine).
pub fn requires_multisig(env: &Env, goal_id: u64) -> bool {
    is_configured(env, goal_id)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn next_proposal_id(env: &Env, goal_id: u64) -> u64 {
    let current: u64 = env
        .storage()
        .persistent()
        .get(&DataKey::NextProposalId(goal_id))
        .unwrap_or(0);
    let next = current
        .checked_add(1)
        .unwrap_or_else(|| panic_with_error!(env, MultisigError::Overflow));
    env.storage()
        .persistent()
        .set(&DataKey::NextProposalId(goal_id), &next);
    next
}

fn require_signer(env: &Env, cfg: &GoalMultisigConfig, caller: &Address) {
    for s in cfg.signers.iter() {
        if s == caller.clone() {
            return;
        }
    }
    panic_with_error!(env, MultisigError::UnauthorizedSigner);
}

fn check_pending_or_panic(env: &Env, proposal: &WithdrawalProposal) {
    match proposal.status {
        ProposalStatus::Pending => {}
        ProposalStatus::Executed => panic_with_error!(env, MultisigError::AlreadyExecuted),
        ProposalStatus::Expired => panic_with_error!(env, MultisigError::ProposalExpired),
        ProposalStatus::Approved => {} // intermediate state; execution path handles it
    }
}

fn mark_expired(env: &Env, goal_id: u64, proposal_id: u64, proposal: &mut WithdrawalProposal) {
    proposal.status = ProposalStatus::Expired;
    env.storage()
        .persistent()
        .set(&DataKey::Proposal(goal_id, proposal_id), proposal);
    MultisigEvents::proposal_expired(env, goal_id, proposal_id, proposal.expires_at);
}

fn validate_signer_config(env: &Env, signers: &Vec<Address>, threshold: u32) {
    let n = signers.len();
    if n == 0 || threshold == 0 || threshold > n {
        panic_with_error!(env, MultisigError::InvalidConfig);
    }
    // Duplicate check O(n²) – acceptable for small signer sets
    for i in 0..n {
        let a = signers
            .get(i)
            .unwrap_or_else(|| panic_with_error!(env, MultisigError::InvalidConfig));
        for j in (i + 1)..n {
            let b = signers
                .get(j)
                .unwrap_or_else(|| panic_with_error!(env, MultisigError::InvalidConfig));
            if a == b {
                panic_with_error!(env, MultisigError::DuplicateSigner);
            }
        }
    }
}

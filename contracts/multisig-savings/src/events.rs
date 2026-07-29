use soroban_sdk::{symbol_short, Address, Env};

use crate::types::WithdrawalProposal;

/// Event helpers for multisig-savings.
/// Each event uses a 2-symbol topic so the 3-element tuple stays within
/// Soroban's limit and is easy to filter off-chain.
pub struct MultisigEvents;

impl MultisigEvents {
    /// Emitted when a new withdrawal proposal is created.
    pub fn proposal_created(env: &Env, proposal: &WithdrawalProposal) {
        let topics = (
            symbol_short!("ms"),
            symbol_short!("created"),
            proposal.proposal_id,
        );
        env.events().publish(
            topics,
            (
                proposal.goal_id,
                proposal.proposer.clone(),
                proposal.amount,
                proposal.expires_at,
            ),
        );
    }

    /// Emitted when a signer approves a proposal.
    pub fn proposal_approved(
        env: &Env,
        goal_id: u64,
        proposal_id: u64,
        signer: &Address,
        approval_count: u32,
        threshold: u32,
    ) {
        let topics = (
            symbol_short!("ms"),
            symbol_short!("approved"),
            proposal_id,
        );
        env.events()
            .publish(topics, (goal_id, signer.clone(), approval_count, threshold));
    }

    /// Emitted when a proposal is executed (quorum reached and withdrawal released).
    pub fn proposal_executed(
        env: &Env,
        goal_id: u64,
        proposal_id: u64,
        executor: &Address,
        amount: i128,
    ) {
        let topics = (
            symbol_short!("ms"),
            symbol_short!("executed"),
            proposal_id,
        );
        env.events()
            .publish(topics, (goal_id, executor.clone(), amount));
    }

    /// Emitted when a proposal is detected as expired.
    pub fn proposal_expired(env: &Env, goal_id: u64, proposal_id: u64, expired_at: u64) {
        let topics = (
            symbol_short!("ms"),
            symbol_short!("expired"),
            proposal_id,
        );
        env.events().publish(topics, (goal_id, expired_at));
    }
}

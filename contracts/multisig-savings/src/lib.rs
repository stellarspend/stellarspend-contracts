//! # multisig-savings
//!
//! Multisig withdrawal approval for savings goals.
//!
//! ## Design
//! Each savings goal can be independently protected by a multisig signer set.
//! Before a withdrawal from a protected goal may proceed, a `WithdrawalProposal`
//! must be created and approved by at least `threshold` distinct signers within
//! the configured `proposal_ttl_seconds` window.
//!
//! ## Architectural decision
//! Implemented as a **separate crate** (not a module inside savings-goals) because:
//! * The logic is substantial (~900 source lines across the four orphaned files).
//! * It may be reused by other contracts (escrow, shared-budgets, etc.).
//! * A clean boundary keeps savings-goals focused on goal management.
//! * This follows the project's existing per-feature crate pattern.
//!
//! ## Public surface used by savings-goals
//! ```text
//! multisig_savings::configure_goal(env, goal_id, signers, threshold, ttl_secs)
//! multisig_savings::requires_multisig(env, goal_id) -> bool
//! multisig_savings::create_proposal(env, goal_id, proposer, amount) -> u64
//! multisig_savings::approve_proposal(env, goal_id, proposal_id, signer) -> u32
//! multisig_savings::execute_proposal(env, goal_id, proposal_id, executor)
//! multisig_savings::is_proposal_executed(env, goal_id, proposal_id) -> bool
//! ```

#![no_std]

mod errors;
mod events;
mod logic;
mod types;

#[cfg(test)]
mod tests;

// Re-export everything callers need.
pub use errors::MultisigError;
pub use events::MultisigEvents;
pub use logic::{
    approve_proposal, configure_goal, create_proposal, execute_proposal, get_config, get_proposal,
    has_approved, is_configured, is_proposal_executed, requires_multisig, update_signers,
    update_ttl,
};
pub use types::{DataKey, GoalMultisigConfig, ProposalStatus, WithdrawalProposal};

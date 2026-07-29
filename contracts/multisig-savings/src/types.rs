use soroban_sdk::{contracttype, Address, Vec};

/// Status of a withdrawal proposal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum ProposalStatus {
    /// Awaiting approvals.
    Pending = 0,
    /// Quorum reached; ready to execute (set atomically with execution).
    Approved = 1,
    /// Executed successfully.
    Executed = 2,
    /// Expired without reaching quorum.
    Expired = 3,
}

/// A withdrawal proposal tied to a savings goal.
#[derive(Clone)]
#[contracttype]
pub struct WithdrawalProposal {
    /// Unique proposal ID (auto-incremented per-goal).
    pub proposal_id: u64,
    /// The savings-goals goal this proposal targets.
    pub goal_id: u64,
    /// Address that created the proposal.
    pub proposer: Address,
    /// Withdrawal amount requested (in stroops).
    pub amount: i128,
    /// Ledger timestamp when the proposal was created.
    pub created_at: u64,
    /// Ledger timestamp after which the proposal may not execute.
    pub expires_at: u64,
    /// Current number of distinct approvals.
    pub approval_count: u32,
    /// Current status.
    pub status: ProposalStatus,
}

/// Multisig configuration stored per goal.
#[derive(Clone)]
#[contracttype]
pub struct GoalMultisigConfig {
    /// Ordered list of authorized signers.
    pub signers: Vec<Address>,
    /// Number of approvals required (M of N).
    pub threshold: u32,
    /// How long (in seconds) a new proposal lives before expiring.
    pub proposal_ttl_seconds: u64,
}

/// Storage keys used by this crate.
/// All keys are scoped by goal_id to avoid collisions across goals.
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    /// GoalMultisigConfig for a goal.
    GoalConfig(u64),
    /// Next proposal ID counter for a goal.
    NextProposalId(u64),
    /// WithdrawalProposal keyed by (goal_id, proposal_id).
    Proposal(u64, u64),
    /// Approval flag keyed by (goal_id, proposal_id, signer).
    Approval(u64, u64, Address),
}

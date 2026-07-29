use soroban_sdk::contracterror;

/// All error codes for the multisig-savings crate.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum MultisigError {
    /// The multisig config for this goal has not been set up.
    NotConfigured = 1,
    /// Signers list is empty, threshold is zero, or threshold > signer count.
    InvalidConfig = 2,
    /// Caller is not in the signer set for this goal.
    UnauthorizedSigner = 3,
    /// The proposal was not found.
    ProposalNotFound = 4,
    /// This signer already approved this proposal.
    DuplicateApproval = 5,
    /// The proposal has already been executed.
    AlreadyExecuted = 6,
    /// The proposal has been cancelled / superseded.
    Cancelled = 7,
    /// Not enough approvals yet to execute.
    QuorumNotReached = 8,
    /// The proposal's expiry timestamp has passed.
    ProposalExpired = 9,
    /// Amount must be positive.
    InvalidAmount = 10,
    /// Duplicate signer in the provided signer list.
    DuplicateSigner = 11,
    /// Numeric overflow.
    Overflow = 12,
}

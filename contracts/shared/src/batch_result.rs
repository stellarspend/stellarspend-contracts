use soroban_sdk::{contracttype, Address, Vec};

/// Shared per-item result shape used by batch contracts.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct BatchItemResult {
    pub success: bool,
    pub target: Address,
    pub amount: i128,
    pub error_code: u32,
}

/// Shared summary shape for batch execution results.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct BatchExecutionResult {
    pub total_requests: u32,
    pub successful: u32,
    pub failed: u32,
    pub results: Vec<BatchItemResult>,
}

/// Explicit atomicity policy for a batch contract.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum BatchAtomicityPolicy {
    /// Abort the entire batch if any item is invalid.
    AllOrNothing,
    /// Continue processing the remaining valid items and report failures individually.
    BestEffort,
}

use soroban_sdk::{contracttype, Address, Symbol};

/// One rule in a user's spending policy: a per-category limit, plus the
/// amount above which a zero-knowledge proof is required before a spend in
/// that category is authorized (enforced by the zk-verifier contract and the
/// spending-rules composition layer — this contract only stores the rule).
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PolicyRule {
    pub category: Symbol,
    pub limit: i128,
    pub zk_required_above: i128,
}

/// Storage keys for this contract.
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    /// user -> `Vec<PolicyRule>`
    Policy(Address),
}

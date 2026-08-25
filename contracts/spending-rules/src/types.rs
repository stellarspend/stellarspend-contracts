use soroban_sdk::{contracttype, Address, Symbol};

/// A single spending rule: a per-category weekly cap plus the amount above
/// which a zero-knowledge proof is required before a payment in that category
/// is authorized (the ZK proof is verified by the zk-verifier contract).
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Rule {
    pub category: Symbol,
    pub weekly_limit: i128,
    pub zk_required_above: i128,
}

/// Contract configuration: the administrator plus the addresses of the three
/// contracts this composition engine calls cross-contract.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Config {
    pub admin: Address,
    pub limits_contract: Address,
    pub categories_contract: Address,
    pub zk_verifier_contract: Address,
}

/// Storage keys. Configuration lives in instance storage; per-user rules in
/// persistent storage, matching the tiering convention used by the other
/// spending-* contracts (see docs/ARCHITECTURE.md).
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    /// Contract-wide configuration.
    Config,
    /// (user, category) -> Rule
    Rule(Address, Symbol),
}

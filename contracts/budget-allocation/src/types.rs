use soroban_sdk::{contracttype, Address, Map, Symbol, Vec};

/// Request structure for setting a user's budget
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BudgetRequest {
    /// The user address to set budget for
    pub user: Address,
    /// The monthly budget amount
    pub amount: i128,
}

/// Budget category structure
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BudgetCategory {
    /// Category name (e.g., "food", "transport", "entertainment")
    pub name: Symbol,
    /// Budget amount for this category
    pub amount: i128,
}

/// Request structure for allocating budgets across multiple categories
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CategoryBudgetRequest {
    /// The user address to set budget for
    pub user: Address,
    /// List of budget categories and amounts
    pub categories: Vec<BudgetCategory>,
    /// Total budget amount (must equal sum of categories)
    pub total_amount: i128,
}

/// Stored budget record for a user
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BudgetRecord {
    pub user: Address,
    pub amount: i128,
    pub last_updated: u64,
}

/// Stored budget categories for a user
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserBudgetCategories {
    pub user: Address,
    pub categories: Map<Symbol, i128>, // category name -> amount
    pub total_amount: i128,
    pub last_updated: u64,
}

/// Query response summarizing a user's budget allocation state
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BudgetAllocationSummary {
    pub remaining_allocation: i128,
    pub total_allocation: i128,
    pub usage_percentage: i128,
}

/// Storage keys for the contract
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    Budget(Address),
    BudgetCategories(Address),    // User's budget categories
    TotalAllocated,               // Track global stats if needed
    BudgetSnapshot(u64, Address), // Snapshot of budget state at timestamp
    SnapshotTimestamps,           // List of all snapshot timestamps
    BudgetRenewalConfig(Address), // Per-user budget renewal configuration
    BudgetVersion(Address, u64),  // Budget version snapshots (user, version_number)
    BudgetVersionCounter(Address), // Number of versions created for a user
}

/// Result of a batch budget allocation operation
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchBudgetResult {
    pub successful: u32,
    pub failed: u32,
    pub total_amount: i128,
}

/// Budget renewal configuration for automatic budget cycling.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BudgetRenewalConfig {
    /// The address whose budget is configured for renewal
    pub user: Address,
    /// Renewal frequency in seconds (e.g., 2_592_000 for 30 days)
    pub frequency_seconds: u64,
    /// Whether automatic renewal is enabled
    pub enabled: bool,
    /// The amount to renew the budget to each cycle
    pub renewal_amount: i128,
    /// Timestamp of the last renewal execution
    pub last_renewed_at: u64,
    /// Number of renewals executed so far
    pub renewal_count: u64,
}

/// A snapshot of a budget's state preserved for historical tracking.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BudgetVersion {
    /// The version number (monotonically increasing)
    pub version: u64,
    /// The user whose budget is versioned
    pub user: Address,
    /// The budget amount at this version
    pub amount: i128,
    /// Snapshot timestamp
    pub created_at: u64,
    /// The renewal that triggered this version (0 if manual)
    pub renewal_id: u64,
}

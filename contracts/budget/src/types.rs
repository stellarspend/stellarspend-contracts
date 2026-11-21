use soroban_sdk::{contracttype, Address, Symbol};

/// Global contract configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Config {
    /// Contract administrator. Not required for any per-user budget
    /// operation today; kept for parity with the rest of the StellarSpend
    /// contracts and as a hook for future admin-gated functionality.
    pub admin: Address,
    /// Auto-incrementing counter used to allocate budget ids.
    pub last_budget_id: u64,
}

/// A user's budget for a spending category over a time window.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Budget {
    /// Unique identifier for this budget.
    pub budget_id: u64,
    /// Address of the user who owns this budget.
    pub user: Address,
    /// Human-readable name for this budget (e.g. "Monthly groceries").
    pub name: Symbol,
    /// Allocated spending amount in the budget's asset.
    pub amount: i128,
    /// Spending category this budget belongs to (e.g. "groceries").
    pub category: Symbol,
    /// Asset symbol the budget is denominated in (e.g. "XLM").
    pub asset: Symbol,
    /// Unix timestamp when the budget becomes active.
    pub start_date: u64,
    /// Unix timestamp when the budget expires.
    pub end_date: u64,
    /// Ledger timestamp when the budget was created.
    pub created_at: u64,
}

/// Storage keys used by this contract.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum DataKey {
    /// Key for the global contract configuration.
    Config,
    /// Key for a specific budget identified by its id.
    Budget(u64),
    /// Key for the list of budget ids belonging to a user.
    UserBudgets(Address),
    /// Key for the admin-configured value.
    Value,
}

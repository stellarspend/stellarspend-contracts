use soroban_sdk::{contracttype, Address};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Payment(u64),
    PaymentCount,
    /// Income stream by ID
    IncomeStream(u64),
    /// Income stream count
    IncomeStreamCount,
    /// Income streams for a user
    UserIncomeStreams(Address),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecurringPayment {
    pub sender: Address,
    pub recipient: Address,
    pub token: Address,
    pub amount: i128,
    pub interval: u64,
    pub next_execution: u64,
    pub active: bool,
    pub paused: bool,
    pub execution_count: u32,
    pub missed_count: u32,
    pub last_missed_at: u64,
}

/// Represents a recurring income stream that auto-funds budgets or goals.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IncomeStream {
    /// Unique stream ID
    pub stream_id: u64,
    /// The address receiving the income
    pub recipient: Address,
    /// Source description (e.g., "salary", "staking_rewards")
    pub source: soroban_sdk::Symbol,
    /// Amount received per interval
    pub amount: i128,
    /// Seconds between income events
    pub interval_seconds: u64,
    /// Ledger timestamp of the next scheduled income
    pub next_payout: u64,
    /// Target budget/goal ID to auto-fund (0 = manual allocation)
    pub target_goal_id: u64,
    /// Whether the stream is active
    pub active: bool,
}

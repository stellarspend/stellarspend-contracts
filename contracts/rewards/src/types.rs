//! Data types and storage keys for the rewards contract.

use soroban_sdk::{contracttype, Address};

// ── Constants ─────────────────────────────────────────────────────────────────

/// TTL bump for persistent storage entries (~1 year in ledgers at ~5s/ledger).
pub const PERSISTENT_TTL_BUMP: u32 = 6_307_200;

/// Default schema version for reward account metadata.
pub const DEFAULT_METADATA_VERSION: u32 = 1;

// ── Storage keys ──────────────────────────────────────────────────────────────

/// All storage keys used by the rewards contract.
///
/// | Key | Storage tier | Description |
/// |---|---|---|
/// | `Admin` | Instance | Contract administrator address |
/// | `Initialized` | Instance | Initialization sentinel |
/// | `RewardBalance(Address)` | Persistent | Current claimable reward balance (stroops) |
/// | `LifetimeEarned(Address)` | Persistent | Total rewards ever earned (stroops) |
/// | `LifetimeClaimed(Address)` | Persistent | Total rewards ever claimed (stroops) |
/// | `RewardAccount(Address)` | Persistent | Full reward account metadata struct |
/// | `RewardTransaction(u64)` | Persistent | Individual reward transaction record by ID |
/// | `RewardIndex(Address)` | Persistent | Ordered list of tx IDs for a participant |
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    /// Contract administrator address (instance storage).
    Admin,
    /// Initialization sentinel (instance storage).
    Initialized,
    /// Current claimable reward balance for an account (persistent storage).
    RewardBalance(Address),
    /// Cumulative rewards earned over the account lifetime (persistent storage).
    LifetimeEarned(Address),
    /// Cumulative rewards claimed over the account lifetime (persistent storage).
    LifetimeClaimed(Address),
    /// Full reward account metadata (persistent storage).
    RewardAccount(Address),
    /// Individual reward transaction record, keyed by transaction ID (persistent storage).
    RewardTransaction(u64),
    /// Monotonically incrementing counter for reward transaction IDs (instance storage).
    RewardTxCounter,
    /// Per-participant ordered list of reward transaction IDs (persistent storage).
    ///
    /// Stores a `Vec<u64>` appended to on every `credit_reward` call, enabling
    /// O(1) lookup of all transactions belonging to an address.
    RewardIndex(Address),
}

// ── Enums ─────────────────────────────────────────────────────────────────────

/// Classifies the origin or mechanism of a reward.
///
/// Used in [`RewardTransaction`] to describe why a reward was issued.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RewardType {
    /// Reward issued for staying within a spending limit.
    SpendingLimit,
    /// Reward issued for completing a savings goal.
    SavingsGoal,
    /// Reward issued for a streak of responsible financial behaviour.
    Streak,
    /// Reward issued as a referral incentive.
    Referral,
    /// Reward issued by the protocol admin as a manual grant.
    ManualGrant,
}

/// Lifecycle state of a single reward transaction.
///
/// Transitions: `Pending` → `Confirmed` or `Pending` → `Cancelled`.
/// A `Claimed` record is terminal and may not be reversed.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RewardStatus {
    /// Reward has been issued but not yet confirmed on-chain.
    Pending,
    /// Reward has been confirmed and is available to claim.
    Confirmed,
    /// Reward has been claimed by the recipient.
    Claimed,
    /// Reward was cancelled before it could be claimed.
    Cancelled,
}

/// Status of a reward account.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AccountStatus {
    /// Account is active and operating normally.
    Active,
    /// Account is suspended.
    Suspended,
    /// Account is inactive.
    Inactive,
}

// ── Structs ───────────────────────────────────────────────────────────────────

/// Metadata associated with a reward account.
///
/// Persisted under `DataKey::RewardAccount(address)`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RewardAccount {
    /// The owner of this reward account.
    pub owner: Address,
    /// Current claimable balance in stroops.
    pub balance: i128,
    /// Total rewards earned over the lifetime of the account in stroops.
    pub lifetime_earned: i128,
    /// Total rewards claimed over the lifetime of the account in stroops.
    pub lifetime_claimed: i128,
    /// Unix timestamp at which the account was first created.
    pub created_at: u64,
    /// Unix timestamp of the most recent account update.
    pub last_updated: u64,
    /// Current protocol status of the account.
    pub account_status: AccountStatus,
    /// Version of the metadata schema.
    pub metadata_version: u32,
}

/// A record of a single reward issuance or claim event.
///
/// Persisted under `DataKey::RewardTransaction(id)`.
/// Every time a reward is issued or claimed a new `RewardTransaction` is written.
#[contracttype]
#[derive(Clone, Debug)]
pub struct RewardTransaction {
    /// Unique, monotonically incrementing transaction identifier.
    pub id: u64,
    /// The account that received (or will receive) this reward.
    pub recipient: Address,
    /// Reward amount in stroops.
    pub amount: i128,
    /// Reason the reward was issued.
    pub reward_type: RewardType,
    /// Current lifecycle state of this transaction.
    pub status: RewardStatus,
    /// Ledger sequence at which this transaction was created.
    pub created_at: u64,
    /// Ledger sequence at which the status was last updated (`0` if never updated).
    pub updated_at: u64,
}

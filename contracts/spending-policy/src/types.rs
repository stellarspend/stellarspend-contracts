//! Type definitions for the spending policy contract.
//!
//! This module defines all rule types, the policy envelope, evaluation
//! results, pending (approval-bound) transactions, storage keys, events
//! and the constants that bound policy size.

use soroban_sdk::{contracttype, symbol_short, Address, Env, Symbol, Vec};

/// Maximum number of rules allowed in a single policy.
pub const MAX_RULES: u32 = 50;

/// Maximum number of approvers allowed in an `ApprovalThreshold` rule.
pub const MAX_APPROVERS: u32 = 20;

/// Number of seconds in a single UTC day.
pub const SECONDS_PER_DAY: u64 = 86_400;

// ---------------------------------------------------------------------------
// Rule types
// ---------------------------------------------------------------------------

/// Limits the total amount that may be spent in a given spending category
/// over a rolling time period.
///
/// The period is identified by `floor(ledger_timestamp / period_seconds)`.
/// Spending is tracked per (wallet, category, period) and resets naturally
/// when the ledger timestamp advances into a new period.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct CategoryLimitRule {
    /// Spending category this limit applies to (e.g. `symbol_short!("groc")`).
    pub category: Symbol,
    /// Maximum cumulative spend allowed within one period (in stroops).
    pub max_amount: i128,
    /// Length of the rolling period in seconds (e.g. 604_800 for a week).
    pub period_seconds: u64,
}

/// Only permits transactions whose recipient appears in the allowlist.
///
/// When multiple `MerchantAllowlistRule`s are present, a recipient must be
/// present in **every** allowlist to be permitted (intersection semantics).
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct MerchantAllowlistRule {
    /// Addresses that are permitted to receive funds.
    pub allowed: Vec<Address>,
}

/// Rejects any transaction whose recipient appears in the blocklist.
///
/// When multiple `MerchantBlocklistRule`s are present, a recipient is blocked
/// if it appears in **any** blocklist (union semantics).
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct MerchantBlocklistRule {
    /// Addresses that are forbidden from receiving funds.
    pub blocked: Vec<Address>,
}

/// Restricts transactions to a specific time-of-day window expressed in
/// seconds since midnight UTC.
///
/// The window is half-open: `[start_seconds, end_seconds)`.
/// When `start_seconds < end_seconds` the window is a normal range
/// (e.g. `[21_600, 86_400)` = 06:00 → midnight).
/// When `start_seconds > end_seconds` the window wraps past midnight
/// (e.g. `[79_200, 7_200)` = 22:00 → 02:00).
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct TimeWindowRule {
    /// Start of the permitted window in seconds since midnight UTC.
    pub start_seconds: u64,
    /// End of the permitted window in seconds since midnight UTC (exclusive).
    pub end_seconds: u64,
}

/// Requires `required_approvals` signatures from the configured approver set
/// before any transaction whose amount is `>= threshold_amount` is released.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct ApprovalThresholdRule {
    /// Amount at or above which multi-approval is required (in stroops).
    pub threshold_amount: i128,
    /// Number of distinct approvals required to release a pending tx.
    pub required_approvals: u32,
    /// Addresses permitted to submit approvals.
    pub approvers: Vec<Address>,
}

/// A single combinable policy rule.
///
/// All variants are evaluated by `evaluate_transaction`. The order in which
/// rules are evaluated is deterministic (see `lib::evaluate_transaction`).
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum PolicyRule {
    CategoryLimit(CategoryLimitRule),
    MerchantAllowlist(MerchantAllowlistRule),
    MerchantBlocklist(MerchantBlocklistRule),
    TimeWindow(TimeWindowRule),
    ApprovalThreshold(ApprovalThresholdRule),
}

/// The full policy rule set for a wallet.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Policy {
    /// Ordered list of rules to enforce.
    pub rules: Vec<PolicyRule>,
    /// Monotonically increasing version. Bumped on every `set_policy` call so
    /// that pending transactions created under an older version are invalidated.
    pub version: u32,
    /// Ledger timestamp of the last update.
    pub updated_at: u64,
}

// ---------------------------------------------------------------------------
// Evaluation / approval outcomes
// ---------------------------------------------------------------------------

/// Machine-readable reason a transaction was rejected.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum RejectionReason {
    /// No amount supplied or amount <= 0.
    InvalidAmount,
    /// Spending in the category would exceed the configured `CategoryLimit`.
    CategoryLimitExceeded,
    /// Recipient is present on a `MerchantBlocklist`.
    MerchantBlocked,
    /// Recipient is not present on a `MerchantAllowlist`.
    MerchantNotAllowed,
    /// Transaction was attempted outside the permitted `TimeWindow`.
    OutsideTimeWindow,
}

/// Result of evaluating a transaction against a wallet's policy.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum EvaluationResult {
    /// All rules passed; the transaction may proceed.
    Approved,
    /// A rule rejected the transaction.
    Rejected(RejectionReason),
    /// The transaction is above an approval threshold and is held pending.
    /// The enclosed value is the pending transaction id.
    PendingApproval(u64),
}

/// Result of submitting an approval for a pending transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum ApprovalOutcome {
    /// The required quorum was reached and the transaction was released.
    Approved,
    /// The transaction remains pending. The enclosed value is the current
    /// count of collected approvals.
    Pending(u32),
}

/// A transaction held pending until enough approvals are collected.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PendingTransaction {
    /// Unique id of the pending transaction.
    pub id: u64,
    /// Wallet that owns the policy / initiated the transaction.
    pub wallet: Address,
    /// Intended recipient of the funds.
    pub recipient: Address,
    /// Amount to be transferred (in stroops).
    pub amount: i128,
    /// Optional spending category.
    pub category: Option<Symbol>,
    /// Ledger timestamp when the pending transaction was created.
    pub created_at: u64,
    /// Version of the policy under which the transaction was created.
    pub policy_version: u32,
    /// Addresses authorised to approve this transaction (snapshot of the
    /// threshold rule's approver list at creation time).
    pub authorized_approvers: Vec<Address>,
    /// Addresses that have already approved.
    pub approvers: Vec<Address>,
    /// Number of approvals required to release the transaction.
    pub required_approvals: u32,
}

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

/// Storage keys for contract state.
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    /// The policy for a wallet address.
    Policy(Address),
    /// A pending transaction by id.
    PendingTx(u64),
    /// Monotonic counter for pending transaction ids (instance storage).
    NextPendingId,
    /// Per-(wallet, category, period) spend tracking.
    CategorySpending(Address, Symbol, u64),
    /// List of pending transaction ids for a wallet.
    PendingByWallet(Address),
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Event helpers for the spending policy contract.
pub struct PolicyEvents;

impl PolicyEvents {
    /// Emitted when a policy is set for the first time.
    pub fn policy_set(env: &Env, wallet: &Address, version: u32) {
        env.events().publish(
            (symbol_short!("policy"), symbol_short!("set")),
            (wallet.clone(), version),
        );
    }

    /// Emitted when an existing policy is replaced.
    pub fn policy_replaced(env: &Env, wallet: &Address, version: u32) {
        env.events().publish(
            (symbol_short!("policy"), symbol_short!("repl")),
            (wallet.clone(), version),
        );
    }

    /// Emitted when a transaction is approved by all rules.
    pub fn tx_approved(env: &Env, wallet: &Address, amount: i128) {
        env.events().publish(
            (symbol_short!("tx"), symbol_short!("apprvd")),
            (wallet.clone(), amount),
        );
    }

    /// Emitted when a transaction is rejected.
    pub fn tx_rejected(env: &Env, wallet: &Address, amount: i128, reason: &RejectionReason) {
        env.events().publish(
            (symbol_short!("tx"), symbol_short!("rjctd")),
            (wallet.clone(), amount, reason.clone()),
        );
    }

    /// Emitted when a transaction is held pending approval.
    pub fn tx_pending(env: &Env, wallet: &Address, pending_id: u64, amount: i128) {
        env.events().publish(
            (symbol_short!("tx"), symbol_short!("pend")),
            (wallet.clone(), pending_id, amount),
        );
    }

    /// Emitted when an approver signs off on a pending transaction.
    pub fn approval_submitted(env: &Env, wallet: &Address, pending_id: u64, approver: &Address) {
        env.events().publish(
            (symbol_short!("apprv"), symbol_short!("sub")),
            (wallet.clone(), pending_id, approver.clone()),
        );
    }

    /// Emitted when a pending transaction is auto-released after reaching quorum.
    pub fn pending_released(env: &Env, wallet: &Address, pending_id: u64) {
        env.events().publish(
            (symbol_short!("pend"), symbol_short!("rels")),
            (wallet.clone(), pending_id),
        );
    }
}

//! # Spending Policy Contract
//!
//! A Soroban smart contract that lets a wallet owner define a programmable
//! rule set governing **all** outgoing transactions from their wallet.
//!
//! The supported rule types — all of which can be freely combined — are:
//!
//! | Rule                  | Effect                                                       |
//! |-----------------------|--------------------------------------------------------------|
//! | `CategoryLimit`       | Caps total spend per category per rolling period.           |
//! | `MerchantAllowlist`   | Only permits recipients on the list.                        |
//! | `MerchantBlocklist`   | Rejects recipients on the list.                             |
//! | `TimeWindow`          | Only permits transactions within a time-of-day window.      |
//! | `ApprovalThreshold`   | Transactions `>= threshold` require N-of-M approvals.       |
//!
//! ## Ownership model
//!
//! Each wallet address owns its own policy. `set_policy` requires the wallet
//! address to authorise the call (`require_auth`), so only the wallet owner
//! can create or replace their policy. There is no global admin.
//!
//! ## Atomic replacement
//!
//! `set_policy` replaces the entire rule set in a single storage write and
//! bumps the policy `version`. Any pending (approval-bound) transactions
//! created under a previous version are invalidated immediately: their stored
//! `policy_version` no longer matches the current version, so a subsequent
//! `submit_approval` for them panics with `PendingTxExpired`.
//!
//! ## Conflict resolution
//!
//! If a recipient appears on both a `MerchantAllowlist` and a
//! `MerchantBlocklist`, the blocklist wins (`MerchantBlocked`). See the
//! `validation` module docs for the full rationale.

#![no_std]

#[cfg(test)]
extern crate std;

mod rules;
mod storage;
mod types;
mod validation;

use soroban_sdk::{contract, contractimpl, panic_with_error, Address, Env, Symbol, Vec};

pub use crate::types::{
    ApprovalOutcome, ApprovalThresholdRule, CategoryLimitRule, DataKey, EvaluationResult,
    MerchantAllowlistRule, MerchantBlocklistRule, PendingTransaction, Policy, PolicyRule,
    RejectionReason, TimeWindowRule, MAX_APPROVERS, MAX_RULES, SECONDS_PER_DAY,
};

use crate::types::{PolicyEvents, PolicyRule::*};
use crate::validation::validate_policy;

/// Error codes for the spending policy contract.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum SpendingPolicyError {
    /// Caller is not authorised to manage this wallet's policy.
    Unauthorized = 1,
    /// No policy found for the wallet.
    PolicyNotFound = 2,
    /// The supplied policy rule set is malformed.
    InvalidPolicy = 3,
    /// A transaction amount was non-positive.
    InvalidAmount = 4,
    /// A `TimeWindow` rule had invalid bounds.
    InvalidTimeWindow = 5,
    /// An `ApprovalThreshold` rule had an invalid threshold/quorum.
    InvalidThreshold = 6,
    /// A pending transaction id does not exist.
    PendingTxNotFound = 7,
    /// An approver has already approved this pending transaction.
    AlreadyApproved = 8,
    /// The caller is not in the authorised approver set.
    NotAnApprover = 9,
    /// The pending transaction was invalidated by a policy replacement.
    PendingTxExpired = 10,
    /// The policy contains too many rules.
    TooManyRules = 11,
    /// A `CategoryLimit` rule had invalid parameters.
    InvalidCategoryLimit = 12,
    /// An `ApprovalThreshold` rule had an invalid approver list.
    InvalidApproverList = 13,
    /// A pending transaction could not be released because the category limit
    /// would be exceeded (re-checked at release time).
    CategoryLimitExceeded = 14,
}

impl From<SpendingPolicyError> for soroban_sdk::Error {
    fn from(e: SpendingPolicyError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

#[contract]
pub struct SpendingPolicyContract;

#[contractimpl]
impl SpendingPolicyContract {
    // -----------------------------------------------------------------------
    // set_policy
    // -----------------------------------------------------------------------

    /// Sets the wallet's full policy rule set, atomically replacing any
    /// existing policy.
    ///
    /// The `wallet` address must authorise the call. Replacing a policy bumps
    /// its `version`, which immediately invalidates every pending transaction
    /// created under the previous version.
    ///
    /// # Panics
    /// - `Unauthorized` if the wallet does not authorise.
    /// - A `SpendingPolicyError` if the rule set fails validation.
    pub fn set_policy(env: Env, wallet: Address, rules: Vec<PolicyRule>) {
        wallet.require_auth();

        if let Err(e) = validate_policy(&env, &rules) {
            panic_with_error!(&env, e);
        }

        let existing = storage::get_policy(&env, &wallet);
        let new_version = match &existing {
            Some(p) => p.version.wrapping_add(1),
            None => 1,
        };
        let now = env.ledger().timestamp();

        let policy = Policy {
            rules,
            version: new_version,
            updated_at: now,
        };

        // Single storage write => atomic replacement.
        storage::set_policy(&env, &wallet, &policy);

        if existing.is_some() {
            PolicyEvents::policy_replaced(&env, &wallet, new_version);
        } else {
            PolicyEvents::policy_set(&env, &wallet, new_version);
        }
    }

    // -----------------------------------------------------------------------
    // evaluate_transaction
    // -----------------------------------------------------------------------

    /// Evaluates a proposed outgoing transaction against the wallet's policy.
    ///
    /// Must be called **before** the transaction is executed. On `Approved`
    /// the category spend (if any matching `CategoryLimit` rule exists) is
    /// recorded immediately. On `PendingApproval` the transaction is held
    /// pending until enough approvals are collected via `submit_approval`.
    ///
    /// # Evaluation order (deterministic)
    /// 1. Amount sanity (`> 0`).
    /// 2. `MerchantBlocklist` — reject if recipient on any blocklist.
    /// 3. `MerchantAllowlist` — reject if recipient missing from any allowlist.
    /// 4. `TimeWindow` — reject if outside any window.
    /// 5. `CategoryLimit` — reject if the category spend would exceed the cap.
    /// 6. `ApprovalThreshold` — if amount `>= threshold`, hold pending.
    /// 7. Otherwise `Approved` (and category spend is recorded).
    ///
    /// A wallet with no policy has no restrictions and the transaction is
    /// approved.
    pub fn evaluate_transaction(
        env: Env,
        wallet: Address,
        recipient: Address,
        amount: i128,
        category: Option<Symbol>,
    ) -> EvaluationResult {
        wallet.require_auth();

        if amount <= 0 {
            let reason = RejectionReason::InvalidAmount;
            PolicyEvents::tx_rejected(&env, &wallet, amount, &reason);
            return EvaluationResult::Rejected(reason);
        }

        let policy = match storage::get_policy(&env, &wallet) {
            Some(p) => p,
            None => {
                // No policy => no restrictions.
                PolicyEvents::tx_approved(&env, &wallet, amount);
                return EvaluationResult::Approved;
            }
        };

        let now = env.ledger().timestamp();

        // 1. Blocklists (union): blocklist wins over allowlist.
        for rule in policy.rules.iter() {
            if let MerchantBlocklist(br) = rule {
                if rules::is_blocked(&recipient, &br) {
                    let reason = RejectionReason::MerchantBlocked;
                    PolicyEvents::tx_rejected(&env, &wallet, amount, &reason);
                    return EvaluationResult::Rejected(reason);
                }
            }
        }

        // 2. Allowlists (intersection): must be present in every allowlist.
        for rule in policy.rules.iter() {
            if let MerchantAllowlist(ar) = rule {
                if !rules::is_allowed(&recipient, &ar) {
                    let reason = RejectionReason::MerchantNotAllowed;
                    PolicyEvents::tx_rejected(&env, &wallet, amount, &reason);
                    return EvaluationResult::Rejected(reason);
                }
            }
        }

        // 3. Time windows: must be inside every window.
        for rule in policy.rules.iter() {
            if let TimeWindow(tw) = rule {
                if !rules::is_in_time_window(now, &tw) {
                    let reason = RejectionReason::OutsideTimeWindow;
                    PolicyEvents::tx_rejected(&env, &wallet, amount, &reason);
                    return EvaluationResult::Rejected(reason);
                }
            }
        }

        // 4. Category limits: check every matching category.
        if let Some(ref cat) = category {
            for rule in policy.rules.iter() {
                if let CategoryLimit(cl) = rule {
                    if *cat == cl.category
                        && !rules::is_within_category_limit(&env, &wallet, amount, cat, &cl)
                    {
                        let reason = RejectionReason::CategoryLimitExceeded;
                        PolicyEvents::tx_rejected(&env, &wallet, amount, &reason);
                        return EvaluationResult::Rejected(reason);
                    }
                }
            }
        }

        // 5. Approval threshold: the first threshold rule governs. If the
        //    amount reaches it, hold the transaction pending.
        let mut threshold: Option<ApprovalThresholdRule> = None;
        for rule in policy.rules.iter() {
            if let ApprovalThreshold(at) = rule {
                threshold = Some(at);
                break;
            }
        }

        if let Some(at) = threshold {
            if amount >= at.threshold_amount {
                let pending_id = storage::get_next_pending_id(&env);
                storage::set_next_pending_id(&env, pending_id + 1);

                let pending = PendingTransaction {
                    id: pending_id,
                    wallet: wallet.clone(),
                    recipient: recipient.clone(),
                    amount,
                    category: category.clone(),
                    created_at: now,
                    policy_version: policy.version,
                    authorized_approvers: at.approvers.clone(),
                    approvers: Vec::new(&env),
                    required_approvals: at.required_approvals,
                };
                storage::set_pending_tx(&env, &pending);

                let mut ids = storage::get_pending_ids_for_wallet(&env, &wallet);
                ids.push_back(pending_id);
                storage::set_pending_ids_for_wallet(&env, &wallet, &ids);

                PolicyEvents::tx_pending(&env, &wallet, pending_id, amount);
                return EvaluationResult::PendingApproval(pending_id);
            }
        }

        // 6. All rules passed and no approval required => approved. Record
        //    category spend for any matching CategoryLimit rule.
        if let Some(ref cat) = category {
            for rule in policy.rules.iter() {
                if let CategoryLimit(cl) = rule {
                    if *cat == cl.category {
                        rules::record_category_spend(&env, &wallet, amount, cat, &cl);
                    }
                }
            }
        }

        PolicyEvents::tx_approved(&env, &wallet, amount);
        EvaluationResult::Approved
    }

    // -----------------------------------------------------------------------
    // submit_approval
    // -----------------------------------------------------------------------

    /// Submits an approval for a pending (above-threshold) transaction.
    ///
    /// Once the number of distinct approvals reaches `required_approvals` the
    /// transaction is **auto-released**: the category spend is recorded (after
    /// a final re-check of any matching `CategoryLimit`) and the pending
    /// record is removed.
    ///
    /// # Panics
    /// - `PendingTxNotFound` if `pending_id` does not exist.
    /// - `PendingTxExpired` if the policy was replaced since the transaction
    ///   was created.
    /// - `NotAnApprover` if the caller is not in the authorised approver set.
    /// - `AlreadyApproved` if the caller has already approved.
    /// - `CategoryLimitExceeded` if, at release time, the category limit would
    ///   be exceeded.
    pub fn submit_approval(env: Env, approver: Address, pending_id: u64) -> ApprovalOutcome {
        approver.require_auth();

        let mut pending = storage::get_pending_tx(&env, pending_id)
            .unwrap_or_else(|| panic_with_error!(&env, SpendingPolicyError::PendingTxNotFound));

        let policy = storage::get_policy(&env, &pending.wallet)
            .unwrap_or_else(|| panic_with_error!(&env, SpendingPolicyError::PolicyNotFound));

        // Policy replacement invalidates pending transactions from old versions.
        if pending.policy_version != policy.version {
            panic_with_error!(&env, SpendingPolicyError::PendingTxExpired);
        }

        // The caller must be in the authorised approver set.
        if !pending.authorized_approvers.contains(&approver) {
            panic_with_error!(&env, SpendingPolicyError::NotAnApprover);
        }

        // No double-approval.
        if pending.approvers.contains(&approver) {
            panic_with_error!(&env, SpendingPolicyError::AlreadyApproved);
        }

        pending.approvers.push_back(approver.clone());
        let count = pending.approvers.len();

        if count >= pending.required_approvals {
            // Auto-release. Re-check category limits before recording spend.
            if let Some(ref cat) = pending.category {
                for rule in policy.rules.iter() {
                    if let CategoryLimit(cl) = rule {
                        if *cat == cl.category
                            && !rules::is_within_category_limit(
                                &env,
                                &pending.wallet,
                                pending.amount,
                                cat,
                                &cl,
                            )
                        {
                            // Release fails: clean up the pending record and fail.
                            storage::remove_pending_tx(&env, pending_id);
                            storage::remove_pending_id_from_wallet(
                                &env,
                                &pending.wallet,
                                pending_id,
                            );
                            panic_with_error!(&env, SpendingPolicyError::CategoryLimitExceeded);
                        }
                    }
                }
            }

            // Record category spend for matching rules.
            if let Some(ref cat) = pending.category {
                for rule in policy.rules.iter() {
                    if let CategoryLimit(cl) = rule {
                        if *cat == cl.category {
                            rules::record_category_spend(
                                &env,
                                &pending.wallet,
                                pending.amount,
                                cat,
                                &cl,
                            );
                        }
                    }
                }
            }

            storage::remove_pending_tx(&env, pending_id);
            storage::remove_pending_id_from_wallet(&env, &pending.wallet, pending_id);

            PolicyEvents::pending_released(&env, &pending.wallet, pending_id);
            PolicyEvents::tx_approved(&env, &pending.wallet, pending.amount);
            ApprovalOutcome::Approved
        } else {
            storage::set_pending_tx(&env, &pending);
            PolicyEvents::approval_submitted(&env, &pending.wallet, pending_id, &approver);
            ApprovalOutcome::Pending(count)
        }
    }

    // -----------------------------------------------------------------------
    // Read-only accessors
    // -----------------------------------------------------------------------

    /// Returns the current policy for `wallet`, or `None` if none is set.
    pub fn get_policy(env: Env, wallet: Address) -> Option<Policy> {
        storage::get_policy(&env, &wallet)
    }

    /// Returns a pending transaction by id (if it still exists).
    pub fn get_pending_transaction(env: Env, pending_id: u64) -> Option<PendingTransaction> {
        storage::get_pending_tx(&env, pending_id)
    }

    /// Returns the list of pending transaction ids currently held for `wallet`.
    pub fn get_pending_ids_for_wallet(env: Env, wallet: Address) -> Vec<u64> {
        storage::get_pending_ids_for_wallet(&env, &wallet)
    }

    /// Returns the recorded spend for `(wallet, category, period_id)`.
    ///
    /// `period_id` should be computed as `floor(ledger_timestamp / period_seconds)`
    /// for the relevant `CategoryLimitRule`.
    pub fn get_category_spending(
        env: Env,
        wallet: Address,
        category: Symbol,
        period_id: u64,
    ) -> i128 {
        storage::get_category_spending(&env, &wallet, &category, period_id)
    }
}

#[cfg(test)]
mod test;

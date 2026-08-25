//! Core evaluation logic for the spending-rules composition engine.
//!
//! This module is deliberately free of any storage or cross-contract access.
//! It answers the two pure questions the engine exists to ask:
//!
//! 1. Does this amount exceed the rule's ZK-required threshold?
//! 2. Would this amount, added to what has already been spent this week, fit
//!    under the rule's weekly category cap?
//!
//! Keeping those decisions here (rather than inline in the contract) makes
//! them unit-testable in isolation and keeps the cross-contract orchestration
//! in `lib.rs` readable.

use crate::types::Rule;
use crate::Error;

/// Returns `true` when `amount` exceeds the rule's ZK-required threshold,
/// i.e. the payment must be accompanied by a verified zero-knowledge proof.
pub fn zk_proof_required(rule: &Rule, amount: i128) -> bool {
    amount > rule.zk_required_above
}

/// Checks that `already_spent + amount` stays within the rule's weekly
/// category cap. Returns `Err(Error::CategoryLimitExceeded)` when the cap
/// would be breached (including on arithmetic overflow).
pub fn check_weekly_cap(rule: &Rule, already_spent: i128, amount: i128) -> Result<(), Error> {
    let total = already_spent
        .checked_add(amount)
        .ok_or(Error::CategoryLimitExceeded)?;
    if total > rule.weekly_limit {
        Err(Error::CategoryLimitExceeded)
    } else {
        Ok(())
    }
}

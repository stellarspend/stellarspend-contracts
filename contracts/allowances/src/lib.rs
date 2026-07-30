//! # Allowances Contract
//!
//! Manages recurring spending allowances on Stellar/Soroban.
//!
//! ## Issues resolved
//! - #822 Create Allowance Contract — storage schema + contract scaffold
//! - #823 Add Allowance Creation    — `create_allowance` with event emission
//! - #824 Implement Weekly Allowances  — `Frequency::Weekly` (7-day interval)
//! - #825 Implement Monthly Allowances — `Frequency::Monthly` (30-day interval)
//! - #832 Add Daily Allowances         — `Frequency::Daily` (24-hour interval)
//! - #833 Add Allowance Pause/Resume   — `pause_allowance` / `resume_allowance`
//! - #834 Add Allowance Cancellation   — `cancel_allowance` (already present, confirmed)
//! - #835 Add Allowance Beneficiary Update — `update_beneficiary`
//! - #841/#842 Implement Allowance Renewal — `renew_allowance` (reactivate + reset schedule)
//! - #844 Implement Allowance Balance Queries — `get_allowance_balance`
//! Manages recurring spending allowances on Stellar/Soroban.
//!
//! ## Issues resolved
//! - #822 Create Allowance Contract — storage schema + contract scaffold
//! - #823 Add Allowance Creation    — `create_allowance` with event emission
//! - #824 Implement Weekly Allowances  — `Frequency::Weekly` (7-day interval)
//! - #825 Implement Monthly Allowances — `Frequency::Monthly` (30-day interval)
//! - #832 Add Daily Allowances         — `Frequency::Daily` (24-hour interval)
//! - #833 Add Allowance Pause/Resume   — `pause_allowance` / `resume_allowance`
//! - #834 Add Allowance Cancellation   — `cancel_allowance` (already present, confirmed)
//! - #835 Add Allowance Beneficiary Update — `update_beneficiary`
//! - #841/#842 Implement Allowance Renewal — `renew_allowance` (reactivate + reset schedule)
//! - #844 Implement Allowance Balance Queries — `get_allowance_balance
#![no_std]

mod types;

#[cfg(test)]
mod test;

use soroban_sdk::{
    contract, contractimpl, panic_with_error, symbol_short, token, Address, Env, Vec,
};

use types::{Allowance, AllowanceError, DataKey, Frequency};

#[contract]
pub struct AllowancesContract;

#[contractimpl]
impl AllowancesContract {
    // ── Creation ──────────────────────────────────────────────────────────

    pub fn create_allowance(
        env: Env,
        owner: Address,
        recipient: Address,
        token: Address,
        amount: i128,
        frequency: Frequency,
        start_time: u64,
    ) -> u64 {
        owner.require_auth();

        if amount <= 0 {
            panic_with_error!(&env, AllowanceError::InvalidAmount);
        }

        let mut count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::AllowanceCount)
            .unwrap_or(0);
        count += 1;

        // Large allowances require approval before they become active (#845).
        // When no threshold is configured, every allowance is active on

        let requires_approval = match env
            .storage()
            .instance()
            .get::<DataKey, i128>(&DataKey::ApprovalThreshold)
        {
            Some(threshold) => amount > threshold,
            None => false,
        };

        let allowance = Allowance {
            owner: owner.clone(),
            recipient: recipient.clone(),
            token,
            amount,
            frequency: frequency.clone(),
            next_distribution: start_time,
            distribution_count: 0,
            active: !requires_approval,
            paused: false,
            pending_approval: requires_approval,
            spending_limit: 0, // unlimited until an owner sets one (#836)
            end_date: 0,       // never expires until an owner sets an end date (#839)
        };

        save_allowance(&env, count, &allowance);
        env.storage()
            .instance()
            .set(&DataKey::AllowanceCount, &count);

        append_index(&env, DataKey::OwnerAllowances(owner.clone()), count);
        append_index(&env, DataKey::RecipientAllowances(recipient.clone()), count);

        let freq_tag = match &frequency {
            Frequency::Once => symbol_short!("once"),
            Frequency::Daily => symbol_short!("daily"),
            Frequency::Weekly => symbol_short!("weekly"),
            Frequency::Monthly => symbol_short!("monthly"),
        };
        env.events().publish(
            (symbol_short!("allow"), symbol_short!("created"), count),
            (owner, recipient, amount, freq_tag),
        );

        count
    }

    // ── Distribution ──────────────────────────────────────────────────────

    pub fn distribute(env: Env, allowance_id: u64) {
        let mut allowance = load_allowance(&env, allowance_id);

        if allowance.pending_approval {
            panic_with_error!(&env, AllowanceError::ApprovalRequired);
        }
        if !allowance.active {
            panic_with_error!(&env, AllowanceError::AlreadyInactive);
        }
        if allowance.paused {
            panic_with_error!(&env, AllowanceError::Paused);
        }

        let now = env.ledger().timestamp();

        // Past the end date the allowance is expired and distributions stop
        // automatically (#839). `0` means no expiry.
        if allowance.end_date != 0 && now >= allowance.end_date {
            panic_with_error!(&env, AllowanceError::Expired);
        }

        if now < allowance.next_distribution {
            panic_with_error!(&env, AllowanceError::TooEarlyToDistribute);
        }

        // Enforce the cumulative spending cap (#836). `0` means unlimited.
        if allowance.spending_limit > 0 {
            let projected = allowance
                .amount
                .checked_mul((allowance.distribution_count + 1) as i128)
                .unwrap_or_else(|| panic_with_error!(&env, AllowanceError::SpendingLimitExceeded));
            if projected > allowance.spending_limit {
                panic_with_error!(&env, AllowanceError::SpendingLimitExceeded);
            }
        }

        let token_client = token::Client::new(&env, &allowance.token);
        if token_client.balance(&allowance.owner) < allowance.amount {
            panic_with_error!(&env, AllowanceError::InsufficientBalance);
        }

        token_client.transfer_from(
            &env.current_contract_address(),
            &allowance.owner,
            &allowance.recipient,
            &allowance.amount,
        );

        allowance.distribution_count += 1;

        // Append to the allowance's payment history (#837): amount, timestamp,
        // and the recipient at the time of this payment.
        let mut history: Vec<PaymentRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::AllowanceHistory(allowance_id))
            .unwrap_or(Vec::new(&env));
        history.push_back(PaymentRecord {
            amount: allowance.amount,
            timestamp: now,
            recipient: allowance.recipient.clone(),
        });
        env.storage()
            .persistent()
            .set(&DataKey::AllowanceHistory(allowance_id), &history);

        match allowance.frequency.interval_seconds() {
            None => {
                allowance.active = false;
                allowance.next_distribution = 0;
            }
            Some(interval) => {
                allowance.next_distribution += interval;
                if allowance.next_distribution <= now {
                    let missed = (now - allowance.next_distribution) / interval;
                    allowance.next_distribution += (missed + 1) * interval;
                }
            }
        }

        save_allowance(&env, allowance_id, &allowance);
        env.storage()
            .persistent()
            .set(&DataKey::Allowance(allowance_id), &allowance);

        // Dedicated payment event for off-chain indexers (#838): a stable
        // `("allow", "payment", allowance_id)` topic carrying (recipient, amount)
        // is emitted on every payment, alongside the richer `distrib` event.
        env.events().publish(
            (
                symbol_short!("allow"),
                symbol_short!("payment"),
                allowance_id,
            ),
            (allowance.recipient.clone(), allowance.amount),
        );
        env.events().publish(
            (
                symbol_short!("allow"),
                symbol_short!("distrib"),
                allowance_id,
            ),
            (
                allowance.recipient,
                allowance.amount,
                allowance.next_distribution,
            ),
        );
    }

    // ── Pause / Resume (#833) ─────────────────────────────────────────────

    /// Temporarily suspends distributions. Only the owner may pause.
    pub fn pause_allowance(env: Env, allowance_id: u64) {
        let mut allowance = load_allowance(&env, allowance_id);

        allowance.owner.require_auth();
        if !allowance.active {
            panic_with_error!(&env, AllowanceError::AlreadyInactive);
        }
        if allowance.paused {
            panic_with_error!(&env, AllowanceError::AlreadyPaused);
        }

        allowance.paused = true;
        save_allowance(&env, allowance_id, &allowance);
        env.events().publish(
            (
                symbol_short!("allow"),
                symbol_short!("paused"),
                allowance_id,
            ),
            allowance.owner,
        );
    }

    /// Resumes a paused allowance. Only the owner may resume.
    pub fn resume_allowance(env: Env, allowance_id: u64) {
        let mut allowance = load_allowance(&env, allowance_id);

        allowance.owner.require_auth();
        if !allowance.active {
            panic_with_error!(&env, AllowanceError::AlreadyInactive);
        }
        if !allowance.paused {
            panic_with_error!(&env, AllowanceError::NotPaused);
        }

        allowance.paused = false;
        save_allowance(&env, allowance_id, &allowance);
        env.events().publish(
            (
                symbol_short!("allow"),
                symbol_short!("resumed"),
                allowance_id,
            ),
            allowance.owner,
        );
    }

    // ── Cancellation (#834) ───────────────────────────────────────────────

    /// Permanently cancels an allowance. Only the owner may cancel.
    pub fn cancel_allowance(env: Env, allowance_id: u64) {
        let mut allowance = load_allowance(&env, allowance_id);

        allowance.owner.require_auth();
        if !allowance.active {
            panic_with_error!(&env, AllowanceError::AlreadyInactive);
        }

        allowance.active = false;
        save_allowance(&env, allowance_id, &allowance);
        env.events().publish(
            (
                symbol_short!("allow"),
                symbol_short!("canceled"),
                allowance_id,
            ),
            allowance.owner,
        );
    }

    // ── Beneficiary update (#835) ─────────────────────────────────────────

    /// Updates the recipient of an active allowance. Only the owner may call.
    /// Future distributions go to `new_recipient`; history is preserved.
    pub fn update_beneficiary(env: Env, allowance_id: u64, new_recipient: Address) {
        let mut allowance = load_allowance(&env, allowance_id);

        allowance.owner.require_auth();
        if !allowance.active {
            panic_with_error!(&env, AllowanceError::AlreadyInactive);
        }

        let old_recipient = allowance.recipient.clone();
        allowance.recipient = new_recipient.clone();
        save_allowance(&env, allowance_id, &allowance);

        // Update recipient index for new beneficiary
        append_index(
            &env,
            DataKey::RecipientAllowances(new_recipient.clone()),
            allowance_id,
        );

        env.events().publish(
            (
                symbol_short!("allow"),
                symbol_short!("ben_upd"),
                allowance_id,
            ),
            (old_recipient, new_recipient),
        );
    }

    // ── Renewal (#841/#842) ───────────────────────────────────────────────

    /// Renews an inactive (expired/cancelled) allowance, preserving its
    /// configuration (recipient, token, amount, frequency, distribution_count).
    /// Reactivates it and resets the schedule to `start_time`. Only the owner may renew. (#841/#842)
    pub fn renew_allowance(env: Env, allowance_id: u64, start_time: u64) {
        let mut allowance: Allowance = env
            .storage()
            .persistent()
            .get(&DataKey::Allowance(allowance_id))
            .unwrap_or_else(|| panic_with_error!(&env, AllowanceError::NotFound));

        allowance.owner.require_auth();

        if allowance.active {
            panic_with_error!(&env, AllowanceError::StillActive);
        }

        allowance.active = true;
        allowance.paused = false;
        allowance.next_distribution = start_time;

        let owner = allowance.owner.clone();
        env.storage()
            .persistent()
            .set(&DataKey::Allowance(allowance_id), &allowance);
        env.events().publish(
            (
                symbol_short!("allow"),
                symbol_short!("renewed"),
                allowance_id,
            ),
            (owner, start_time),
        );
    }

    // ── Balance query (#844) ──────────────────────────────────────────────

    /// Returns the funds currently backing this allowance — i.e. the owner's
    /// spendable balance of the allowance's token, which is the source distributions
    /// are paid from. Reflects the real amount available for future distributions. (#844)
    pub fn get_allowance_balance(env: Env, allowance_id: u64) -> i128 {
        let allowance: Allowance = env
            .storage()
            .persistent()
            .get(&DataKey::Allowance(allowance_id))
            .unwrap_or_else(|| panic_with_error!(&env, AllowanceError::NotFound));

        token::Client::new(&env, &allowance.token).balance(&allowance.owner)
    }

    // ── Queries ───────────────────────────────────────────────────────────

    pub fn get_allowance(env: Env, allowance_id: u64) -> Allowance {
        load_allowance(&env, allowance_id)
    }

    /// Returns usage analytics for an allowance (#846): total amount
    /// distributed, the average payment, and the owner's remaining spendable
    /// balance in the allowance token.
    pub fn get_allowance_analytics(env: Env, allowance_id: u64) -> AllowanceAnalytics {
        let allowance: Allowance = env
            .storage()
            .persistent()
            .get(&DataKey::Allowance(allowance_id))
            .unwrap_or_else(|| panic_with_error!(&env, AllowanceError::NotFound));

        let count = allowance.distribution_count as i128;
        let total_distributed = allowance.amount.saturating_mul(count);
        let average_payment = if count == 0 {
            0
        } else {
            total_distributed / count
        };
        let remaining = token::Client::new(&env, &allowance.token).balance(&allowance.owner);

        AllowanceAnalytics {
            total_distributed,
            distribution_count: allowance.distribution_count,
            average_payment,
            remaining,
        }
    }

    pub fn get_owner_allowances(env: Env, owner: Address) -> Vec<u64> {
        env.storage()
            .persistent()
            .get(&DataKey::OwnerAllowances(owner))
            .unwrap_or(Vec::new(&env))
    }

    /// Returns the full payment history for an allowance (#837), oldest first.
    /// Empty if no distributions have occurred (or the allowance does not exist).
    pub fn get_allowance_history(env: Env, allowance_id: u64) -> Vec<PaymentRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::AllowanceHistory(allowance_id))
            .unwrap_or(Vec::new(&env))
    }

    pub fn get_recipient_allowances(env: Env, recipient: Address) -> Vec<u64> {
        env.storage()
            .persistent()
            .get(&DataKey::RecipientAllowances(recipient))
            .unwrap_or(Vec::new(&env))
    }

    pub fn allowance_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::AllowanceCount)
            .unwrap_or(0)
    }
}

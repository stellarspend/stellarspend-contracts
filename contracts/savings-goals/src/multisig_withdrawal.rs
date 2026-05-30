//! Multi-signature withdrawal guard for savings goals.
//!
//! Requires a configurable number of approvals before a withdrawal is executed.

#![no_std]

use soroban_sdk::{contracttype, panic_with_error, symbol_short, Address, Env, Vec};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum MultisigError {
    AlreadyApproved = 1,
    ThresholdNotMet = 2,
    InvalidThreshold = 3,
}

impl From<MultisigError> for soroban_sdk::Error {
    fn from(e: MultisigError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

#[contracttype]
#[derive(Clone)]
enum MsKey { Approvals(u64), Threshold(u64) }

/// Sets the required approval threshold for a goal.
pub fn set_threshold(env: &Env, goal_id: u64, threshold: u32) {
    if threshold == 0 { panic_with_error!(env, MultisigError::InvalidThreshold); }
    env.storage().persistent().set(&MsKey::Threshold(goal_id), &threshold);
}

/// Records an approval from `approver` for the given goal withdrawal.
pub fn approve(env: &Env, approver: Address, goal_id: u64) {
    approver.require_auth();
    let mut approvals: Vec<Address> = env.storage().persistent()
        .get(&MsKey::Approvals(goal_id)).unwrap_or(Vec::new(env));
    if approvals.contains(&approver) { panic_with_error!(env, MultisigError::AlreadyApproved); }
    approvals.push_back(approver);
    env.storage().persistent().set(&MsKey::Approvals(goal_id), &approvals);
    env.events().publish((symbol_short!("multisig"), symbol_short!("approved")), goal_id);
}

/// Returns true if the approval count meets the threshold.
pub fn is_approved(env: &Env, goal_id: u64) -> bool {
    let threshold: u32 = env.storage().persistent().get(&MsKey::Threshold(goal_id)).unwrap_or(1);
    let approvals: Vec<Address> = env.storage().persistent()
        .get(&MsKey::Approvals(goal_id)).unwrap_or(Vec::new(env));
    approvals.len() >= threshold
}
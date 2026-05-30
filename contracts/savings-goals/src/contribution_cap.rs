//! Goal contribution cap enforcement.
//!
//! Prevents users from depositing more than the target amount into a savings goal.

#![no_std]

use soroban_sdk::{contracttype, panic_with_error, Address, Env, Symbol};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum CapError {
    GoalNotFound = 1,
    ExceedsTarget = 2,
    InvalidAmount = 3,
}

impl From<CapError> for soroban_sdk::Error {
    fn from(e: CapError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

/// Returns the maximum amount a user may still contribute to reach the target.
///
/// Returns 0 if the goal is already fully funded.
pub fn remaining_capacity(current_amount: i128, target_amount: i128) -> i128 {
    if current_amount >= target_amount {
        0
    } else {
        target_amount - current_amount
    }
}

/// Validates that `deposit` does not push `current_amount` past `target_amount`.
///
/// Returns `Err(CapError::ExceedsTarget)` when the deposit would overfund the goal.
pub fn validate_contribution(env: &Env, current_amount: i128, target_amount: i128, deposit: i128) {
    if deposit <= 0 {
        panic_with_error!(env, CapError::InvalidAmount);
    }
    if current_amount + deposit > target_amount {
        panic_with_error!(env, CapError::ExceedsTarget);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    #[test]
    fn remaining_capacity_returns_zero_when_full() {
        assert_eq!(remaining_capacity(1000, 1000), 0);
        assert_eq!(remaining_capacity(1500, 1000), 0);
    }

    #[test]
    fn remaining_capacity_returns_gap() {
        assert_eq!(remaining_capacity(300, 1000), 700);
    }
}
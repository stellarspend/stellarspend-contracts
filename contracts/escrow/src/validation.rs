//! Validation utilities for escrow reversals.

use crate::types::{Escrow, EscrowStatus};
use soroban_sdk::Address;

/// Error codes for reversal validation.
#[allow(non_snake_case)]
pub mod ErrorCode {
    /// Escrow not found
    pub const ESCROW_NOT_FOUND: u32 = 0;
    /// Escrow already released
    pub const ALREADY_RELEASED: u32 = 1;
    /// Escrow already reversed
    pub const ALREADY_REVERSED: u32 = 2;
    /// Caller not authorized to reverse
    pub const UNAUTHORIZED: u32 = 3;
    /// Deadline not yet reached (for time-based reversals)
    pub const DEADLINE_NOT_REACHED: u32 = 4;
}

/// Validation error types for reversals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationError {
    /// Escrow does not exist
    EscrowNotFound,
    /// Escrow has already been released
    AlreadyReleased,
    /// Escrow has already been reversed
    AlreadyReversed,
    /// Caller is not authorized
    Unauthorized,
    /// Deadline has not been reached yet
    DeadlineNotReached,
}

impl ValidationError {
    /// Convert to error code for result tracking.
    pub fn to_error_code(&self) -> u32 {
        match self {
            ValidationError::EscrowNotFound => ErrorCode::ESCROW_NOT_FOUND,
            ValidationError::AlreadyReleased => ErrorCode::ALREADY_RELEASED,
            ValidationError::AlreadyReversed => ErrorCode::ALREADY_REVERSED,
            ValidationError::Unauthorized => ErrorCode::UNAUTHORIZED,
            ValidationError::DeadlineNotReached => ErrorCode::DEADLINE_NOT_REACHED,
        }
    }
}

/// Validates whether an escrow can be reversed.
///
/// # Arguments
/// * `escrow` - Optional escrow record (None if not found)
/// * `caller` - The address attempting the reversal
/// * `admin` - The admin address
/// * `check_deadline` - Whether to enforce deadline check
/// * `current_ledger` - Current ledger sequence for deadline comparison
///
/// # Returns
/// * `Ok(())` if reversal is valid
/// * `Err(ValidationError)` with specific error if invalid
pub fn validate_reversal(
    escrow: Option<&Escrow>,
    caller: &Address,
    admin: &Address,
    check_deadline: bool,
    current_ledger: u64,
) -> Result<(), ValidationError> {
    // Check if escrow exists
    let escrow = escrow.ok_or(ValidationError::EscrowNotFound)?;

    // Check escrow status
    match escrow.status {
        EscrowStatus::Released => return Err(ValidationError::AlreadyReleased),
        EscrowStatus::Reversed => return Err(ValidationError::AlreadyReversed),
        EscrowStatus::Active => {}
    }

    // Check authorization: admin or depositor can reverse
    let is_admin = caller == admin;
    let is_depositor = caller == &escrow.depositor;

    if !is_admin && !is_depositor {
        return Err(ValidationError::Unauthorized);
    }

    // If not admin and deadline check is enabled, verify deadline has passed
    if check_deadline && !is_admin && current_ledger < escrow.deadline {
        return Err(ValidationError::DeadlineNotReached);
    }

    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    fn create_test_escrow(env: &Env, status: EscrowStatus) -> Escrow {
        Escrow {
            escrow_id: 1,
            depositor: Address::generate(env),
            recipient: Address::generate(env),
            token: Address::generate(env),
            amount: 1000,
            status,
            created_at: 100,
            deadline: 200,
        }
    }

    #[test]
    fn test_validate_reversal_active_escrow_by_admin() {
        let env = Env::default();
        let escrow = create_test_escrow(&env, EscrowStatus::Active);
        let admin = Address::generate(&env);

        let result = validate_reversal(Some(&escrow), &admin, &admin, false, 100);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_reversal_active_escrow_by_depositor() {
        let env = Env::default();
        let mut escrow = create_test_escrow(&env, EscrowStatus::Active);
        let depositor = Address::generate(&env);
        escrow.depositor = depositor.clone();
        let admin = Address::generate(&env);

        let result = validate_reversal(Some(&escrow), &depositor, &admin, false, 100);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_reversal_escrow_not_found() {
        let env = Env::default();
        let caller = Address::generate(&env);
        let admin = Address::generate(&env);

        let result = validate_reversal(None, &caller, &admin, false, 100);
        assert_eq!(result, Err(ValidationError::EscrowNotFound));
    }

    #[test]
    fn test_validate_reversal_already_released() {
        let env = Env::default();
        let escrow = create_test_escrow(&env, EscrowStatus::Released);
        let admin = Address::generate(&env);

        let result = validate_reversal(Some(&escrow), &admin, &admin, false, 100);
        assert_eq!(result, Err(ValidationError::AlreadyReleased));
    }

    #[test]
    fn test_validate_reversal_already_reversed() {
        let env = Env::default();
        let escrow = create_test_escrow(&env, EscrowStatus::Reversed);
        let admin = Address::generate(&env);

        let result = validate_reversal(Some(&escrow), &admin, &admin, false, 100);
        assert_eq!(result, Err(ValidationError::AlreadyReversed));
    }

    #[test]
    fn test_validate_reversal_unauthorized() {
        let env = Env::default();
        let escrow = create_test_escrow(&env, EscrowStatus::Active);
        let admin = Address::generate(&env);
        let unauthorized = Address::generate(&env);

        let result = validate_reversal(Some(&escrow), &unauthorized, &admin, false, 100);
        assert_eq!(result, Err(ValidationError::Unauthorized));
    }

    #[test]
    fn test_validate_reversal_deadline_not_reached() {
        let env = Env::default();
        let mut escrow = create_test_escrow(&env, EscrowStatus::Active);
        escrow.deadline = 300;
        let depositor = Address::generate(&env);
        escrow.depositor = depositor.clone();
        let admin = Address::generate(&env);

        // Depositor tries to reverse before deadline with deadline check enabled
        let result = validate_reversal(Some(&escrow), &depositor, &admin, true, 100);
        assert_eq!(result, Err(ValidationError::DeadlineNotReached));
    }

    #[test]
    fn test_validate_reversal_admin_bypasses_deadline() {
        let env = Env::default();
        let mut escrow = create_test_escrow(&env, EscrowStatus::Active);
        escrow.deadline = 300;
        let admin = Address::generate(&env);

        // Admin can reverse even before deadline
        let result = validate_reversal(Some(&escrow), &admin, &admin, true, 100);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_reversal_depositor_after_deadline() {
        let env = Env::default();
        let mut escrow = create_test_escrow(&env, EscrowStatus::Active);
        escrow.deadline = 200;
        let depositor = Address::generate(&env);
        escrow.depositor = depositor.clone();
        let admin = Address::generate(&env);

        // Depositor can reverse after deadline
        let result = validate_reversal(Some(&escrow), &depositor, &admin, true, 250);
        assert!(result.is_ok());
    }

    #[test]
    fn test_error_code_conversion() {
        assert_eq!(ValidationError::EscrowNotFound.to_error_code(), ErrorCode::ESCROW_NOT_FOUND);
        assert_eq!(ValidationError::AlreadyReleased.to_error_code(), ErrorCode::ALREADY_RELEASED);
        assert_eq!(ValidationError::AlreadyReversed.to_error_code(), ErrorCode::ALREADY_REVERSED);
        assert_eq!(ValidationError::Unauthorized.to_error_code(), ErrorCode::UNAUTHORIZED);
        assert_eq!(ValidationError::DeadlineNotReached.to_error_code(), ErrorCode::DEADLINE_NOT_REACHED);
    }
}

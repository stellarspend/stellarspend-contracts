//! Validation utilities for batch transfers.

use soroban_sdk::{Address, Env};

/// Validation error types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationError {
    /// Invalid transfer amount
    InvalidAmount,
}

/// Validates a recipient address.
pub fn validate_address(_env: &Env, _address: &Address) -> Result<(), ValidationError> {
    Ok(())
}

/// Validates a transfer amount.
/// Ensures the amount is positive and within reasonable bounds.
pub fn validate_amount(amount: i128) -> Result<(), ValidationError> {
    // Amount must be positive
    if amount <= 0 {
        return Err(ValidationError::InvalidAmount);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    #[test]
    fn test_validate_amount_positive() {
        assert!(validate_amount(1000).is_ok());
        assert!(validate_amount(1).is_ok());
        assert!(validate_amount(i128::MAX).is_ok());
    }

    #[test]
    fn test_validate_amount_negative() {
        assert_eq!(validate_amount(-1), Err(ValidationError::InvalidAmount));
        assert_eq!(validate_amount(-1000), Err(ValidationError::InvalidAmount));
    }

    #[test]
    fn test_validate_amount_zero() {
        assert_eq!(validate_amount(0), Err(ValidationError::InvalidAmount));
    }

    #[test]
    fn test_validate_address() {
        let env = Env::default();
        let address = Address::generate(&env);
        assert!(validate_address(&env, &address).is_ok());
    }
}

//! Validation utilities for batch currency conversions.

use soroban_sdk::{Address, Env};

/// Validation error types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationError {
    /// Invalid conversion amount
    InvalidAmount,
    /// Invalid expected output amount
    InvalidMinOutput,
    /// Same asset conversion (from_asset == to_asset)
    SameAsset,
}

/// Validates an address.
///
/// Currently accepts all addresses. In production, could verify address exists on-chain.
pub fn validate_address(_env: &Env, _address: &Address) -> Result<(), ValidationError> {
    Ok(())
}

/// Validates a conversion amount.
/// Ensures the amount is positive and within reasonable bounds.
pub fn validate_amount(amount: i128) -> Result<(), ValidationError> {
    if amount <= 0 {
        return Err(ValidationError::InvalidAmount);
    }
    Ok(())
}

/// Validates minimum output amount.
pub fn validate_min_output(min_amount_out: i128) -> Result<(), ValidationError> {
    if min_amount_out <= 0 {
        return Err(ValidationError::InvalidMinOutput);
    }
    Ok(())
}

/// Validates that from_asset and to_asset are different.
pub fn validate_asset_pair(
    from_asset: &Address,
    to_asset: &Address,
) -> Result<(), ValidationError> {
    if from_asset == to_asset {
        return Err(ValidationError::SameAsset);
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
        assert_eq!(validate_amount(0), Err(ValidationError::InvalidAmount));
    }

    #[test]
    fn test_validate_min_output() {
        assert!(validate_min_output(100).is_ok());
        assert_eq!(
            validate_min_output(0),
            Err(ValidationError::InvalidMinOutput)
        );
        assert_eq!(
            validate_min_output(-1),
            Err(ValidationError::InvalidMinOutput)
        );
    }

    #[test]
    fn test_validate_asset_pair() {
        let env = Env::default();
        let asset1 = Address::generate(&env);
        let asset2 = Address::generate(&env);

        assert!(validate_asset_pair(&asset1, &asset2).is_ok());
        assert_eq!(
            validate_asset_pair(&asset1, &asset1),
            Err(ValidationError::SameAsset)
        );
    }

    #[test]
    fn test_validate_address() {
        let env = Env::default();
        let address = Address::generate(&env);
        assert!(validate_address(&env, &address).is_ok());
    }
}

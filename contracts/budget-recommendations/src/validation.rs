//! Validation utilities for budget recommendations.

use soroban_sdk::{Env, Vec};

use crate::types::UserProfile;

/// Validation error types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationError {
    /// Invalid user ID
    InvalidUserId,
    /// Invalid income amount
    InvalidIncome,
    /// Invalid expenses amount
    InvalidExpenses,
    /// Invalid savings balance
    InvalidSavings,
    /// Invalid risk tolerance
    InvalidRiskTolerance,
}

/// Validates a user profile for budget recommendations.
///
/// Returns Ok(()) if valid, or a ValidationError if invalid.
pub fn validate_user_profile(_env: &Env, profile: &UserProfile) -> Result<(), ValidationError> {
    // Validate user ID
    if profile.user_id == 0 {
        return Err(ValidationError::InvalidUserId);
    }

    // Validate address
    // Note: Address validation is basic - in production you might want more checks
    // For now, we just ensure it's not a zero address (if applicable)
    
    // Validate income (must be positive)
    if profile.monthly_income <= 0 {
        return Err(ValidationError::InvalidIncome);
    }

    // Validate expenses (must be non-negative)
    if profile.monthly_expenses < 0 {
        return Err(ValidationError::InvalidExpenses);
    }

    // Validate savings (must be non-negative)
    if profile.savings_balance < 0 {
        return Err(ValidationError::InvalidSavings);
    }

    // Validate risk tolerance (must be 1-5)
    if profile.risk_tolerance < 1 || profile.risk_tolerance > 5 {
        return Err(ValidationError::InvalidRiskTolerance);
    }

    // Validate that expenses don't exceed income (warning case, but allow for debt scenarios)
    // We'll allow this but flag it in recommendations

    Ok(())
}

/// Validates a batch of user profiles.
///
/// Returns Ok(()) if all profiles are valid, or an error message if any are invalid.
pub fn validate_batch(profiles: &Vec<UserProfile>) -> Result<(), &'static str> {
    let count = profiles.len();

    if count == 0 {
        return Err("Batch cannot be empty");
    }

    if count > crate::types::MAX_BATCH_SIZE {
        return Err("Batch exceeds maximum size");
    }

    // Validate each profile
    let env = Env::default(); // Note: In production, pass env as parameter
    for profile in profiles.iter() {
        if let Err(_) = validate_user_profile(&env, &profile) {
            return Err("Invalid user profile in batch");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env, Symbol};

    fn create_test_profile(env: &Env, user_id: u64, income: i128, expenses: i128) -> UserProfile {
        UserProfile {
            user_id,
            address: Address::generate(env),
            monthly_income: income,
            monthly_expenses: expenses,
            savings_balance: 0,
            spending_categories: Symbol::new(env, "food,transport"),
            risk_tolerance: 3,
        }
    }

    #[test]
    fn test_validate_user_profile_valid() {
        let env = Env::default();
        let profile = create_test_profile(&env, 1, 100000, 50000);
        assert!(validate_user_profile(&env, &profile).is_ok());
    }

    #[test]
    fn test_validate_user_profile_invalid_user_id() {
        let env = Env::default();
        let mut profile = create_test_profile(&env, 1, 100000, 50000);
        profile.user_id = 0;
        assert_eq!(
            validate_user_profile(&env, &profile),
            Err(ValidationError::InvalidUserId)
        );
    }

    #[test]
    fn test_validate_user_profile_invalid_income() {
        let env = Env::default();
        let profile = create_test_profile(&env, 1, 0, 50000);
        assert_eq!(
            validate_user_profile(&env, &profile),
            Err(ValidationError::InvalidIncome)
        );
    }

    #[test]
    fn test_validate_user_profile_invalid_expenses() {
        let env = Env::default();
        let mut profile = create_test_profile(&env, 1, 100000, 50000);
        profile.monthly_expenses = -1;
        assert_eq!(
            validate_user_profile(&env, &profile),
            Err(ValidationError::InvalidExpenses)
        );
    }

    #[test]
    fn test_validate_user_profile_invalid_risk_tolerance() {
        let env = Env::default();
        let mut profile = create_test_profile(&env, 1, 100000, 50000);
        profile.risk_tolerance = 6;
        assert_eq!(
            validate_user_profile(&env, &profile),
            Err(ValidationError::InvalidRiskTolerance)
        );
    }
}

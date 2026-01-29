//! Validation logic for spending limit update requests.

use soroban_sdk::Address;

use crate::types::{ErrorCode, SpendingLimitRequest, MAX_SPENDING_LIMIT, MIN_SPENDING_LIMIT};

/// Validates a spending limit update request.
///
/// # Returns
/// * `Ok(())` if valid
/// * `Err(error_code)` if invalid
pub fn validate_limit_request(request: &SpendingLimitRequest) -> Result<(), u32> {
    // Validate user address
    if !is_valid_address(&request.user) {
        return Err(ErrorCode::INVALID_USER_ADDRESS);
    }

    // Validate monthly limit amount
    if !is_valid_limit(request.monthly_limit) {
        return Err(ErrorCode::INVALID_LIMIT);
    }

    // Validate category if provided
    // In Soroban, symbols are always valid by construction
    // This check exists for consistency with validation patterns

    Ok(())
}

/// Validates that an address is valid.
///
/// In Soroban, all Address instances are valid by construction,
/// but this function exists for consistency with validation patterns
/// and potential future enhancements.
fn is_valid_address(_address: &Address) -> bool {
    // Address is always valid in Soroban SDK by construction
    true
}

/// Validates that a spending limit is within acceptable bounds.
///
/// # Arguments
/// * `limit` - The spending limit to validate
///
/// # Returns
/// * `true` if limit is >= MIN_SPENDING_LIMIT and <= MAX_SPENDING_LIMIT
pub fn is_valid_limit(limit: i128) -> bool {
    limit >= MIN_SPENDING_LIMIT && limit <= MAX_SPENDING_LIMIT
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, symbol_short, Env};

    fn create_valid_request(env: &Env) -> SpendingLimitRequest {
        SpendingLimitRequest {
            user: Address::generate(env),
            monthly_limit: 100_000_000_000, // 10,000 XLM
            category: Some(symbol_short!("general")),
        }
    }

    #[test]
    fn test_valid_limit_request() {
        let env = Env::default();
        let request = create_valid_request(&env);
        assert!(validate_limit_request(&request).is_ok());
    }

    #[test]
    fn test_invalid_limit_too_low() {
        let env = Env::default();
        let mut request = create_valid_request(&env);
        request.monthly_limit = 100; // Below minimum
        assert_eq!(
            validate_limit_request(&request),
            Err(ErrorCode::INVALID_LIMIT)
        );
    }

    #[test]
    fn test_invalid_limit_negative() {
        let env = Env::default();
        let mut request = create_valid_request(&env);
        request.monthly_limit = -1000;
        assert_eq!(
            validate_limit_request(&request),
            Err(ErrorCode::INVALID_LIMIT)
        );
    }

    #[test]
    fn test_invalid_limit_too_high() {
        let env = Env::default();
        let mut request = create_valid_request(&env);
        request.monthly_limit = MAX_SPENDING_LIMIT + 1;
        assert_eq!(
            validate_limit_request(&request),
            Err(ErrorCode::INVALID_LIMIT)
        );
    }

    #[test]
    fn test_is_valid_limit() {
        assert!(is_valid_limit(MIN_SPENDING_LIMIT));
        assert!(is_valid_limit(MAX_SPENDING_LIMIT));
        assert!(is_valid_limit(100_000_000_000));
        assert!(!is_valid_limit(MIN_SPENDING_LIMIT - 1));
        assert!(!is_valid_limit(MAX_SPENDING_LIMIT + 1));
        assert!(!is_valid_limit(-1000));
    }

    #[test]
    fn test_valid_request_without_category() {
        let env = Env::default();
        let mut request = create_valid_request(&env);
        request.category = None;
        assert!(validate_limit_request(&request).is_ok());
    }
}

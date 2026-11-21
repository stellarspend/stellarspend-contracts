
use soroban_sdk::Address;

use crate::{BudgetError, Error, MAX_BUDGET_LIMIT};

/// Validates a budget amount represented as a signed integer.
///
/// The amount must be strictly positive.
///
/// # Errors
///
/// Returns `Error::InvalidAmount` if `amount` is zero or negative.
pub fn validate_amount(amount: i128) -> Result<(), Error> {
    if amount <= 0 {
        return Err(Error::InvalidAmount);
    }

    Ok(())
}

/// Validates that the end date is strictly after the start date.
///
/// # Errors
///
/// Returns `Error::InvalidDateRange` if `end_date` is less than
/// or equal to `start_date`.
pub fn validate_date_range(
    start_date: u64,
    end_date: u64,
) -> Result<(), Error> {
    if end_date <= start_date {
        return Err(Error::InvalidDateRange);
    }

    Ok(())
}

/// Validates a budget amount represented in unsigned atomic units.
///
/// The amount must be greater than zero and no greater than
/// `MAX_BUDGET_LIMIT`.
///
/// # Errors
///
/// Returns `BudgetError::InvalidAmount` if the amount is zero.
/// Returns `BudgetError::ExceedsLimit` if the amount exceeds the limit.
pub fn validate_budget_amount(amount: u64) -> Result<(), BudgetError> {
    if amount == 0 {
        return Err(BudgetError::InvalidAmount);
    }

    if amount > MAX_BUDGET_LIMIT {
        return Err(BudgetError::ExceedsLimit);
    }

    Ok(())
}

/// Validates that the budget's active period has a valid time range.
///
/// # Errors
///
/// Returns `BudgetError::InvalidTimeRange` if `start_time` is
/// greater than or equal to `end_time`.
pub fn validate_time_range(
    start_time: u64,
    end_time: u64,
) -> Result<(), BudgetError> {
    if start_time >= end_time {
        return Err(BudgetError::InvalidTimeRange);
    }

    Ok(())
}

/// Validates that the caller is the designated budget administrator
/// and requires the caller's Soroban authorization.
///
/// # Errors
///
/// Returns `BudgetError::Unauthorized` if the caller is not the
/// designated administrator.
///
/// # Panics
///
/// Soroban authorization enforcement may fail if the caller has not
/// authorized the invocation.
pub fn validate_authorization(
    caller: &Address,
    authorized_admin: &Address,
) -> Result<(), BudgetError> {
    if caller != authorized_admin {
        return Err(BudgetError::Unauthorized);
    }

    caller.require_auth();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::Address as _,
        Env,
    };

    #[test]
    fn signed_amount_rejects_zero_and_negative_values() {
        assert_eq!(validate_amount(0), Err(Error::InvalidAmount));
        assert_eq!(validate_amount(-1), Err(Error::InvalidAmount));
    }

    #[test]
    fn signed_amount_accepts_positive_values() {
        assert_eq!(validate_amount(1), Ok(()));
        assert_eq!(validate_amount(i128::MAX), Ok(()));
    }

    #[test]
    fn date_range_rejects_equal_and_reversed_dates() {
        assert_eq!(
            validate_date_range(100, 100),
            Err(Error::InvalidDateRange)
        );

        assert_eq!(
            validate_date_range(200, 100),
            Err(Error::InvalidDateRange)
        );
    }

    #[test]
    fn date_range_accepts_chronological_dates() {
        assert_eq!(validate_date_range(100, 101), Ok(()));
    }

    #[test]
    fn budget_amount_rejects_zero() {
        assert_eq!(
            validate_budget_amount(0),
            Err(BudgetError::InvalidAmount)
        );
    }

    #[test]
    fn budget_amount_rejects_values_above_limit() {
        if MAX_BUDGET_LIMIT < u64::MAX {
            assert_eq!(
                validate_budget_amount(MAX_BUDGET_LIMIT + 1),
                Err(BudgetError::ExceedsLimit)
            );
        }
    }

    #[test]
    fn budget_amount_accepts_values_within_limit() {
        if MAX_BUDGET_LIMIT > 0 {
            assert_eq!(validate_budget_amount(1), Ok(()));
            assert_eq!(
                validate_budget_amount(MAX_BUDGET_LIMIT),
                Ok(())
            );
        }
    }

    #[test]
    fn time_range_rejects_equal_and_reversed_timestamps() {
        assert_eq!(
            validate_time_range(100, 100),
            Err(BudgetError::InvalidTimeRange)
        );

        assert_eq!(
            validate_time_range(200, 100),
            Err(BudgetError::InvalidTimeRange)
        );
    }

    #[test]
    fn time_range_accepts_valid_timestamps() {
        assert_eq!(validate_time_range(100, 101), Ok(()));
    }

    #[test]
    fn authorization_rejects_a_different_address() {
        let env = Env::default();
        let caller = Address::generate(&env);
        let admin = Address::generate(&env);

        assert_eq!(
            validate_authorization(&caller, &admin),
            Err(BudgetError::Unauthorized)
        );
    }

    #[test]
    fn authorization_accepts_the_admin_with_mocked_auth() {
        use crate::Contract;

        let env = Env::default();
        let admin = Address::generate(&env);

        let contract_id = env.register(Contract, ());
        env.mock_all_auths();

        env.as_contract(&contract_id, || {
            assert_eq!(validate_authorization(&admin, &admin), Ok(()));
        });
    }
}

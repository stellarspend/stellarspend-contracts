//! # Compliance validation
//!
//! Input validation helpers used by the compliance contract, together with the
//! error type surfaced when a value fails a check.

use soroban_sdk::contracterror;

/// Errors raised by validation.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// Amount must be non-negative.
    InvalidAmount = 1,
    /// Contract has not been initialized.
    NotInitialized = 2,
}

/// Validates a financial amount.
///
/// # Errors
///
/// Returns [`Error::InvalidAmount`] when `amount` is negative.
pub fn validate_amount(amount: i128) -> Result<(), Error> {
    if amount < 0 {
        Err(Error::InvalidAmount)
    } else {
        Ok(())
    }
}

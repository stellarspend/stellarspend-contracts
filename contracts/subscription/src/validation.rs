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
/// Returns [`Error::InvalidAmount`] if `amount` is negative.
pub fn validate_amount(amount: i128) -> Result<(), Error> {
    if amount < 0 {
        Err(Error::InvalidAmount)
    } else {
        Ok(())
    }
}

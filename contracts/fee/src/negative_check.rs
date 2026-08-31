use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum NegativeAmountError {
    NegativeAmount = 1,
}

/// Validates that `amount` is zero or positive, returning a negative-amount error when it is below zero.
pub fn check_not_negative(amount: i128) -> Result<(), NegativeAmountError> {
    if amount < 0 {
        return Err(NegativeAmountError::NegativeAmount);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_positive_amount() {
        assert!(check_not_negative(100).is_ok());
    }

    #[test]
    fn test_zero_amount() {
        assert!(check_not_negative(0).is_ok());
    }

    #[test]
    fn test_negative_amount() {
        assert_eq!(check_not_negative(-1), Err(NegativeAmountError::NegativeAmount));
    }
}

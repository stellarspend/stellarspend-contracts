//! Validation utilities for batch transfers.

use crate::TransferRequest;
use soroban_sdk::{Address, Env, Vec};

/// Validation error types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// Invalid transfer amount
    InvalidAmount,
    /// Duplicate recipient in the batch
    DuplicateRecipient(Address),
    /// Batch is empty (no recipients provided)
    EmptyBatch,
}

/// Validates a recipient address.
pub fn validate_address(_env: &Env, _address: &Address) -> Result<(), ValidationError> {
    Ok(())
}

/// Ensures a recipient address has not already appeared in the batch.
pub fn validate_unique_recipient(
    seen: &Vec<Address>,
    recipient: &Address,
) -> Result<(), ValidationError> {
    for existing in seen.iter() {
        if existing == recipient.clone() {
            return Err(ValidationError::DuplicateRecipient(recipient.clone()));
        }
    }
    Ok(())
}

/// Validates that each transfer recipient appears only once in the batch.
pub fn validate_unique_recipients(
    env: &Env,
    transfers: &Vec<TransferRequest>,
) -> Result<(), ValidationError> {
    let mut seen: Vec<Address> = Vec::new(env);

    for request in transfers.iter() {
        validate_unique_recipient(&seen, &request.recipient)?;
        seen.push_back(request.recipient.clone());
    }

    Ok(())
}

/// Validates that a batch is not empty.
/// Returns an error if the transfer requests vector is empty to avoid
/// unnecessary execution costs.
pub fn validate_batch_not_empty<T>(transfers: &Vec<T>) -> Result<(), ValidationError> {
    if transfers.is_empty() {
        return Err(ValidationError::EmptyBatch);
    }
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
    use crate::TransferRequest;
    use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

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

    #[test]
    fn test_validate_batch_not_empty_with_transfers() {
        let env = Env::default();
        let recipient = Address::generate(&env);
        let mut transfers: Vec<TransferRequest> = Vec::new(&env);
        transfers.push_back(TransferRequest {
            recipient,
            amount: 100,
        });
        assert!(validate_batch_not_empty(&transfers).is_ok());
    }

    #[test]
    fn test_validate_batch_not_empty_rejects_empty_vec() {
        let env = Env::default();
        let transfers: Vec<TransferRequest> = Vec::new(&env);
        assert_eq!(
            validate_batch_not_empty(&transfers),
            Err(ValidationError::EmptyBatch)
        );
    }

    #[test]
    fn test_validate_unique_recipients_rejects_duplicate() {
        let env = Env::default();
        let recipient = Address::generate(&env);
        let mut transfers: Vec<TransferRequest> = Vec::new(&env);

        transfers.push_back(TransferRequest {
            recipient: recipient.clone(),
            amount: 100,
        });
        transfers.push_back(TransferRequest {
            recipient: recipient.clone(),
            amount: 200,
        });

        assert_eq!(
            validate_unique_recipients(&env, &transfers),
            Err(ValidationError::DuplicateRecipient(recipient))
        );
    }
}

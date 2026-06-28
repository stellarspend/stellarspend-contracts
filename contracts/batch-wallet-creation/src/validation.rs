use std::collections::HashSet;

#[derive(Debug, PartialEq)]
pub enum ValidationError {
    DuplicateWallet(String),
    EmptyBatch,
}

pub fn validate_no_duplicates(wallets: &[String]) -> Result<(), ValidationError> {
    if wallets.is_empty() {
        return Err(ValidationError::EmptyBatch);
    }
    let mut seen = HashSet::new();
    for w in wallets {
        if !seen.insert(w) {
            return Err(ValidationError::DuplicateWallet(w.clone()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_duplicates_passes() {
        let wallets = vec!["GAAA".into(), "GBBB".into(), "GCCC".into()];
        assert_eq!(validate_no_duplicates(&wallets), Ok(()));
    }

    #[test]
    fn test_duplicate_rejected() {
        let wallets = vec!["GAAA".into(), "GBBB".into(), "GAAA".into()];
        assert_eq!(validate_no_duplicates(&wallets), Err(ValidationError::DuplicateWallet("GAAA".into())));
    }

    #[test]
    fn test_empty_batch_rejected() {
        assert_eq!(validate_no_duplicates(&[]), Err(ValidationError::EmptyBatch));
    }
}

//! Validation logic for wallet nicknames.

extern crate alloc;
use alloc::string::ToString;
use soroban_sdk::Symbol;

/// Validates that a wallet nickname is not empty and has reasonable length.
///
/// # Returns
/// * `Ok(())` if valid
/// * `Err(())` if invalid (empty or too long)
pub fn validate_nickname(nickname: &Symbol) -> Result<(), ()> {
    let len = nickname.to_string().len();
    if len == 0 || len > 32 {
        return Err(());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{symbol_short, Env};

    #[test]
    fn test_valid_nickname() {
        let nickname = symbol_short!("MyWallet");
        assert!(validate_nickname(&nickname).is_ok());
    }

    #[test]
    fn test_valid_short_nickname() {
        let nickname = symbol_short!("W");
        assert!(validate_nickname(&nickname).is_ok());
    }

    #[test]
    fn test_valid_max_length_nickname() {
        let env = Env::default();
        let nickname = Symbol::new(&env, "abcdefghijklmnopqrstuvwxyz012345");
        assert!(validate_nickname(&nickname).is_ok());
    }
}

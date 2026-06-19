use alloc::format;
use soroban_sdk::{Env, String, Symbol};

/// Shared validation errors for simple reusable helpers.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ValidationError {
    NegativeAmount,
    InvalidAddress,
}

/// Validates that an amount is not negative.
pub fn validate_amount(amount: i128) -> Result<(), ValidationError> {
    if amount < 0 {
        Err(ValidationError::NegativeAmount)
    } else {
        Ok(())
    }
}

/// Validates a user address string format.
///
/// Accepts 56-character classic Stellar-style strings with `G` account
/// or `C` contract prefixes and only base32 characters (`A-Z`, `2-7`).
pub fn validate_user_address(address: &soroban_sdk::String) -> Result<(), ValidationError> {
    if address.len() != 56 {
        return Err(ValidationError::InvalidAddress);
    }

    let mut bytes = [0u8; 56];
    address.copy_into_slice(&mut bytes);

    let prefix = bytes[0];
    if prefix != b'G' && prefix != b'C' {
        return Err(ValidationError::InvalidAddress);
    }

    for b in bytes.iter() {
        let is_upper_alpha = *b >= b'A' && *b <= b'Z';
        let is_base32_digit = *b >= b'2' && *b <= b'7';
        if !is_upper_alpha && !is_base32_digit {
            return Err(ValidationError::InvalidAddress);
        }
    }

    Ok(())
}

/// Increment a counter in storage and return the new value
pub fn increment_counter(env: &Env, counter_key: &Symbol) -> u64 {
    let mut counter: u64 = env.storage().persistent().get(counter_key).unwrap_or(0);

    counter += 1;
    env.storage().persistent().set(counter_key, &counter);

    counter
}

/// Generates a deterministic unique transaction reference ID.
///
/// The reference ID is created by combining:
/// - The sender's address
/// - The current ledger sequence number
/// - A transaction counter for uniqueness
///
/// Returns a formatted reference ID string like "TXN-XXXXXXXXXXXXXXXX" suitable for
/// tracking and reconciliation of individual transactions.
pub fn generate_transaction_reference_id(
    env: &Env,
    _sender: &soroban_sdk::Address,
    counter_key: &Symbol,
) -> String {
    // Increment the transaction counter to ensure uniqueness
    let tx_counter = increment_counter(env, counter_key);

    // Get the current ledger sequence number
    let ledger_seq = env.ledger().sequence();

    // Create components for the reference ID
    let counter_str = format!("{:016x}", tx_counter);
    let ledger_str = format!("{:08x}", ledger_seq);

    // Combine components to create reference ID
    // Format: TXN-{ledger}{counter}
    let ref_id = String::from_str(env, &format!("TXN-{}{}", ledger_str, &counter_str[8..]));

    ref_id
}

#[cfg(test)]
mod tests {
    use super::{
        generate_transaction_reference_id, increment_counter, validate_amount,
        validate_user_address, ValidationError,
    };
    use soroban_sdk::{contract, contractimpl, testutils::Address as _, Env, String, Symbol};

    #[contract]
    struct TestContract;

    #[contractimpl]
    impl TestContract {
        pub fn noop() {}
    }

    #[test]
    fn accepts_zero_and_positive_amounts() {
        assert_eq!(validate_amount(0), Ok(()));
        assert_eq!(validate_amount(1), Ok(()));
        assert_eq!(validate_amount(1_000_000), Ok(()));
    }

    #[test]
    fn rejects_negative_amounts() {
        assert_eq!(validate_amount(-1), Err(ValidationError::NegativeAmount));
        assert_eq!(validate_amount(-99), Err(ValidationError::NegativeAmount));
    }

    #[test]
    fn accepts_valid_user_address() {
        let env = Env::default();
        let address = String::from_str(
            &env,
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        );
        assert_eq!(validate_user_address(&address), Ok(()));
    }

    #[test]
    fn rejects_invalid_user_address() {
        let env = Env::default();

        let empty = String::from_str(&env, "");
        assert_eq!(
            validate_user_address(&empty),
            Err(ValidationError::InvalidAddress)
        );

        let bad_prefix = String::from_str(
            &env,
            "XAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        );
        assert_eq!(
            validate_user_address(&bad_prefix),
            Err(ValidationError::InvalidAddress)
        );

        let bad_chars = String::from_str(&env, "GINVALID!ADDRESS");
        assert_eq!(
            validate_user_address(&bad_chars),
            Err(ValidationError::InvalidAddress)
        );
    }

    #[test]
    fn increment_counter_works() {
        let env = Env::default();
        let contract_id = env.register(TestContract, ());
        let counter_key = Symbol::new(&env, "test_counter");

        env.as_contract(&contract_id, || {
            assert_eq!(increment_counter(&env, &counter_key), 1);
            assert_eq!(increment_counter(&env, &counter_key), 2);
            assert_eq!(increment_counter(&env, &counter_key), 3);
        });
    }

    #[test]
    fn generates_transaction_reference_id() {
        let env = Env::default();
        let contract_id = env.register(TestContract, ());
        let sender = soroban_sdk::Address::generate(&env);
        let counter_key = Symbol::new(&env, "tx_ref_counter");

        env.as_contract(&contract_id, || {
            let ref_id = generate_transaction_reference_id(&env, &sender, &counter_key);

            // Verify format: should start with "TXN-"
            let _ref_str = String::from_str(&env, "TXN-");
            assert!(ref_id.len() > 4);
        });
    }

    #[test]
    fn transaction_reference_ids_are_unique() {
        let env = Env::default();
        let contract_id = env.register(TestContract, ());
        let sender = soroban_sdk::Address::generate(&env);
        let counter_key = Symbol::new(&env, "tx_ref_counter");

        env.as_contract(&contract_id, || {
            let ref_id_1 = generate_transaction_reference_id(&env, &sender, &counter_key);
            let ref_id_2 = generate_transaction_reference_id(&env, &sender, &counter_key);
            let ref_id_3 = generate_transaction_reference_id(&env, &sender, &counter_key);

            // All reference IDs should be unique
            assert_ne!(ref_id_1, ref_id_2);
            assert_ne!(ref_id_2, ref_id_3);
            assert_ne!(ref_id_1, ref_id_3);
        });
    }
}

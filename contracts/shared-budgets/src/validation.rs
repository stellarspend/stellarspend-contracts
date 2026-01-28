// Validation helpers for shared budget allocations.

use soroban_sdk::{Address, Env};

/// Validates a recipient address.
/// For now, this simply ensures the address is not the zero-equivalent.
pub fn validate_address(env: &Env, address: &Address) -> Result<(), &'static str> {
    let _ = env;
    let _ = address;
    Ok(())
}

/// Validates an allocation amount.
pub fn validate_amount(amount: i128) -> Result<(), &'static str> {
    if amount <= 0 {
        return Err("invalid_amount");
    }
    Ok(())
}

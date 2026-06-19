//! Validation logic for balance update requests.

use soroban_sdk::{symbol_short, Address, Env, Symbol, Vec};

use crate::types::{
    BalanceUpdateRequest, CurrencyBalance, DataKey, ErrorCode, MAX_BALANCE, MIN_BALANCE,
};

/// Validates a balance update request.
///
/// # Returns
/// * `Ok(())` if valid
/// * `Err(error_code)` if invalid
pub fn validate_balance_request(request: &BalanceUpdateRequest) -> Result<(), u32> {
    // Validate user address
    if !is_valid_address(&request.user) {
        return Err(ErrorCode::INVALID_USER_ADDRESS);
    }

    // Validate currency
    if !is_valid_currency(&request.currency) {
        return Err(ErrorCode::INVALID_CURRENCY);
    }

    // Validate amount
    if !is_valid_amount(request.amount) {
        return Err(ErrorCode::INVALID_AMOUNT);
    }

    // Validate operation type
    if !is_valid_operation(&request.operation) {
        return Err(ErrorCode::INVALID_OPERATION);
    }

    Ok(())
}

/// Validates that an address is valid.
fn is_valid_address(_address: &Address) -> bool {
    // Address is always valid in Soroban SDK by construction
    true
}

/// Validates that a currency symbol is valid.
fn is_valid_currency(_currency: &Symbol) -> bool {
    // Symbol is always valid in Soroban SDK by construction
    // Currency symbols are typically 3-4 characters (e.g., XLM, USDC)
    true
}

/// Validates that an amount is within acceptable bounds.
///
/// # Arguments
/// * `amount` - The amount to validate
///
/// # Returns
/// * `true` if amount is >= MIN_BALANCE and <= MAX_BALANCE
pub fn is_valid_amount(amount: i128) -> bool {
    amount >= MIN_BALANCE && amount <= MAX_BALANCE
}

/// Validates that an operation type is valid.
///
/// # Arguments
/// * `operation` - The operation symbol to validate
///
/// # Returns
/// * `true` if operation is "set", "add", or "subtract"
pub fn is_valid_operation(operation: &Symbol) -> bool {
    is_set_operation(operation) || is_add_operation(operation) || is_subtract_operation(operation)
}

/// Rejects transfer-like debit/credit pairs that use different currencies.
pub fn validate_transfer_currency_consistency(
    request: &BalanceUpdateRequest,
    requests: &Vec<BalanceUpdateRequest>,
) -> Result<(), u32> {
    if !is_transfer_operation(&request.operation) {
        return Ok(());
    }

    for other in requests.iter() {
        if request.amount == other.amount
            && is_opposite_transfer_operation(&request.operation, &other.operation)
            && request.currency != other.currency
        {
            return Err(ErrorCode::INVALID_CURRENCY);
        }
    }

    Ok(())
}

/// Validates balance after operation to prevent negative balances.
///
/// # Arguments
/// * `env` - The contract environment
/// * `user` - User's address
/// * `currency` - Currency symbol
/// * `operation` - Operation to perform
/// * `amount` - Amount for the operation
///
/// # Returns
/// * `Ok(new_balance)` if operation is valid
/// * `Err(error_code)` if operation would result in invalid balance
pub fn validate_and_compute_balance(
    env: &Env,
    user: &Address,
    currency: &Symbol,
    operation: &Symbol,
    amount: i128,
) -> Result<i128, u32> {
    // Get current balance
    let current_balance: i128 = env
        .storage()
        .persistent()
        .get(&DataKey::Balance(user.clone(), currency.clone()))
        .map(|balance: CurrencyBalance| balance.balance)
        .unwrap_or(0);

    // Compute new balance based on operation
    let new_balance = compute_new_balance(current_balance, operation, amount)?;

    // Validate new balance is non-negative
    if new_balance < 0 {
        return Err(ErrorCode::INSUFFICIENT_BALANCE);
    }

    // Validate new balance doesn't exceed maximum
    if new_balance > MAX_BALANCE {
        return Err(ErrorCode::ARITHMETIC_OVERFLOW);
    }

    Ok(new_balance)
}

/// Computes new balance based on operation.
fn compute_new_balance(current: i128, operation: &Symbol, amount: i128) -> Result<i128, u32> {
    if is_set_operation(operation) {
        Ok(amount)
    } else if is_add_operation(operation) {
        current
            .checked_add(amount)
            .ok_or(ErrorCode::ARITHMETIC_OVERFLOW)
    } else if is_subtract_operation(operation) {
        current
            .checked_sub(amount)
            .ok_or(ErrorCode::ARITHMETIC_OVERFLOW)
    } else {
        Err(ErrorCode::INVALID_OPERATION)
    }
}

fn is_set_operation(operation: &Symbol) -> bool {
    *operation == symbol_short!("set")
}

fn is_add_operation(operation: &Symbol) -> bool {
    *operation == symbol_short!("add")
}

fn is_subtract_operation(operation: &Symbol) -> bool {
    *operation == symbol_short!("subtract")
}

fn is_transfer_operation(operation: &Symbol) -> bool {
    is_add_operation(operation) || is_subtract_operation(operation)
}

fn is_opposite_transfer_operation(left: &Symbol, right: &Symbol) -> bool {
    (is_add_operation(left) && is_subtract_operation(right))
        || (is_subtract_operation(left) && is_add_operation(right))
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{symbol_short, testutils::Address as _, Env};

    fn create_valid_request(env: &Env) -> BalanceUpdateRequest {
        BalanceUpdateRequest {
            user: Address::generate(env),
            currency: symbol_short!("USDC"),
            amount: 1000_000_000, // 1000 USDC
            operation: symbol_short!("set"),
        }
    }

    #[test]
    fn test_valid_balance_request() {
        let env = Env::default();
        let request = create_valid_request(&env);
        assert!(validate_balance_request(&request).is_ok());
    }

    #[test]
    fn test_invalid_amount_zero() {
        let env = Env::default();
        let mut request = create_valid_request(&env);
        request.amount = 0;
        assert_eq!(
            validate_balance_request(&request),
            Err(ErrorCode::INVALID_AMOUNT)
        );
    }

    #[test]
    fn test_invalid_amount_negative() {
        let env = Env::default();
        let mut request = create_valid_request(&env);
        request.amount = -1000;
        assert_eq!(
            validate_balance_request(&request),
            Err(ErrorCode::INVALID_AMOUNT)
        );
    }

    #[test]
    fn test_is_valid_amount() {
        assert!(is_valid_amount(MIN_BALANCE));
        assert!(is_valid_amount(MAX_BALANCE));
        assert!(is_valid_amount(1000_000_000));
        assert!(!is_valid_amount(MIN_BALANCE - 1));
        assert!(!is_valid_amount(0));
        assert!(!is_valid_amount(-1000));
    }

    #[test]
    fn test_valid_operations() {
        let set_op = symbol_short!("set");
        let add_op = symbol_short!("add");
        let subtract_op = symbol_short!("subtract");

        assert!(is_valid_operation(&set_op));
        assert!(is_valid_operation(&add_op));
        assert!(is_valid_operation(&subtract_op));
    }
}

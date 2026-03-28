//! StellarSpend Contracts Library
//!
//! This library provides standardized error handling, common utilities,
//! and shared functionality for all StellarSpend contracts.

pub mod delegation;
pub mod errors;
pub mod fees;
pub mod storage;
// Re-export commonly used types and functions
pub use errors::{
    ErrorCategory, ErrorContext, ErrorDocumentation, ErrorHelpers, ErrorSeverity, RetryStrategy,
    StellarSpendError,
};

use soroban_sdk::{contracterror, contracttype, panic_with_error, Address, Env, Map, String, Vec};

/// Standardized contract error macro
///
/// This macro provides a consistent way to panic with standardized errors
/// across all contracts. It automatically maps the error to the appropriate
/// StellarSpendError and provides context information.
#[macro_export]
macro_rules! std_error {
    ($env:expr, $error:expr) => {
        panic_with_error!($env, $error);
    };
    ($env:expr, $error:expr, $context:expr) => {
        // Log error context if provided
        if errors::ErrorHelpers::should_log($error as u32) {
            let context = errors::ErrorHelpers::create_context(
                $env,
                $error as u32,
                env::current_contract_id($env).to_string(),
                "unknown_function", // This would be filled by the calling function
                Vec::new($env),
                Map::new($env),
            );
            // In a real implementation, you would store this context
        }
        panic_with_error!($env, $error);
    };
}

/// Standardized validation macro
///
/// Provides consistent validation patterns across contracts
#[macro_export]
macro_rules! validate {
    ($env:expr, $condition:expr, $error:expr) => {
        if !$condition {
            std_error!($env, $error);
        }
    };
    ($env:expr, $condition:expr, $error:expr, $message:expr) => {
        if !$condition {
            // Log validation failure message
            std_error!($env, $error, $message);
        }
    };
}

/// Standardized authorization check macro
#[macro_export]
macro_rules! require_auth {
    ($env:expr, $caller:expr, $required:expr) => {
        $caller.require_auth();
        if $caller != $required {
            std_error!($env, StellarSpendError::Unauthorized);
        }
    };
}

/// Standardized admin check macro
#[macro_export]
macro_rules! require_admin {
    ($env:expr, $caller:expr) => {
        $caller.require_auth();
        let admin = get_admin($env);
        if $caller != admin {
            std_error!($env, StellarSpendError::AdminRequired);
        }
    };
}

/// Standardized amount validation macro
#[macro_export]
macro_rules! validate_amount {
    ($env:expr, $amount:expr) => {
        validate!($env, $amount > 0, StellarSpendError::InvalidAmount);
        validate!(
            $env,
            $amount <= i128::MAX / 2,
            StellarSpendError::AmountTooLarge
        );
    };
    ($env:expr, $amount:expr, $min:expr) => {
        validate!($env, $amount >= $min, StellarSpendError::AmountTooSmall);
        validate_amount!($env, $amount);
    };
    ($env:expr, $amount:expr, $min:expr, $max:expr) => {
        validate!($env, $amount >= $min, StellarSpendError::AmountTooSmall);
        validate!($env, $amount <= $max, StellarSpendError::AmountTooLarge);
        validate!($env, $amount > 0, StellarSpendError::InvalidAmount);
    };
}

/// Standardized address validation macro
#[macro_export]
macro_rules! validate_address {
    ($env:expr, $address:expr) => {
        validate!($env, !$address.is_none(), StellarSpendError::InvalidAddress);
        validate!(
            $env,
            $address != Address::from_contract_id($env),
            StellarSpendError::ZeroAddress
        );
    };
}

/// Standardized safe arithmetic macros
#[macro_export]
macro_rules! safe_add {
    ($env:expr, $a:expr, $b:expr) => {
        $a.checked_add($b).unwrap_or_else(|| {
            std_error!($env, StellarSpendError::Overflow);
            0i128
        })
    };
}

#[macro_export]
macro_rules! safe_sub {
    ($env:expr, $a:expr, $b:expr) => {
        $a.checked_sub($b).unwrap_or_else(|| {
            std_error!($env, StellarSpendError::Underflow);
            0i128
        })
    };
}

#[macro_export]
macro_rules! safe_mul {
    ($env:expr, $a:expr, $b:expr) => {
        $a.checked_mul($b).unwrap_or_else(|| {
            std_error!($env, StellarSpendError::Overflow);
            0i128
        })
    };
}

#[macro_export]
macro_rules! safe_div {
    ($env:expr, $a:expr, $b:expr) => {
        validate!($env, $b != 0, StellarSpendError::DivisionByZero);
        $a.checked_div($b).unwrap_or_else(|| {
            std_error!($env, StellarSpendError::InvalidCalculation);
            0i128
        })
    };
}

/// Common contract utilities
pub struct ContractUtils;

impl ContractUtils {
    /// Get contract admin from storage
    pub fn get_admin(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&errors::DataKey::Admin)
            .unwrap_or_else(|| std_error!(env, StellarSpendError::NotInitialized))
    }

    /// Check if contract is initialized
    pub fn is_initialized(env: &Env) -> bool {
        env.storage().instance().has(&errors::DataKey::Admin)
    }

    /// Validate contract state
    pub fn require_initialized(env: &Env) {
        validate!(
            env,
            Self::is_initialized(env),
            StellarSpendError::NotInitialized
        );
    }

    /// Get current timestamp with validation
    pub fn get_timestamp(env: &Env) -> u64 {
        let timestamp = env.ledger().timestamp();
        validate!(env, timestamp > 0, StellarSpendError::InvalidTimestamp);
        timestamp
    }

    /// Generate unique transaction ID
    pub fn generate_transaction_id(env: &Env) -> u64 {
        let timestamp = env.ledger().timestamp();
        let sequence = env.ledger().sequence();
        safe_add!(env, timestamp, sequence) as u64
    }

    /// Emit standardized error event
    pub fn emit_error_event(env: &Env, error: StellarSpendError, context: Option<&ErrorContext>) {
        let topics = (
            soroban_sdk::symbol_short!("error"),
            soroban_sdk::symbol_short!("contract"),
        );

        let data = (
            error.code(),
            error.category() as u32,
            error.severity() as u32,
            env.ledger().timestamp(),
        );

        env.events().publish(topics, data);

        // Emit additional context if provided
        if let Some(ctx) = context {
            let ctx_topics = (
                soroban_sdk::symbol_short!("error_context"),
                soroban_sdk::symbol_short!("details"),
            );
            let ctx_data = (
                ctx.contract_name.clone(),
                ctx.function_name.clone(),
                ctx.error_code,
                ctx.timestamp,
            );
            env.events().publish(ctx_topics, ctx_data);
        }
    }

    /// Check rate limit for user
    pub fn check_rate_limit(
        env: &Env,
        user: &Address,
        operation: &str,
        limit: u32,
        window_seconds: u64,
    ) -> Result<(), StellarSpendError> {
        let current_time = env.ledger().timestamp();
        let key = errors::DataKey::RateLimit(user.clone(), operation.into());

        // Get current rate limit data
        let rate_data: Option<RateLimitData> = env.storage().temporary().get(&key);

        match rate_data {
            Some(data) => {
                if current_time < data.window_start + window_seconds {
                    // Within current window
                    if data.count >= limit {
                        return Err(StellarSpendError::RateLimitExceeded);
                    }

                    // Update count
                    let updated_data = RateLimitData {
                        count: data.count + 1,
                        window_start: data.window_start,
                    };
                    env.storage().temporary().set(&key, &updated_data);
                } else {
                    // New window
                    let new_data = RateLimitData {
                        count: 1,
                        window_start: current_time,
                    };
                    env.storage().temporary().set(&key, &new_data);
                }
            }
            None => {
                // First operation in window
                let new_data = RateLimitData {
                    count: 1,
                    window_start: current_time,
                };
                env.storage().temporary().set(&key, &new_data);
            }
        }

        Ok(())
    }
}

/// Rate limiting data structure
#[derive(Clone)]
#[contracttype]
pub struct RateLimitData {
    pub count: u32,
    pub window_start: u64,
}

/// Extended DataKey enum for common storage patterns
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    RateLimit(Address, String),
    ErrorLog(u64),
    Config(String),
    State(String),
    Metadata(String),
    AuditLog,
}

/// Standardized contract trait
///
/// Contracts can implement this trait to get common functionality
pub trait StandardContract {
    /// Get contract name
    fn contract_name() -> &'static str;

    /// Get contract version
    fn contract_version() -> &'static str;

    /// Initialize contract with standard checks
    fn initialize_standard(env: &Env, admin: Address) -> Result<(), StellarSpendError>;

    /// Validate contract state
    fn validate_state(env: &Env) -> Result<(), StellarSpendError>;

    /// Get contract metrics
    fn get_metrics(env: &Env) -> ContractMetrics;
}

/// Contract metrics structure
#[derive(Clone)]
#[contracttype]
pub struct ContractMetrics {
    pub name: String,
    pub version: String,
    pub total_operations: u64,
    pub total_errors: u64,
    pub last_operation: u64,
    pub is_paused: bool,
}

/// Standardized event emission
pub struct EventEmit;

impl EventEmit {
    /// Emit standardized operation event
    pub fn operation_started(env: &Env, operation: &str, user: &Address, parameters: Vec<String>) {
        let topics = (
            soroban_sdk::symbol_short!("operation"),
            soroban_sdk::symbol_short!("started"),
        );
        let data = (
            operation,
            user.clone(),
            parameters,
            env.ledger().timestamp(),
        );
        env.events().publish(topics, data);
    }

    /// Emit standardized operation completed event
    pub fn operation_completed(env: &Env, operation: &str, user: &Address, result: &str) {
        let topics = (
            soroban_sdk::symbol_short!("operation"),
            soroban_sdk::symbol_short!("completed"),
        );
        let data = (operation, user.clone(), result, env.ledger().timestamp());
        env.events().publish(topics, data);
    }

    /// Emit standardized operation failed event
    pub fn operation_failed(env: &Env, operation: &str, user: &Address, error: StellarSpendError) {
        let topics = (
            soroban_sdk::symbol_short!("operation"),
            soroban_sdk::symbol_short!("failed"),
        );
        let data = (
            operation,
            user.clone(),
            error.code(),
            env.ledger().timestamp(),
        );
        env.events().publish(topics, data);
    }
}

/// Standardized testing utilities
#[cfg(test)]
pub mod testing {
    use super::*;
    use soroban_sdk::{Address, Env};

    /// Setup test environment with standard configuration
    pub fn setup_test_env() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        (env, admin)
    }

    /// Create test error context
    pub fn create_test_context(
        env: &Env,
        error_code: u32,
        contract_name: &str,
        function_name: &str,
    ) -> ErrorContext {
        ErrorHelpers::create_context(
            env,
            error_code,
            contract_name,
            function_name,
            Vec::new(env),
            Map::new(env),
        )
    }

    /// Assert error with standardized checking
    pub fn assert_error(
        env: &Env,
        result: Result<(), StellarSpendError>,
        expected: StellarSpendError,
    ) {
        match result {
            Err(error) => assert_eq!(error, expected),
            Ok(_) => panic!("Expected error but got success"),
        }
    }

    /// Assert success with standardized checking
    pub fn assert_success(env: &Env, result: Result<(), StellarSpendError>) {
        match result {
            Ok(_) => {} // Success as expected
            Err(error) => panic!("Expected success but got error: {:?}", error),
        }
    }
}

/// Re-export commonly used Soroban types for convenience
pub use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, Address,
    Env, Map, String, Vec, U256,
};

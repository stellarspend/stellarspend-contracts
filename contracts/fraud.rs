//! Fraud detection logic for flagging suspicious transactions.

use soroban_sdk::{
    contract, contractimpl, contracterror, contracttype, panic_with_error, symbol_short,
    Address, Env, String,
};

const DEFAULT_FRAUD_THRESHOLD: i128 = 10_000;
const DEFAULT_MAX_DAILY: i128 = 100_000;

/// Storage keys for the fraud contract.
#[derive(Clone)]
#[contracttype]
pub enum FraudDataKey {
    Admin,
    Config,
    /// Per-user daily total keyed by `(user_address, day_number)`.
    UserDaily(Address, u64),
    /// Fraud flag reason keyed by user address.
    FraudReason(Address),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum FraudError {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
    InvalidThreshold = 4,
    // [SEC-FRAUD-01] Explicit overflow guard.
    Overflow = 5,
    // [SEC-FRAUD-02] Explicit invalid-amount guard.
    InvalidAmount = 6,
}

/// Persisted fraud configuration.
#[derive(Clone)]
#[contracttype]
pub struct FraudConfig {
    pub threshold: i128,
    pub max_daily: i128,
}

impl FraudConfig {
    fn default_config() -> Self {
        Self {
            threshold: DEFAULT_FRAUD_THRESHOLD,
            max_daily: DEFAULT_MAX_DAILY,
        }
    }
}

#[contract]
pub struct FraudContract;

#[contractimpl]
impl FraudContract {
    /// Initializes the contract with default fraud thresholds.
    /// Only callable once.
    pub fn initialize(env: Env, admin: Address) {
        // [SEC-FRAUD-03] Re-initialization guard.
        if env.storage().instance().has(&FraudDataKey::Admin) {
            panic_with_error!(&env, FraudError::AlreadyInitialized);
        }
        env.storage().instance().set(&FraudDataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&FraudDataKey::Config, &FraudConfig::default_config());
    }

    /// Updates fraud thresholds. Only the admin may call.
    ///
    /// # Security
    /// - [SEC-FRAUD-04] Admin-only gate prevents arbitrary callers from
    ///   disabling fraud detection by setting large thresholds.
    /// - Both `threshold` and `max_daily` must be strictly positive.
    pub fn set_config(env: Env, admin: Address, threshold: i128, max_daily: i128) {
        // [SEC-FRAUD-04] Authenticate and verify admin before writing config.
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&FraudDataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, FraudError::NotInitialized));

        if admin != stored_admin {
            panic_with_error!(&env, FraudError::Unauthorized);
        }

        if threshold <= 0 || max_daily <= 0 {
            panic_with_error!(&env, FraudError::InvalidThreshold);
        }

        env.storage()
            .instance()
            .set(&FraudDataKey::Config, &FraudConfig { threshold, max_daily });

        env.events().publish(
            (symbol_short!("fraud"), symbol_short!("cfg")),
            (admin.clone(), threshold, max_daily, env.ledger().timestamp()),
        );
    }

    /// Evaluates a transaction for fraud signals.
    ///
    /// Returns `true` when the transaction is flagged.
    ///
    /// # Security
    /// - [SEC-FRAUD-02] Non-positive amounts are rejected; previously a zero or
    ///   negative amount would silently pass all threshold checks.
    /// - [SEC-FRAUD-01] Daily total accumulation uses `checked_add`; previously
    ///   it used plain `+` which could wrap on i128::MAX.
    /// - [SEC-FRAUD-05] Config is read from persistent storage rather than
    ///   re-instantiated from defaults so admin updates take effect immediately.
    /// - [SEC-FRAUD-06] Per-user daily key uses a typed `UserDaily(Address, u64)`
    ///   variant instead of a raw tuple, preventing key-collision attacks between
    ///   different contract data namespaces.
    pub fn check_transaction(env: Env, user: Address, amount: i128) -> bool {
        // [SEC-FRAUD-02] Reject non-positive amounts immediately.
        if amount <= 0 {
            panic_with_error!(&env, FraudError::InvalidAmount);
        }

        // [SEC-FRAUD-05] Load persisted config; fall back to defaults only if
        // the contract was deployed without calling initialize.
        let config: FraudConfig = env
            .storage()
            .instance()
            .get(&FraudDataKey::Config)
            .unwrap_or_else(FraudConfig::default_config);

        let mut flagged = false;
        let mut reason = String::from_str(&env, "");

        // Rule 1: Single-transaction size check.
        if amount >= config.threshold {
            flagged = true;
            reason = String::from_str(&env, "threshold_exceeded");
        }

        // Rule 2: Rolling daily total check.
        // Day number is stable within a 24-hour window.
        let today: u64 = env.ledger().timestamp() / 86_400;

        // [SEC-FRAUD-06] Typed storage key prevents namespace collisions.
        let user_key = FraudDataKey::UserDaily(user.clone(), today);

        let prev_total: i128 = env
            .storage()
            .persistent()
            .get(&user_key)
            .unwrap_or(0);

        // [SEC-FRAUD-01] Checked addition — overflow surfaces as a typed error.
        let new_total = prev_total
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(&env, FraudError::Overflow));

        env.storage().persistent().set(&user_key, &new_total);

        if new_total > config.max_daily {
            flagged = true;
            if reason.is_empty() {
                reason = String::from_str(&env, "daily_limit_exceeded");
            }
        }

        // Store fraud reason if flagged
        if flagged {
            env.storage()
                .persistent()
                .set(&FraudDataKey::FraudReason(user.clone()), &reason);

            // Emit a structured fraud alert event when flagged.
            env.events().publish(
                (symbol_short!("fraud"), symbol_short!("alert"), user.clone()),
                (amount, new_total, env.ledger().timestamp()),
            );
        }

        flagged
    }

    /// Returns the fraud flag reason for the given address.
    ///
    /// Returns `Some(reason)` if the address is flagged, `None` if unflagged.
    pub fn get_fraud_flag_reason(env: Env, user: Address) -> Option<String> {
        env.storage()
            .persistent()
            .get(&FraudDataKey::FraudReason(user))
    }
}

#[cfg(test)]
mod tests {
    use super::{FraudContract, FraudContractClient};
    use soroban_sdk::{testutils::Address as _, Address, Env};

    fn setup<'a>() -> (Env, Address, FraudContractClient<'a>) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(FraudContract, ());
        let client = FraudContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        (env, admin, client)
    }

    #[test]
    fn get_fraud_flag_reason_returns_none_for_unflagged() {
        let (_env, _admin, client) = setup();
        let user = Address::generate(&env);
        assert!(client.get_fraud_flag_reason(&user).is_none());
    }

    #[test]
    fn get_fraud_flag_reason_returns_reason_when_flagged() {
        let (_env, _admin, client) = setup();
        let user = Address::generate(&env);
        client.check_transaction(&user, &20_000);
        let reason = client.get_fraud_flag_reason(&user);
        assert!(reason.is_some());
    }
}

use soroban_sdk::{panic_with_error, symbol_short, Env, Address, String, Symbol, IntoVal};
use crate::oracle::{PriceOracle, Price, OracleError};

/// Reflector Oracle Adapter
/// 
/// This adapter integrates with a Reflector-style oracle contract.
/// Reflector provides on-chain price feeds with TWAP support.
pub struct ReflectorOracle {
    /// The address of the Reflector oracle contract
    pub contract_address: Address,
}

impl ReflectorOracle {
    pub fn new(contract_address: Address) -> Self {
        Self { contract_address }
    }
}

impl PriceOracle for ReflectorOracle {
    fn get_price(&self, env: &Env, asset_a: String, asset_b: String) -> Price {
        // Call the Reflector contract to get the current price
        let fn_name: Symbol = symbol_short!("get_price");
        let args = soroban_sdk::vec![&env, asset_a.clone().into_val(env), asset_b.clone().into_val(env)];
        let result: Result<(i128, u64), soroban_sdk::Error> = env.invoke_contract(&self.contract_address, &fn_name, args);
        let result = result.unwrap_or_else(|_| panic_with_error!(env, OracleError::OracleUnavailable));

        let (value, timestamp) = result;

        Price {
            value,
            timestamp,
            source: String::from_str(env, "Reflector"),
            is_twap: false,
            window_seconds: 0,
        }
    }

    fn get_twap(&self, env: &Env, asset_a: String, asset_b: String, window_seconds: u64) -> Price {
        // Call the Reflector contract to get TWAP
        let fn_name: Symbol = symbol_short!("get_twap");
        let args = soroban_sdk::vec![
            &env,
            asset_a.clone().into_val(env),
            asset_b.clone().into_val(env),
            window_seconds.into_val(env),
        ];
        let result: Result<(i128, u64), soroban_sdk::Error> = env.invoke_contract(&self.contract_address, &fn_name, args);
        let result = result.unwrap_or_else(|_| panic_with_error!(env, OracleError::OracleUnavailable));

        let (value, timestamp) = result;

        Price {
            value,
            timestamp,
            source: String::from_str(env, "Reflector"),
            is_twap: true,
            window_seconds,
        }
    }

    fn is_fresh(&self, env: &Env, asset_a: String, asset_b: String, staleness_threshold: u64) -> bool {
        let price = self.get_price(env, asset_a, asset_b);
        let current_time = env.ledger().timestamp();
        
        // Check if the price is fresh
        if current_time < price.timestamp {
            return false; // Timestamp in the future (invalid)
        }
        
        let age = current_time - price.timestamp;
        age <= staleness_threshold
    }
}

/// Mock Oracle for testing
pub struct MockOracle {
    pub price_value: i128,
    pub price_timestamp: u64,
    pub is_fresh_result: bool,
    pub should_fail: bool,
}

impl PriceOracle for MockOracle {
    fn get_price(&self, env: &Env, _asset_a: String, _asset_b: String) -> Price {
        if self.should_fail {
            panic_with_error!(env, OracleError::OracleUnavailable);
        }

        Price {
            value: self.price_value,
            timestamp: self.price_timestamp,
            source: String::from_str(env, "MockOracle"),
            is_twap: false,
            window_seconds: 0,
        }
    }

    fn get_twap(&self, env: &Env, _asset_a: String, _asset_b: String, window_seconds: u64) -> Price {
        if self.should_fail {
            panic_with_error!(env, OracleError::OracleUnavailable);
        }

        Price {
            value: self.price_value,
            timestamp: self.price_timestamp,
            source: String::from_str(env, "MockOracle"),
            is_twap: true,
            window_seconds,
        }
    }

    fn is_fresh(&self, _env: &Env, _asset_a: String, _asset_b: String, _staleness_threshold: u64) -> bool {
        self.is_fresh_result
    }
}

use shared::oracle::{OracleError, Price, PriceOracle};
use soroban_sdk::{panic_with_error, Env, String};

/// Oracle manager for the multi-currency wallet
pub struct OracleManager {
    oracle: Box<dyn PriceOracle>,
    staleness_threshold: u64,
    max_deviation_bps: i128,
}

impl OracleManager {
    pub fn new(
        oracle: Box<dyn PriceOracle>,
        staleness_threshold: u64,
        max_deviation_bps: i128,
    ) -> Self {
        Self {
            oracle,
            staleness_threshold,
            max_deviation_bps,
        }
    }

    /// Get a validated price from the oracle
    ///
    /// This function:
    /// 1. Gets the current price from the oracle
    /// 2. Checks for staleness
    /// 3. Gets the TWAP and checks for manipulation
    pub fn get_validated_price(
        &self,
        env: &Env,
        asset_a: String,
        asset_b: String,
    ) -> Result<Price, OracleError> {
        // 1. Get the current price
        let price = self.oracle.get_price(env, asset_a.clone(), asset_b.clone());

        // 2. Check for staleness
        let current_time = env.ledger().timestamp();
        if current_time.saturating_sub(price.timestamp) > self.staleness_threshold {
            return Err(OracleError::PriceStale);
        }

        // 3. Get the TWAP for manipulation resistance
        let twap = self.oracle.get_twap(env, asset_a, asset_b, 300); // 5-minute window

        // 4. Check deviation
        let deviation = self.calculate_deviation(price.value, twap.value);
        if deviation > self.max_deviation_bps {
            return Err(OracleError::PriceDeviationExceeded);
        }

        // 5. Check for manipulation (sudden spikes)
        if self.is_manipulated(price.value, twap.value) {
            return Err(OracleError::PriceManipulationDetected);
        }

        Ok(price)
    }

    /// Calculate the deviation between two prices in basis points
    fn calculate_deviation(&self, price: i128, twap: i128) -> i128 {
        if twap == 0 {
            return 0;
        }
        let diff = (price - twap).abs();
        (diff * 10_000) / twap
    }

    /// Check if the price is manipulated (sudden extreme spike)
    fn is_manipulated(&self, price: i128, twap: i128) -> bool {
        // If the price deviates more than 50% from TWAP, it's likely manipulated
        let deviation = self.calculate_deviation(price, twap);
        deviation > 5_000 // 50% in basis points
    }

    /// Check if the oracle data is fresh
    pub fn is_fresh(&self, env: &Env, asset_a: String, asset_b: String) -> bool {
        self.oracle
            .is_fresh(env, asset_a, asset_b, self.staleness_threshold)
    }
}

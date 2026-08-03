extern crate alloc;

use soroban_sdk::{Env, String};

/// Oracle price feed interface
///
/// This trait defines the interface for any price oracle provider.
/// It allows the system to swap oracle providers without changing core logic.
pub trait PriceOracle {
    /// Get the current price for a given asset pair
    ///
    /// # Arguments
    /// * `asset_a` - The base asset
    /// * `asset_b` - The quote asset
    ///
    /// # Returns
    /// * `Price` - The current price with metadata
    fn get_price(&self, env: &Env, asset_a: String, asset_b: String) -> Price;

    /// Get the Time-Weighted Average Price (TWAP)
    ///
    /// # Arguments
    /// * `asset_a` - The base asset
    /// * `asset_b` - The quote asset
    /// * `window_seconds` - The time window for TWAP calculation
    ///
    /// # Returns
    /// * `Price` - The TWAP price with metadata
    fn get_twap(&self, env: &Env, asset_a: String, asset_b: String, window_seconds: u64) -> Price;

    /// Check if the oracle has fresh data
    ///
    /// # Arguments
    /// * `asset_a` - The base asset
    /// * `asset_b` - The quote asset
    /// * `staleness_threshold` - Maximum acceptable age in seconds
    ///
    /// # Returns
    /// * `bool` - True if the data is fresh
    fn is_fresh(
        &self,
        env: &Env,
        asset_a: String,
        asset_b: String,
        staleness_threshold: u64,
    ) -> bool;
}

/// Price data structure
#[derive(Clone, Debug)]
pub struct Price {
    /// The price value (fixed-point, 7 decimals)
    pub value: i128,
    /// The timestamp of the price update
    pub timestamp: u64,
    /// The source of the price
    pub source: String,
    /// Whether the price is a TWAP
    pub is_twap: bool,
    /// The window used for TWAP (if applicable)
    pub window_seconds: u64,
}

/// Oracle error types
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OracleError {
    PriceNotFound = 1,
    PriceStale = 2,
    PriceDeviationExceeded = 3,
    PriceManipulationDetected = 4,
    OracleUnavailable = 5,
    InvalidAssetPair = 6,
}

impl From<OracleError> for soroban_sdk::Error {
    fn from(error: OracleError) -> Self {
        soroban_sdk::Error::from_contract_error(error as u32)
    }
}

/// Helper function to convert a price to a human-readable string
pub fn format_price(price: &Price) -> String {
    let value = price.value;
    let integer = value / 10_000_000;
    let fractional = value % 10_000_000;
    let formatted = alloc::format!("{}.{:07}", integer, fractional);
    String::from_str(&Env::default(), &formatted)
}

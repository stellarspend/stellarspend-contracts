//! Oracle price-feed abstractions for StellarSpend contracts.
//!
//! Defines the [`PriceOracle`] trait so contracts can read asset prices from
//! different providers without changing their core logic.

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
    /// No price is available for the requested asset pair.
    PriceNotFound = 1,
    /// The latest price is older than the requested staleness threshold.
    PriceStale = 2,
    /// The price moved further than the allowed deviation in a single update.
    PriceDeviationExceeded = 3,
    /// The update was rejected because it looked like price manipulation.
    PriceManipulationDetected = 4,
    /// The configured oracle could not be reached or did not respond.
    OracleUnavailable = 5,
    /// The asset pair is not supported by the configured oracle.
    InvalidAssetPair = 6,
}

impl From<OracleError> for soroban_sdk::Error {
    fn from(error: OracleError) -> Self {
        soroban_sdk::Error::from_contract_error(error as u32)
    }
}

/// Helper function to convert a price to a human-readable string
///
/// Formats the fixed-point `value` (7 decimals) as a decimal string such as
/// `"1.5000000"`.
///
/// # Arguments
/// * `price` - The price to format
///
/// # Returns
/// * `String` - The human-readable representation of the price value
pub fn format_price(price: &Price) -> String {
    let value = price.value;
    let integer = value / 10_000_000;
    let fractional = value % 10_000_000;
    let formatted = alloc::format!("{}.{:07}", integer, fractional);
    String::from_str(&Env::default(), &formatted)
}

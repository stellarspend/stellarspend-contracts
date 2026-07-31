use soroban_sdk::{contracttype, Symbol};

#[contracttype]
pub enum DataKey {
    /// Stored conversion rate keyed by `(from, to)` currency symbols.
    Rate(Symbol, Symbol),
}

#[contracttype]
#[derive(Clone)]
pub struct ConversionRate {
    pub from_currency: soroban_sdk::String,
    pub to_currency: soroban_sdk::String,
    pub rate_numerator: i128,
    pub rate_denominator: i128,
}

#![allow(unused)]
use soroban_sdk::{contract, contractimpl, Env, String, Symbol, Vec};

pub mod math;
pub mod types;
pub use types::*;

#[contract]
pub struct CurrencyConversionContract;

#[contractimpl]
impl CurrencyConversionContract {
    pub fn convert(env: Env, amount: i128, rate: ConversionRate) -> i128 {
        let to_amount = math::convert_amount(amount, rate.rate_numerator, rate.rate_denominator);
        env.events()
            .publish(("conversion_performed", rate.from_currency), to_amount);
        to_amount
    }

    pub fn normalize(
        env: Env,
        balances: Vec<(String, i128)>,
        rates: Vec<ConversionRate>,
        base_currency: String,
    ) -> i128 {
        math::normalize_balances(balances, rates, base_currency)
    }

    /// Returns the stored conversion rate for a currency pair.
    ///
    /// Returns `0` when no rate is stored for `(from, to)`.
    pub fn get_conversion_rate(env: Env, from: Symbol, to: Symbol) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Rate(from, to))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod test;

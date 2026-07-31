#![cfg(test)]

use super::*;
use soroban_sdk::{symbol_short, Env};

#[test]
fn get_conversion_rate_returns_stored_rate() {
    let env = Env::default();
    let contract_id = env.register(CurrencyConversionContract, ());
    let client = CurrencyConversionContractClient::new(&env, &contract_id);

    let from = symbol_short!("USD");
    let to = symbol_short!("XLM");
    let stored_rate: i128 = 105;

    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::Rate(from.clone(), to.clone()), &stored_rate);
    });

    assert_eq!(client.get_conversion_rate(&from, &to), stored_rate);
}

#[test]
fn get_conversion_rate_returns_zero_for_unknown_pair() {
    let env = Env::default();
    let contract_id = env.register(CurrencyConversionContract, ());
    let client = CurrencyConversionContractClient::new(&env, &contract_id);

    let from = symbol_short!("EUR");
    let to = symbol_short!("GBP");

    // Documented default: unknown pairs return 0.
    assert_eq!(client.get_conversion_rate(&from, &to), 0);
}

//! Asset conversion contract for Stellar assets.

use soroban_sdk::{
    contract, contracterror, contractimpl, panic_with_error, Address, Env, Map, Symbol,
    symbol_short,
};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ConversionError {
    SameToken = 1,
    InvalidAmount = 2,
    RateNotFound = 3,
    // [SEC-CONV-01] Explicit overflow error instead of a generic &'static str.
    Overflow = 4,
    // [SEC-CONV-02] Division-by-zero guard.
    DivisionByZero = 5,
    // [SEC-CONV-03] Zero conversion result guard.
    ZeroResult = 6,
}

pub struct MockPriceOracle;

impl MockPriceOracle {
    /// Returns `(numerator, denominator)` representing the exchange rate
    /// `from → to`, i.e. `converted = amount * num / denom`.
    ///
    /// # Security
    /// - [SEC-CONV-02] Callers must validate that `denom != 0` before use.
    /// - Real implementations must replace this with an authenticated on-chain
    ///   oracle; unauthenticated off-chain price feeds are a manipulation vector.
    pub fn get_rate(_from: &Address, _to: &Address) -> Option<(u32, u32)> {
        // Mock: 1 from_token = 2 to_token
        Some((2, 1))
    }
}

#[contract]
pub struct ConversionContract;

#[contractimpl]
impl ConversionContract {
    /// Converts `amount` of `from_token` to `to_token` using the oracle rate.
    ///
    /// Returns the converted amount (truncated, not rounded).
    ///
    /// # Security
    /// - [SEC-CONV-04] `user.require_auth()` ensures only the token owner can
    ///   initiate a conversion; previously the caller was unchecked.
    /// - [SEC-CONV-01] Overflow on `amount * numerator` surfaces as a typed
    ///   `Overflow` error rather than a panic string.
    /// - [SEC-CONV-02] A zero denominator from the oracle triggers
    ///   `DivisionByZero` before the division is attempted.
    /// - [SEC-CONV-03] A zero conversion result is rejected; it indicates an
    ///   amount too small for the current rate, preventing dust-drain attacks.
    /// - Same-token conversion and non-positive amounts are rejected up front.
    pub fn convert_assets(
        env: Env,
        user: Address,
        from_token: Address,
        to_token: Address,
        amount: i128,
    ) -> Result<i128, ConversionError> {
        // [SEC-CONV-04] Authenticate the initiating user.
        user.require_auth();

        if from_token == to_token {
            return Err(ConversionError::SameToken);
        }
        if amount <= 0 {
            return Err(ConversionError::InvalidAmount);
        }

        let (num, denom) =
            MockPriceOracle::get_rate(&from_token, &to_token).ok_or(ConversionError::RateNotFound)?;

        // [SEC-CONV-02] Guard against a zero denominator from the oracle.
        if denom == 0 {
            return Err(ConversionError::DivisionByZero);
        }

        // [SEC-CONV-01] Checked multiplication before dividing.
        let numerator_product = amount
            .checked_mul(num as i128)
            .ok_or(ConversionError::Overflow)?;

        let converted = numerator_product
            .checked_div(denom as i128)
            .ok_or(ConversionError::DivisionByZero)?;

        // [SEC-CONV-03] Reject dust conversions that round to zero.
        if converted == 0 {
            return Err(ConversionError::ZeroResult);
        }

        let mut counts: Map<Address, u32> = env
            .storage()
            .instance()
            .get(&CONVERSION_HISTORY_COUNT_KEY)
            .unwrap_or_else(|| Map::new(&env));
        let current_count = counts.get(user.clone()).unwrap_or(0);
        counts.set(user.clone(), current_count + 1);
        env.storage().instance().set(&CONVERSION_HISTORY_COUNT_KEY, &counts);

        env.events().publish(
            (
                symbol_short!("convert"),
                user.clone(),
            ),
            (
                from_token.clone(),
                to_token.clone(),
                amount,
                converted,
                env.ledger().timestamp(),
            ),
        );

        Ok(converted)
    }

    pub fn get_conversion_history_count(env: Env, owner: Address) -> u32 {
        let counts: Map<Address, u32> = env
            .storage()
            .instance()
            .get(&CONVERSION_HISTORY_COUNT_KEY)
            .unwrap_or_else(|| Map::new(&env));
        counts.get(owner).unwrap_or(0)
    }
}

const CONVERSION_HISTORY_COUNT_KEY: Symbol = symbol_short!("conv_hist");

#[cfg(test)]
mod test {
    use super::{ConversionContract, ConversionContractClient};
    use soroban_sdk::{testutils::Address as _, Address, Env};

    fn setup_contract() -> (Env, ConversionContractClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(ConversionContract, ());
        let client = ConversionContractClient::new(&env, &contract_id);
        (env, client)
    }

    #[test]
    fn get_conversion_history_count_tracks_successful_conversions() {
        let (env, client) = setup_contract();
        let owner = Address::generate(&env);
        let other_owner = Address::generate(&env);
        let from_token = Address::generate(&env);
        let to_token = Address::generate(&env);

        assert_eq!(client.get_conversion_history_count(&owner), 0);
        assert_eq!(client.get_conversion_history_count(&other_owner), 0);

        let first_conversion = client
            .convert_assets(&owner, &from_token, &to_token, &10)
            .unwrap();
        assert_eq!(first_conversion, 20);
        assert_eq!(client.get_conversion_history_count(&owner), 1);
        assert_eq!(client.get_conversion_history_count(&other_owner), 0);

        let second_conversion = client
            .convert_assets(&owner, &from_token, &to_token, &5)
            .unwrap();
        assert_eq!(second_conversion, 10);
        assert_eq!(client.get_conversion_history_count(&owner), 2);
    }
}
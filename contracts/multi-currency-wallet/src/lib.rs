#![no_std]
mod oracle;

use oracle::OracleManager;
use shared::oracle::{format_price, OracleError, Price};
use shared::reflector_oracle::ReflectorOracle;
use soroban_sdk::{contract, contracttype, panic_with_error, Address, Env, String};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrencyWallet {
    pub owner: Address,
    pub balances: Vec<(String, i128)>,
    pub oracle_address: Address,
    pub staleness_threshold: u64,
    pub max_deviation_bps: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversionRequest {
    pub from_asset: String,
    pub to_asset: String,
    pub amount: i128,
    pub min_received: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversionResult {
    pub from_amount: i128,
    pub to_amount: i128,
    pub rate: i128,
    pub timestamp: u64,
}

#[contract]
pub struct MultiCurrencyWallet;

#[contractimpl]
impl MultiCurrencyWallet {
    /// Initialize the wallet with oracle configuration
    pub fn initialize(
        env: Env,
        owner: Address,
        oracle_address: Address,
        staleness_threshold: u64,
        max_deviation_bps: i128,
    ) {
        let wallet = CurrencyWallet {
            owner: owner.clone(),
            balances: Vec::new(&env),
            oracle_address: oracle_address.clone(),
            staleness_threshold,
            max_deviation_bps,
        };

        env.storage()
            .set(&String::from_str(&env, "wallet"), &wallet);
    }

    /// Add a balance to the wallet
    pub fn add_balance(env: Env, asset: String, amount: i128) {
        let mut wallet: CurrencyWallet = env
            .storage()
            .get(&String::from_str(&env, "wallet"))
            .unwrap_or_else(|| panic!("Wallet not initialized"));

        // Update balance
        let mut found = false;
        for (i, (existing_asset, existing_amount)) in wallet.balances.iter().enumerate() {
            if *existing_asset == asset {
                let new_amount = existing_amount + amount;
                wallet.balances.set(i, (asset.clone(), new_amount));
                found = true;
                break;
            }
        }

        if !found {
            wallet.balances.push((asset, amount));
        }

        env.storage()
            .set(&String::from_str(&env, "wallet"), &wallet);
    }

    /// Convert currency using oracle rate
    pub fn convert_currency(env: Env, request: ConversionRequest) -> ConversionResult {
        // 1. Get the wallet
        let wallet: CurrencyWallet = env
            .storage()
            .get(&String::from_str(&env, "wallet"))
            .unwrap_or_else(|| panic!("Wallet not initialized"));

        // 2. Create the oracle manager
        let oracle = ReflectorOracle::new(wallet.oracle_address.clone());
        let oracle_manager = OracleManager::new(
            Box::new(oracle),
            wallet.staleness_threshold,
            wallet.max_deviation_bps,
        );

        // 3. Get validated price from oracle
        let price = oracle_manager
            .get_validated_price(&env, request.from_asset.clone(), request.to_asset.clone())
            .unwrap_or_else(|e| {
                panic_with_error!(env, e);
            });

        // 4. Calculate conversion
        let to_amount = (request.amount * price.value) / 10_000_000;

        // 5. Check minimum received
        if to_amount < request.min_received {
            panic!("Minimum received not met");
        }

        // 6. Update balances
        Self::update_balances(
            &env,
            request.from_asset,
            request.to_asset,
            request.amount,
            to_amount,
        );

        // 7. Return result
        ConversionResult {
            from_amount: request.amount,
            to_amount,
            rate: price.value,
            timestamp: env.ledger().timestamp(),
        }
    }

    /// Update balances after conversion
    fn update_balances(
        env: &Env,
        from_asset: String,
        to_asset: String,
        from_amount: i128,
        to_amount: i128,
    ) {
        let mut wallet: CurrencyWallet = env
            .storage()
            .get(&String::from_str(&env, "wallet"))
            .unwrap();

        // Subtract from balance
        for (i, (asset, amount)) in wallet.balances.iter().enumerate() {
            if *asset == from_asset {
                let new_amount = amount - from_amount;
                wallet.balances.set(i, (from_asset.clone(), new_amount));
                break;
            }
        }

        // Add to balance
        let mut found = false;
        for (i, (asset, amount)) in wallet.balances.iter().enumerate() {
            if *asset == to_asset {
                let new_amount = amount + to_amount;
                wallet.balances.set(i, (to_asset.clone(), new_amount));
                found = true;
                break;
            }
        }

        if !found {
            wallet.balances.push((to_asset, to_amount));
        }

        env.storage()
            .set(&String::from_str(&env, "wallet"), &wallet);
    }

    /// Get wallet balance
    pub fn get_balance(env: Env, asset: String) -> i128 {
        let wallet: CurrencyWallet = env
            .storage()
            .get(&String::from_str(&env, "wallet"))
            .unwrap_or_else(|| panic!("Wallet not initialized"));

        for (existing_asset, amount) in wallet.balances.iter() {
            if *existing_asset == asset {
                return *amount;
            }
        }

        0
    }

    /// Check oracle freshness
    pub fn is_oracle_fresh(env: Env, asset_a: String, asset_b: String) -> bool {
        let wallet: CurrencyWallet = env
            .storage()
            .get(&String::from_str(&env, "wallet"))
            .unwrap_or_else(|| panic!("Wallet not initialized"));

        let oracle = ReflectorOracle::new(wallet.oracle_address.clone());
        let oracle_manager = OracleManager::new(
            Box::new(oracle),
            wallet.staleness_threshold,
            wallet.max_deviation_bps,
        );

        oracle_manager.is_fresh(&env, asset_a, asset_b)
    }
}

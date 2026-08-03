//! # On-Chain Fraud Detection Contract
#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Vec};

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    RiskScore(Address),
    TransferTimestamps(Address),
    Blacklist(Address),
    Whitelist(Address),
    FlaggedTx(u64),
}

#[contract]
pub struct FraudDetectionContract;

#[contractimpl]
impl FraudDetectionContract {
    /// Evaluates risk score based on transaction velocity and flags suspicious activity.
    pub fn record_transaction(env: Env, user: Address, tx_id: u64, amount: i128) -> u32 {
        let is_blacklisted: bool = env
            .storage()
            .instance()
            .get(&DataKey::Blacklist(user.clone()))
            .unwrap_or(false);

        if is_blacklisted {
            env.storage()
                .instance()
                .set(&DataKey::RiskScore(user.clone()), &100u32);
            env.storage()
                .instance()
                .set(&DataKey::FlaggedTx(tx_id), &true);
            return 100;
        }

        let is_whitelisted: bool = env
            .storage()
            .instance()
            .get(&DataKey::Whitelist(user.clone()))
            .unwrap_or(false);

        if is_whitelisted {
            return 0;
        }

        let now = env.ledger().timestamp();
        let mut timestamps: Vec<u64> = env
            .storage()
            .instance()
            .get(&DataKey::TransferTimestamps(user.clone()))
            .unwrap_or(Vec::new(&env));

        // Keep last 10 timestamps (bounded rolling window for predictable CPU cost)
        timestamps.push_back(now);
        while timestamps.len() > 10 {
            timestamps.pop_front();
        }

        env.storage()
            .instance()
            .set(&DataKey::TransferTimestamps(user.clone()), &timestamps);

        // Velocity threshold check: >= 5 transfers within 60 seconds
        let window_start = if now > 60 { now - 60 } else { 0 };
        let mut recent_count = 0u32;

        for ts in timestamps.iter() {
            if ts >= window_start {
                recent_count += 1;
            }
        }

        let mut score = 0u32;
        if recent_count >= 5 {
            score = 80;
            env.storage()
                .instance()
                .set(&DataKey::FlaggedTx(tx_id), &true);
        }

        env.storage()
            .instance()
            .set(&DataKey::RiskScore(user.clone()), &score);
        score
    }

    /// Gets current risk score for a user (0-100).
    pub fn get_risk_score(env: Env, user: Address) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::RiskScore(user))
            .unwrap_or(0)
    }

    /// Checks if a transaction was flagged.
    pub fn is_transaction_flagged(env: Env, tx_id: u64) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::FlaggedTx(tx_id))
            .unwrap_or(false)
    }

    /// Sets blacklist status for an address.
    pub fn set_blacklisted(env: Env, admin: Address, user: Address, blacklisted: bool) {
        admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::Blacklist(user), &blacklisted);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::Env;

    #[test]
    fn test_burst_transfers_raises_risk_score() {
        let env = Env::default();
        let contract_id = env.register(FraudDetectionContract, ());
        let client = FraudDetectionContractClient::new(&env, &contract_id);

        let user = Address::generate(&env);

        // Simulate 5 transfers in rapid succession
        for i in 1..=5 {
            client.record_transaction(&user, &i, &100);
        }

        let score = client.get_risk_score(&user);
        assert_eq!(score, 80);
        assert!(client.is_transaction_flagged(&5));
    }
}

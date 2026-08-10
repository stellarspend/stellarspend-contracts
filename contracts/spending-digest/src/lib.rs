//! # Spending Digest Contract
#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    TotalSpent(Address),
    PeriodCount(Address),
    LastPeriodSpent(Address),
}

#[contract]
pub struct SpendingDigestContract;

#[contractimpl]
impl SpendingDigestContract {
    /// Records a spending entry for an owner.
    pub fn record_spending(env: Env, owner: Address, amount: i128) {
        owner.require_auth();

        let total: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalSpent(owner.clone()))
            .unwrap_or(0);
        let count: i128 = env
            .storage()
            .instance()
            .get(&DataKey::PeriodCount(owner.clone()))
            .unwrap_or(0);

        let new_total = total.checked_add(amount).unwrap_or(total);
        let new_count = count + 1;

        env.storage()
            .instance()
            .set(&DataKey::TotalSpent(owner.clone()), &new_total);
        env.storage()
            .instance()
            .set(&DataKey::PeriodCount(owner.clone()), &new_count);
        env.storage()
            .instance()
            .set(&DataKey::LastPeriodSpent(owner.clone()), &amount);
    }

    /// Returns (total_spent, average_per_period, last_period_spent) for an owner address.
    /// Returns (0, 0, 0) for address with no history.
    pub fn get_spending_digest_summary(env: Env, owner: Address) -> (i128, i128, i128) {
        let total_spent: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalSpent(owner.clone()))
            .unwrap_or(0);

        let period_count: i128 = env
            .storage()
            .instance()
            .get(&DataKey::PeriodCount(owner.clone()))
            .unwrap_or(0);

        let last_period_spent: i128 = env
            .storage()
            .instance()
            .get(&DataKey::LastPeriodSpent(owner.clone()))
            .unwrap_or(0);

        let average_per_period = if period_count > 0 {
            total_spent / period_count
        } else {
            0
        };

        (total_spent, average_per_period, last_period_spent)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::Env;

    #[test]
    fn test_spending_digest_summary_no_history() {
        let env = Env::default();
        let contract_id = env.register(SpendingDigestContract, ());
        let client = SpendingDigestContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let summary = client.get_spending_digest_summary(&owner);

        assert_eq!(summary, (0, 0, 0));
    }

    #[test]
    fn test_spending_digest_summary_with_history() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(SpendingDigestContract, ());
        let client = SpendingDigestContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        client.record_spending(&owner, &100);
        client.record_spending(&owner, &200);

        let summary = client.get_spending_digest_summary(&owner);
        assert_eq!(summary, (300, 150, 200));
    }
}

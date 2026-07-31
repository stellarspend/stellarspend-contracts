use soroban_sdk::{Address, Env};
use crate::PenaltyDataKey;

pub fn get_penalty_amount(env: &Env, owner: Address) -> i128 {
    env.storage()
        .persistent()
        .get(&PenaltyDataKey::AccumulatedPenalty(owner))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn test_get_penalty_amount_no_penalties() {
        let env = Env::default();
        let owner = Address::generate(&env);
        
        let amount = get_penalty_amount(&env, owner);
        assert_eq!(amount, 0);
    }

    #[test]
    fn test_get_penalty_amount_with_penalties() {
        let env = Env::default();
        let owner = Address::generate(&env);
        
        env.storage().persistent().set(&PenaltyDataKey::AccumulatedPenalty(owner.clone()), &500_i128);
        
        let amount = get_penalty_amount(&env, owner);
        assert_eq!(amount, 500);
    }
}

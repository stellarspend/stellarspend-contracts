#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, Address, Env,
};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum PenaltyError {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
}

#[contracttype]
#[derive(Clone, Debug)]
pub enum PenaltyDataKey {
    Admin,
    PenaltyPercent,
    Treasury,
}

fn bump_instance(env: &Env) {
    env.storage().instance().extend_ttl(5000, 5000);
}

#[contract]
pub struct PenaltyContract;

#[contractimpl]
impl PenaltyContract {
    pub fn initialize(env: Env, admin: Address, penalty_percent: u32, treasury: Address) {
        if env.storage().instance().has(&PenaltyDataKey::Admin) {
            panic_with_error!(&env, PenaltyError::AlreadyInitialized);
        }
        env.storage().instance().set(&PenaltyDataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&PenaltyDataKey::PenaltyPercent, &penalty_percent);
        env.storage()
            .instance()
            .set(&PenaltyDataKey::Treasury, &treasury);
        bump_instance(&env);
    }

    pub fn set_penalty_percent(env: Env, caller: Address, percent: u32) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&PenaltyDataKey::Admin)
            .expect("not initialized");
        if caller != admin {
            panic_with_error!(&env, PenaltyError::Unauthorized);
        }
        caller.require_auth();
        env.storage()
            .instance()
            .set(&PenaltyDataKey::PenaltyPercent, &percent);
        bump_instance(&env);
    }

    pub fn get_penalty_percent(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&PenaltyDataKey::PenaltyPercent)
            .unwrap_or(10)
    }

    pub fn set_treasury(env: Env, caller: Address, treasury: Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&PenaltyDataKey::Admin)
            .expect("not initialized");
        if caller != admin {
            panic_with_error!(&env, PenaltyError::Unauthorized);
        }
        caller.require_auth();
        env.storage()
            .instance()
            .set(&PenaltyDataKey::Treasury, &treasury);
        bump_instance(&env);
    }

    pub fn get_treasury(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&PenaltyDataKey::Treasury)
            .expect("treasury not set")
    }

    /// Calculate penalty fee using stored penalty percent (as percentage, e.g., 10 = 10%).
    pub fn calculate_penalty_fee(env: Env, amount: i128) -> i128 {
        let percent = Self::get_penalty_percent(env);
        amount * percent as i128 / 100
    }

    /// Calculate penalty fee using provided basis points (e.g., 1000 = 10%).
    pub fn calculate_penalty_fee_with_bps(_env: Env, amount: i128, bps: u32) -> i128 {
        amount * bps as i128 / 10_000
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn setup() -> (Env, Address, PenaltyContractClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(PenaltyContract, ());
        let client = PenaltyContractClient::new(&env, &contract_id);
        (env, contract_id, client)
    }

    fn init_default(env: &Env, client: &PenaltyContractClient<'static>) -> (Address, Address) {
        let admin = Address::generate(env);
        let treasury = Address::generate(env);
        client.initialize(&admin, &10, &treasury);
        (admin, treasury)
    }

    #[test]
    fn test_initialize() {
        let (env, _, client) = setup();
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.initialize(&admin, &10, &treasury);
        assert_eq!(client.get_penalty_percent(), 10);
        assert_eq!(client.get_treasury(), treasury);
    }

    #[test]
    fn test_initialize_twice_fails() {
        let (env, _, client) = setup();
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.initialize(&admin, &10, &treasury);
        let result = client.try_initialize(&admin, &20, &treasury);
        assert!(result.is_err());
    }

    #[test]
    fn test_set_penalty_percent() {
        let (env, _, client) = setup();
        let (admin, _) = init_default(&env, &client);
        client.set_penalty_percent(&admin, &20);
        assert_eq!(client.get_penalty_percent(), 20);
    }

    #[test]
    fn test_unauthorized_set_penalty_percent_rejected() {
        let (env, _, client) = setup();
        let (_, _) = init_default(&env, &client);
        let attacker = Address::generate(&env);
        let result = client.try_set_penalty_percent(&attacker, &20);
        assert!(result.is_err());
    }

    #[test]
    fn test_set_treasury() {
        let (env, _, client) = setup();
        let (admin, _) = init_default(&env, &client);
        let new_treasury = Address::generate(&env);
        client.set_treasury(&admin, &new_treasury);
        assert_eq!(client.get_treasury(), new_treasury);
    }

    #[test]
    fn test_unauthorized_set_treasury_rejected() {
        let (env, _, client) = setup();
        let (_, _) = init_default(&env, &client);
        let attacker = Address::generate(&env);
        let new_treasury = Address::generate(&env);
        let result = client.try_set_treasury(&attacker, &new_treasury);
        assert!(result.is_err());
    }

    #[test]
    fn test_calculate_penalty_fee_default_percent() {
        let (env, _, client) = setup();
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.initialize(&admin, &10, &treasury);

        let penalty = client.calculate_penalty_fee(&1_000);
        assert_eq!(penalty, 100);
    }

    #[test]
    fn test_calculate_penalty_fee_zero_amount() {
        let (env, _, client) = setup();
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.initialize(&admin, &10, &treasury);

        let penalty = client.calculate_penalty_fee(&0);
        assert_eq!(penalty, 0);
    }

    #[test]
    fn test_calculate_penalty_fee_custom_percent() {
        let (env, _, client) = setup();
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.initialize(&admin, &25, &treasury);

        let penalty = client.calculate_penalty_fee(&2_000);
        assert_eq!(penalty, 500);
    }

    #[test]
    fn test_calculate_penalty_fee_with_bps() {
        let (env, _, client) = setup();
        let (_, _) = init_default(&env, &client);

        let penalty = client.calculate_penalty_fee_with_bps(&1_000, &1_000);
        assert_eq!(penalty, 100);
    }

    #[test]
    fn test_calculate_penalty_fee_with_bps_zero_bps() {
        let (env, _, client) = setup();
        let (_, _) = init_default(&env, &client);

        let penalty = client.calculate_penalty_fee_with_bps(&1_000, &0);
        assert_eq!(penalty, 0);
    }

    #[test]
    fn test_penalty_cannot_exceed_principal() {
        let (env, _, client) = setup();
        let (_, _) = init_default(&env, &client);

        let penalty = client.calculate_penalty_fee_with_bps(&500, &10_000);
        assert!(penalty <= 500);
    }

    #[test]
    fn test_multiple_withdrawal_timing_combinations() {
        let (env, _, client) = setup();
        let (_, _) = init_default(&env, &client);

        let test_cases = [
            (1_000_000, 500, 50_000),
            (1_000_000, 1_000, 100_000),
            (1_000_000, 2_000, 200_000),
            (500_000, 500, 25_000),
            (500_000, 1_000, 50_000),
            (100_000, 1_500, 15_000),
        ];

        for (amount, bps, expected) in test_cases {
            let penalty = client.calculate_penalty_fee_with_bps(&amount, &bps);
            assert_eq!(
                penalty, expected,
                "penalty for amount={} bps={} expected={} got={}",
                amount, bps, expected, penalty
            );
        }
    }
}

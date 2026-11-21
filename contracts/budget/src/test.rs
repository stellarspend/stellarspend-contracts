#[cfg(test)]
mod tests {
    use crate::{Contract, ContractClient, Error};
    use soroban_sdk::{testutils::Address as _, Address, Env, Symbol};

    fn setup(env: &Env) -> ContractClient<'static> {
        env.mock_all_auths();
        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        client.initialize(&admin);
        client
    }

    fn category(env: &Env) -> Symbol {
        Symbol::new(env, "groceries")
    }

    fn xlm(env: &Env) -> Symbol {
        Symbol::new(env, "XLM")
    }

    #[test]
    fn create_budget_returns_incrementing_ids() {
        let env = Env::default();
        let client = setup(&env);
        let user = Address::generate(&env);
        let cat = category(&env);
        let asset = xlm(&env);

        let id1 = client.create_budget(
            &user,
            &Symbol::new(&env, "food"),
            &500_i128,
            &cat,
            &asset,
            &0_u64,
            &1000_u64,
        );
        let id2 = client.create_budget(
            &user,
            &Symbol::new(&env, "fun"),
            &200_i128,
            &cat,
            &asset,
            &0_u64,
            &1000_u64,
        );

        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
    }

    #[test]
    fn get_budgets_returns_all_budgets_for_user() {
        let env = Env::default();
        let client = setup(&env);
        let user = Address::generate(&env);
        let cat = category(&env);
        let asset = xlm(&env);
        client.create_budget(
            &user,
            &Symbol::new(&env, "food"),
            &500_i128,
            &cat,
            &asset,
            &0_u64,
            &1000_u64,
        );
        client.create_budget(
            &user,
            &Symbol::new(&env, "fun"),
            &200_i128,
            &cat,
            &asset,
            &0_u64,
            &1000_u64,
        );

        let budgets = client.get_budgets(&user);
        assert_eq!(budgets.len(), 2);
    }

    #[test]
    fn get_budgets_is_empty_for_a_user_with_no_budgets() {
        let env = Env::default();
        let client = setup(&env);
        let user = Address::generate(&env);

        let budgets = client.get_budgets(&user);
        assert_eq!(budgets.len(), 0);
    }

    #[test]
    fn update_budget_persists_new_amount() {
        let env = Env::default();
        let client = setup(&env);
        let user = Address::generate(&env);
        let cat = category(&env);
        let asset = xlm(&env);
        let id = client.create_budget(
            &user,
            &Symbol::new(&env, "food"),
            &500_i128,
            &cat,
            &asset,
            &0_u64,
            &1000_u64,
        );

        client.update_budget(&user, &id, &750_i128);

        let budgets = client.get_budgets(&user);
        assert_eq!(budgets.get(0).unwrap().amount, 750);
    }

    #[test]
    fn delete_budget_removes_it_from_get_budgets() {
        let env = Env::default();
        let client = setup(&env);
        let user = Address::generate(&env);
        let cat = category(&env);
        let asset = xlm(&env);
        let id1 = client.create_budget(
            &user,
            &Symbol::new(&env, "food"),
            &500_i128,
            &cat,
            &asset,
            &0_u64,
            &1000_u64,
        );
        let id2 = client.create_budget(
            &user,
            &Symbol::new(&env, "fun"),
            &200_i128,
            &cat,
            &asset,
            &0_u64,
            &1000_u64,
        );

        client.delete_budget(&user, &id1);

        let budgets = client.get_budgets(&user);
        assert_eq!(budgets.len(), 1);
        assert_eq!(budgets.get(0).unwrap().budget_id, id2);
    }

    #[test]
    fn update_budget_rejects_non_owner() {
        let env = Env::default();
        let client = setup(&env);
        let user = Address::generate(&env);
        let other = Address::generate(&env);
        let cat = category(&env);
        let asset = xlm(&env);
        let id = client.create_budget(
            &user,
            &Symbol::new(&env, "food"),
            &500_i128,
            &cat,
            &asset,
            &0_u64,
            &1000_u64,
        );

        let result = client.try_update_budget(&other, &id, &750_i128);
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
        assert_eq!(client.get_budgets(&user).get(0).unwrap().amount, 500);
    }

    #[test]
    fn delete_budget_rejects_non_owner() {
        let env = Env::default();
        let client = setup(&env);
        let user = Address::generate(&env);
        let other = Address::generate(&env);
        let cat = category(&env);
        let asset = xlm(&env);
        let id = client.create_budget(
            &user,
            &Symbol::new(&env, "food"),
            &500_i128,
            &cat,
            &asset,
            &0_u64,
            &1000_u64,
        );

        let result = client.try_delete_budget(&other, &id);
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
        assert_eq!(client.get_budgets(&user).len(), 1);
    }

    #[test]
    fn create_budget_rejects_non_positive_amount() {
        let env = Env::default();
        let client = setup(&env);
        let user = Address::generate(&env);
        let cat = category(&env);
        let asset = xlm(&env);

        let result = client.try_create_budget(
            &user,
            &Symbol::new(&env, "food"),
            &0_i128,
            &cat,
            &asset,
            &0_u64,
            &1000_u64,
        );
        assert_eq!(result, Err(Ok(Error::InvalidAmount)));
    }

    #[test]
    fn create_budget_rejects_invalid_date_range() {
        let env = Env::default();
        let client = setup(&env);
        let user = Address::generate(&env);
        let cat = category(&env);
        let asset = xlm(&env);

        let result = client.try_create_budget(
            &user,
            &Symbol::new(&env, "food"),
            &500_i128,
            &cat,
            &asset,
            &1000_u64,
            &500_u64,
        );
        assert_eq!(result, Err(Ok(Error::InvalidDateRange)));
    }

    #[test]
    fn update_budget_rejects_unknown_budget() {
        let env = Env::default();
        let client = setup(&env);
        let user = Address::generate(&env);

        let result = client.try_update_budget(&user, &999_u64, &100_i128);
        assert_eq!(result, Err(Ok(Error::BudgetNotFound)));
    }

    #[test]
    fn delete_budget_rejects_unknown_budget() {
        let env = Env::default();
        let client = setup(&env);
        let user = Address::generate(&env);

        let result = client.try_delete_budget(&user, &999_u64);
        assert_eq!(result, Err(Ok(Error::BudgetNotFound)));
    }

    #[test]
    fn double_initialize_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let result = client.try_initialize(&admin);
        assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
    }

    #[test]
    fn initialize_then_create_budget_persists_amount() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let cat = category(&env);
        let asset = xlm(&env);

        client.initialize(&admin);

        let id = client.create_budget(
            &user,
            &Symbol::new(&env, "food"),
            &500_i128,
            &cat,
            &asset,
            &0_u64,
            &1000_u64,
        );

        let budgets = client.get_budgets(&user);
        assert_eq!(budgets.len(), 1);
        assert_eq!(budgets.get(0).unwrap().budget_id, id);
        assert_eq!(budgets.get(0).unwrap().amount, 500);
    }

    #[test]
    fn set_value_negative_returns_invalid_amount() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let result = client.try_set_value(&admin, &-1_i128);
        assert_eq!(result, Err(Ok(Error::InvalidAmount)));
    }

    #[test]
    fn get_value_returns_zero_after_initialize() {
        let env = Env::default();
        let client = setup(&env);
        assert_eq!(client.get_value(), 0);
    }
}

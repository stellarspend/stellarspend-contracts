#[cfg(test)]
mod tests {
    use crate::types::PolicyRule;
    use crate::{storage, Contract, ContractClient, Error};
    use soroban_sdk::{testutils::Address as _, vec, Address, Env, Symbol};

    fn setup<'a>() -> (Env, ContractClient<'a>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &contract_id);
        client.initialize(&admin);
        (env, client, admin)
    }

    fn rule(env: &Env, category: &str, limit: i128, zk_required_above: i128) -> PolicyRule {
        PolicyRule {
            category: Symbol::new(env, category),
            limit,
            zk_required_above,
        }
    }

    // ── Happy path (the issue's own scenario) ───────────────────────────

    #[test]
    fn set_and_get_policy_round_trips() {
        let (env, client, _admin) = setup();
        let user = Address::generate(&env);

        let rules = vec![
            &env,
            rule(&env, "Groceries", 500, 1_000),
            rule(&env, "Entertainment", 100, 200),
        ];
        client.set_policy(&user, &rules);

        assert_eq!(client.get_policy(&user), rules);
    }

    #[test]
    fn setting_new_rules_completely_replaces_the_old_set() {
        let (env, client, _admin) = setup();
        let user = Address::generate(&env);

        let first = vec![
            &env,
            rule(&env, "Groceries", 500, 1_000),
            rule(&env, "Entertainment", 100, 200),
        ];
        client.set_policy(&user, &first);
        assert_eq!(client.get_policy(&user).len(), 2);

        let second = vec![&env, rule(&env, "Rent", 2_000, 5_000)];
        client.set_policy(&user, &second);

        let stored = client.get_policy(&user);
        assert_eq!(stored, second);
        assert_eq!(stored.len(), 1);
        // The old "Groceries"/"Entertainment" rules are gone entirely, not merged.
        assert!(!stored
            .iter()
            .any(|r| r.category == Symbol::new(&env, "Groceries")));
    }

    #[test]
    fn get_policy_for_a_user_who_never_set_one_is_empty() {
        let (env, client, _admin) = setup();
        let user = Address::generate(&env);
        assert_eq!(client.get_policy(&user).len(), 0);
    }

    // ── Unauthorized caller ──────────────────────────────────────────────

    #[test]
    fn set_policy_requires_the_user_to_authorize() {
        let env = Env::default();
        // No mock_all_auths.
        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &contract_id);
        let user = Address::generate(&env);
        let rules = vec![&env, rule(&env, "Groceries", 500, 1_000)];

        let result = client.try_set_policy(&user, &rules);
        assert!(result.is_err());
    }

    // ── Boundary values ──────────────────────────────────────────────────

    #[test]
    fn set_policy_accepts_zero_limit_and_zero_zk_threshold() {
        let (env, client, _admin) = setup();
        let user = Address::generate(&env);
        // limit=0 ("block this category"), zk_required_above=0 ("always require a proof").
        let rules = vec![&env, rule(&env, "Gambling", 0, 0)];

        client.set_policy(&user, &rules);
        assert_eq!(client.get_policy(&user), rules);
    }

    #[test]
    fn set_policy_rejects_a_negative_limit() {
        let (env, client, _admin) = setup();
        let user = Address::generate(&env);
        let rules = vec![&env, rule(&env, "Groceries", -1, 1_000)];

        assert_eq!(
            client.try_set_policy(&user, &rules),
            Err(Ok(Error::InvalidAmount))
        );
    }

    #[test]
    fn set_policy_rejects_a_negative_zk_threshold() {
        let (env, client, _admin) = setup();
        let user = Address::generate(&env);
        let rules = vec![&env, rule(&env, "Groceries", 500, -1)];

        assert_eq!(
            client.try_set_policy(&user, &rules),
            Err(Ok(Error::InvalidAmount))
        );
    }

    #[test]
    fn set_policy_accepts_an_empty_rule_set() {
        let (env, client, _admin) = setup();
        let user = Address::generate(&env);

        // First set some rules, then explicitly clear them.
        client.set_policy(&user, &vec![&env, rule(&env, "Groceries", 500, 1_000)]);
        client.set_policy(&user, &vec![&env]);

        assert_eq!(client.get_policy(&user).len(), 0);
    }

    // ── Admin / initialize ───────────────────────────────────────────────

    #[test]
    fn initialize_twice_is_rejected() {
        let (_, client, admin) = setup();
        assert_eq!(
            client.try_initialize(&admin),
            Err(Ok(Error::AlreadyInitialized))
        );
    }

    // ── initialize persistence ────────────────────────────────────────────
    //
    // This crate has no `get_value`: the admin is the only thing initialize
    // writes. The ticket's "assert get_value returns 0" has no direct
    // counterpart here, so these tests assert the observable equivalent —
    // that initialize persists the admin and seeds no policy.

    #[test]
    fn initialize_persists_the_admin_and_leaves_policy_empty() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &contract_id);

        // Nothing is stored before initialize.
        assert_eq!(storage::read_admin(&env), None);

        // The contractimpl client unwraps Result, so a success is just `()`.
        assert_eq!(client.initialize(&admin), ());

        assert_eq!(storage::read_admin(&env), Some(admin.clone()));

        // initialize must not seed any policy, so an untouched user reads
        // back an empty rule set rather than an implicit default.
        assert_eq!(client.get_policy(&user).len(), 0);
    }

    #[test]
    fn initialize_requires_the_admin_to_authorize() {
        let env = Env::default();
        // Deliberately do NOT mock_all_auths — nothing has authorized anything.
        let admin = Address::generate(&env);
        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &contract_id);

        assert!(client.try_initialize(&admin).is_err());
        assert_eq!(storage::read_admin(&env), None);
    }

    #[test]
    fn rejected_reinitialize_does_not_hand_admin_over() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let other = Address::generate(&env);
        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &contract_id);
        client.initialize(&admin);

        assert_eq!(
            client.try_initialize(&other),
            Err(Ok(Error::AlreadyInitialized))
        );
        assert_eq!(storage::read_admin(&env), Some(admin));
    }
}

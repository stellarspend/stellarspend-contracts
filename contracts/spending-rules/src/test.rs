#[cfg(test)]
mod tests {
    use crate::{storage, Contract, ContractClient, Error};
    use soroban_sdk::{testutils::Address as _, Address, Bytes, Env, String as SdkString, Symbol};

    // ── Constants ────────────────────────────────────────────────────────────
    const XLM: &str = "XLM";

    // ── ZK-verifier test constants (mirrored from zk-verifier crate) ─────────
    const ULTRAHONK_MAGIC: [u8; 4] = [0x55, 0x48, 0x6e, 0x6b]; // "UHnk"
    const EXPECTED_PROOF_VERSION: u8 = 0x01;
    const MIN_PROOF_LENGTH: u32 = 4096;
    const VERIFYING_KEY_COMMITMENT: [u8; 32] = [
        0x3e, 0x8a, 0x1f, 0x5e, 0x9c, 0x2b, 0x8d, 0x4a, 0x6e, 0x3f, 0x0c, 0x7b, 0x5a, 0x9d, 0x1e,
        0x4f, 0x2c, 0x8b, 0x6a, 0x0d, 0x7e, 0x3f, 0x1c, 0x5b, 0x9a, 0x4d, 0x8e, 0x2f, 0x0a, 0x6c,
        0x1b, 0x7d,
    ];

    // ── Proof builder helpers (adapted from zk-verifier/src/test.rs) ─────────

    /// Builds a valid-looking proof for testing. The commitment is computed as
    /// SHA-256(proof_body || user_bytes || vk_commitment), which the verifier
    /// checks against the commitment embedded in the proof header.
    fn build_valid_proof(env: &Env, user: &Address) -> Bytes {
        build_valid_proof_with_seed(env, user, 0xDEAD_BEEF)
    }

    fn build_valid_proof_with_seed(env: &Env, user: &Address, seed: u32) -> Bytes {
        let proof_body = build_proof_body_with_seed(env, seed);

        let user_str: SdkString = user.to_string();
        let len = user_str.len() as usize;
        let mut raw = [0u8; 56];
        user_str.copy_into_slice(&mut raw[..len]);
        let mut user_bytes = Bytes::new(env);
        for byte in raw.iter().take(len) {
            user_bytes.push_back(*byte);
        }

        // Build preimage: proof_body || user_bytes || vk_commitment
        let mut preimage = Bytes::new(env);
        preimage.append(&proof_body);
        preimage.append(&user_bytes);
        for byte in VERIFYING_KEY_COMMITMENT.iter() {
            preimage.push_back(*byte);
        }

        let commitment = env.crypto().sha256(&preimage);

        // Assemble the full proof: [magic:4][version:1][commitment:32][proof_body:N]
        let mut proof = Bytes::new(env);
        for byte in ULTRAHONK_MAGIC.iter() {
            proof.push_back(*byte);
        }
        proof.push_back(EXPECTED_PROOF_VERSION);
        let commitment_array = commitment.to_array();
        for byte in commitment_array.iter() {
            proof.push_back(*byte);
        }
        proof.append(&proof_body);

        proof
    }

    fn build_proof_body_with_seed(env: &Env, seed: u32) -> Bytes {
        let target_len = MIN_PROOF_LENGTH as usize;
        let mut body = Bytes::new(env);
        let mut state: u32 = seed;
        for _ in 0..target_len {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            body.push_back((state >> 24) as u8);
        }
        body
    }

    // ── Setup helpers ────────────────────────────────────────────────────────

    /// Sets up all four contracts on a shared Env, initializes them, and
    /// returns (env, rules_client, limits_client, categories_client, zk_client, admin).
    fn setup_full<'a>() -> (
        Env,
        ContractClient<'a>,
        spending_limits::ContractClient<'a>,
        spending_categories::ContractClient<'a>,
        zk_verifier::ZkVerifierContractClient<'a>,
        Address,
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);

        // Register all contracts
        let limits_id = env.register(spending_limits::Contract, ());
        let categories_id = env.register(spending_categories::Contract, ());
        let zk_id = env.register(zk_verifier::ZkVerifierContract, ());
        let rules_id = env.register(Contract, ());

        let limits_client = spending_limits::ContractClient::new(&env, &limits_id);
        let categories_client = spending_categories::ContractClient::new(&env, &categories_id);
        let zk_client = zk_verifier::ZkVerifierContractClient::new(&env, &zk_id);
        let rules_client = ContractClient::new(&env, &rules_id);

        // Initialize all contracts
        limits_client.initialize(&admin);
        categories_client.initialize(&admin);
        rules_client.initialize(&admin, &limits_id, &categories_id, &zk_id);

        (
            env,
            rules_client,
            limits_client,
            categories_client,
            zk_client,
            admin,
        )
    }

    fn groceries(env: &Env) -> Symbol {
        Symbol::new(env, "Groceries")
    }

    fn weekly(env: &Env) -> Symbol {
        Symbol::new(env, "weekly")
    }

    fn xlm(env: &Env) -> Symbol {
        Symbol::new(env, XLM)
    }

    // ══════════════════════════════════════════════════════════════════════════
    // ACCEPTANCE CRITERIA TESTS
    // ══════════════════════════════════════════════════════════════════════════

    /// Payment under all thresholds passes — no proof needed.
    ///
    /// Rule: weekly 200 XLM Groceries cap, ZK required above 100 XLM.
    /// Attempt a 50 XLM Groceries payment → must succeed, no proof needed.
    #[test]
    fn payment_under_all_thresholds_passes_without_proof() {
        let (env, rules_client, limits_client, _cats_client, _zk_client, _admin) = setup_full();
        let user = Address::generate(&env);

        // Set up a generous weekly limit on spending-limits (not the binding constraint)
        limits_client.set_limit(&user, &xlm(&env), &1000, &weekly(&env));

        // Set rule: 200 XLM weekly cap, ZK required above 100 XLM
        rules_client.set_rule(&_admin, &user, &groceries(&env), &200, &100);

        // 50 XLM < 100 (ZK threshold) and 50 < 200 (weekly cap) → must pass
        let result = rules_client.try_evaluate(&user, &groceries(&env), &50, &None);
        assert_eq!(result, Ok(Ok(())));
    }

    /// Payment above ZK threshold fails without proof.
    ///
    /// Attempt a 150 XLM Groceries payment with no proof → must fail with ZkProofRequired.
    #[test]
    fn payment_above_zk_threshold_fails_without_proof() {
        let (env, rules_client, limits_client, _cats_client, _zk_client, _admin) = setup_full();
        let user = Address::generate(&env);

        limits_client.set_limit(&user, &xlm(&env), &1000, &weekly(&env));

        rules_client.set_rule(&_admin, &user, &groceries(&env), &200, &100);

        // 150 XLM > 100 (ZK threshold), no proof → ZkProofRequired
        let result = rules_client.try_evaluate(&user, &groceries(&env), &150, &None);
        assert_eq!(result, Err(Ok(Error::ZkProofRequired)));
    }

    /// Payment above ZK threshold passes with valid proof.
    ///
    /// Attempt a 150 XLM Groceries payment with a valid proof → must succeed.
    #[test]
    fn payment_above_zk_threshold_passes_with_valid_proof() {
        let (env, rules_client, limits_client, _cats_client, _zk_client, _admin) = setup_full();
        let user = Address::generate(&env);

        limits_client.set_limit(&user, &xlm(&env), &1000, &weekly(&env));

        rules_client.set_rule(&_admin, &user, &groceries(&env), &200, &100);

        let proof = build_valid_proof(&env, &user);

        // 150 XLM > 100 (ZK threshold), valid proof provided → must pass
        let result = rules_client.try_evaluate(&user, &groceries(&env), &150, &Some(proof));
        assert_eq!(result, Ok(Ok(())));
    }

    /// Payment exceeding weekly category cap fails even with valid proof.
    ///
    /// Rule: 200 XLM weekly Groceries cap. User has already spent 80 XLM this
    /// week. Attempt 170 XLM with valid proof → 80 + 170 = 250 > 200 → fail.
    #[test]
    fn payment_exceeding_weekly_category_cap_fails_with_valid_proof() {
        let (env, rules_client, limits_client, categories_client, _zk_client, admin) = setup_full();
        let user = Address::generate(&env);

        // Set up a generous spending-limits cap (not the binding constraint)
        limits_client.set_limit(&user, &xlm(&env), &1000, &weekly(&env));

        // Set rule: 200 XLM weekly cap, ZK required above 100 XLM
        rules_client.set_rule(&admin, &user, &groceries(&env), &200, &100);

        // Pre-record 80 XLM of category spend via spending-categories.
        // Use a dummy tx_id that won't collide with evaluate's usage.
        let setup_tx_id: u64 = 9999;
        categories_client.set_category(&user, &setup_tx_id, &groceries(&env));
        categories_client.record_category_spend(&user, &setup_tx_id, &80);

        let proof = build_valid_proof(&env, &user);

        // 80 (already spent) + 170 (proposed) = 250 > 200 (weekly cap)
        let result = rules_client.try_evaluate(&user, &groceries(&env), &170, &Some(proof));
        assert_eq!(result, Err(Ok(Error::CategoryLimitExceeded)));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // ADDITIONAL COVERAGE
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn rule_not_found_returns_error() {
        let (env, rules_client, _limits_client, _cats_client, _zk_client, _admin) = setup_full();
        let user = Address::generate(&env);

        // No rule set for this user/category
        let result = rules_client.try_evaluate(&user, &groceries(&env), &50, &None);
        assert_eq!(result, Err(Ok(Error::RuleNotFound)));
    }

    #[test]
    fn zero_amount_is_rejected() {
        let (env, rules_client, limits_client, _cats_client, _zk_client, admin) = setup_full();
        let user = Address::generate(&env);

        limits_client.set_limit(&user, &xlm(&env), &1000, &weekly(&env));
        rules_client.set_rule(&admin, &user, &groceries(&env), &200, &100);

        let result = rules_client.try_evaluate(&user, &groceries(&env), &0, &None);
        assert_eq!(result, Err(Ok(Error::InvalidAmount)));
    }

    #[test]
    fn negative_amount_is_rejected() {
        let (env, rules_client, limits_client, _cats_client, _zk_client, admin) = setup_full();
        let user = Address::generate(&env);

        limits_client.set_limit(&user, &xlm(&env), &1000, &weekly(&env));
        rules_client.set_rule(&admin, &user, &groceries(&env), &200, &100);

        let result = rules_client.try_evaluate(&user, &groceries(&env), &-10, &None);
        assert_eq!(result, Err(Ok(Error::InvalidAmount)));
    }

    #[test]
    fn invalid_zk_proof_is_rejected() {
        let (env, rules_client, limits_client, _cats_client, _zk_client, admin) = setup_full();
        let user = Address::generate(&env);

        limits_client.set_limit(&user, &xlm(&env), &1000, &weekly(&env));
        rules_client.set_rule(&admin, &user, &groceries(&env), &200, &100);

        // Build a proof with wrong magic bytes → zk-verifier rejects it
        let mut bad_proof = Bytes::new(&env);
        bad_proof.push_back(0xBA);
        bad_proof.push_back(0xDC);
        bad_proof.push_back(0xAF);
        bad_proof.push_back(0xFE);
        for _ in 4..MIN_PROOF_LENGTH {
            bad_proof.push_back(0x00);
        }

        // 150 XLM > 100 (ZK threshold), invalid proof → ZkProofInvalid
        let result = rules_client.try_evaluate(&user, &groceries(&env), &150, &Some(bad_proof));
        assert_eq!(result, Err(Ok(Error::ZkProofInvalid)));
    }

    #[test]
    fn payment_at_exactly_zk_threshold_does_not_require_proof() {
        let (env, rules_client, limits_client, _cats_client, _zk_client, admin) = setup_full();
        let user = Address::generate(&env);

        limits_client.set_limit(&user, &xlm(&env), &1000, &weekly(&env));
        rules_client.set_rule(&admin, &user, &groceries(&env), &200, &100);

        // Exactly 100 XLM (at the threshold, not above) → no proof required
        let result = rules_client.try_evaluate(&user, &groceries(&env), &100, &None);
        assert_eq!(result, Ok(Ok(())));
    }

    #[test]
    fn payment_at_exactly_weekly_limit_succeeds() {
        let (env, rules_client, limits_client, _cats_client, _zk_client, admin) = setup_full();
        let user = Address::generate(&env);

        limits_client.set_limit(&user, &xlm(&env), &1000, &weekly(&env));
        rules_client.set_rule(&admin, &user, &groceries(&env), &200, &i128::MAX);

        // Exactly 200 XLM (at the limit, not over) → succeeds
        let result = rules_client.try_evaluate(&user, &groceries(&env), &200, &None);
        assert_eq!(result, Ok(Ok(())));
    }

    #[test]
    fn payment_one_over_weekly_limit_fails() {
        let (env, rules_client, limits_client, _cats_client, _zk_client, admin) = setup_full();
        let user = Address::generate(&env);

        limits_client.set_limit(&user, &xlm(&env), &1000, &weekly(&env));
        rules_client.set_rule(&admin, &user, &groceries(&env), &200, &i128::MAX);

        // 201 XLM > 200 (weekly limit) → fails
        let result = rules_client.try_evaluate(&user, &groceries(&env), &201, &None);
        assert_eq!(result, Err(Ok(Error::CategoryLimitExceeded)));
    }

    #[test]
    fn spending_limits_cap_is_respected() {
        let (env, rules_client, limits_client, _cats_client, _zk_client, admin) = setup_full();
        let user = Address::generate(&env);

        // Set a tight spending-limits cap of 80 XLM weekly
        limits_client.set_limit(&user, &xlm(&env), &80, &weekly(&env));

        // Rule allows 200 XLM (not the binding constraint)
        rules_client.set_rule(&admin, &user, &groceries(&env), &200, &i128::MAX);

        // 100 XLM > 80 (spending-limits cap) → fails
        let result = rules_client.try_evaluate(&user, &groceries(&env), &100, &None);
        assert_eq!(result, Err(Ok(Error::CategoryLimitExceeded)));
    }

    #[test]
    fn get_rule_returns_none_for_unset_user() {
        let (env, rules_client, _limits_client, _cats_client, _zk_client, _admin) = setup_full();
        let user = Address::generate(&env);

        let result = rules_client.get_rule(&user, &groceries(&env));
        assert_eq!(result, None);
    }

    #[test]
    fn get_rule_returns_set_rule() {
        let (env, rules_client, limits_client, _cats_client, _zk_client, admin) = setup_full();
        let user = Address::generate(&env);

        limits_client.set_limit(&user, &xlm(&env), &1000, &weekly(&env));
        rules_client.set_rule(&admin, &user, &groceries(&env), &200, &100);

        let rule = rules_client.get_rule(&user, &groceries(&env));
        assert!(rule.is_some());
        let rule = rule.unwrap();
        assert_eq!(rule.category, groceries(&env));
        assert_eq!(rule.weekly_limit, 200);
        assert_eq!(rule.zk_required_above, 100);
    }

    #[test]
    fn initialize_twice_is_rejected() {
        let (env, _rules_client, limits_client, categories_client, _zk_client, admin) =
            setup_full();

        // Create a fresh contract and initialize it, then try again
        let rules2_id = env.register(Contract, ());
        let rules2_client = ContractClient::new(&env, &rules2_id);
        rules2_client.initialize(
            &admin,
            &limits_client.address,
            &categories_client.address,
            &_zk_client.address,
        );

        // Second initialize must fail
        let result = rules2_client.try_initialize(
            &admin,
            &limits_client.address,
            &categories_client.address,
            &_zk_client.address,
        );
        assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
    }

    #[test]
    fn different_categories_are_independent() {
        let (env, rules_client, limits_client, _cats_client, _zk_client, admin) = setup_full();
        let user = Address::generate(&env);

        limits_client.set_limit(&user, &xlm(&env), &1000, &weekly(&env));

        let groceries_cat = groceries(&env);
        let travel_cat = Symbol::new(&env, "Travel");

        // Groceries: 200 weekly, no ZK required
        rules_client.set_rule(&admin, &user, &groceries_cat, &200, &i128::MAX);
        // Travel: 50 weekly, ZK required above 30
        rules_client.set_rule(&admin, &user, &travel_cat, &50, &30);

        // 40 XLM Groceries → passes (40 < 200, no ZK needed)
        let result = rules_client.try_evaluate(&user, &groceries_cat, &40, &None);
        assert_eq!(result, Ok(Ok(())));

        // 40 XLM Travel → fails ZK check (40 > 30, no proof)
        let result = rules_client.try_evaluate(&user, &travel_cat, &40, &None);
        assert_eq!(result, Err(Ok(Error::ZkProofRequired)));

        // 20 XLM Travel → passes (20 < 30, no ZK needed; 20 < 50 weekly)
        let result = rules_client.try_evaluate(&user, &travel_cat, &20, &None);
        assert_eq!(result, Ok(Ok(())));
    }

    #[test]
    fn get_cross_contract_addresses_returns_configured_values() {
        let (_env, _rules_client, limits_client, categories_client, _zk_client, _admin) =
            setup_full();

        assert_eq!(
            _rules_client.get_spending_limits_address(),
            Some(limits_client.address.clone())
        );
        assert_eq!(
            _rules_client.get_spending_categories_address(),
            Some(categories_client.address.clone())
        );
    }

    // ── initialize persistence ────────────────────────────────────────────
    //
    // This crate has no `get_value`: initialize writes the admin plus the
    // three dependency addresses it composes over. The ticket's "assert
    // get_value returns 0" has no direct counterpart here, so these tests
    // assert the observable equivalent — that initialize persists the admin
    // and each dependency address, and that it seeds no rules.

    #[test]
    fn initialize_persists_the_admin_and_all_three_dependency_addresses() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let limits_id = env.register(spending_limits::Contract, ());
        let categories_id = env.register(spending_categories::Contract, ());
        let zk_id = env.register(zk_verifier::ZkVerifierContract, ());
        let rules_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &rules_id);

        // Nothing is stored before initialize.
        assert_eq!(storage::read_config(&env), None);
        assert_eq!(client.get_spending_limits_address(), None);
        assert_eq!(client.get_spending_categories_address(), None);
        assert_eq!(client.get_zk_verifier_address(), None);

        // The contractimpl client unwraps Result, so a success is just `()`.
        assert_eq!(client.initialize(&admin, &limits_id, &categories_id, &zk_id), ());

        let config = storage::read_config(&env).expect("initialize must persist a config");
        assert_eq!(config.admin, admin);
        assert_eq!(client.get_spending_limits_address(), Some(limits_id));
        assert_eq!(client.get_spending_categories_address(), Some(categories_id));
        assert_eq!(client.get_zk_verifier_address(), Some(zk_id));
    }

    #[test]
    fn initialize_seeds_no_rules() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let limits_id = env.register(spending_limits::Contract, ());
        let categories_id = env.register(spending_categories::Contract, ());
        let zk_id = env.register(zk_verifier::ZkVerifierContract, ());
        let rules_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &rules_id);
        client.initialize(&admin, &limits_id, &categories_id, &zk_id);

        // A fresh user has no rule, so evaluation fails closed with
        // RuleNotFound rather than falling back to a default cap.
        assert_eq!(client.get_rule(&user, &groceries(&env)), None);
        assert_eq!(
            client.try_evaluate(&user, &groceries(&env), &1, &None),
            Err(Ok(Error::RuleNotFound))
        );
    }

    #[test]
    fn initialize_requires_the_admin_to_authorize() {
        let env = Env::default();
        // Deliberately do NOT mock_all_auths — nothing has authorized anything.
        let admin = Address::generate(&env);
        let limits_id = env.register(spending_limits::Contract, ());
        let categories_id = env.register(spending_categories::Contract, ());
        let zk_id = env.register(zk_verifier::ZkVerifierContract, ());
        let rules_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &rules_id);

        assert!(client
            .try_initialize(&admin, &limits_id, &categories_id, &zk_id)
            .is_err());
        assert_eq!(storage::read_config(&env), None);
        assert_eq!(client.get_spending_limits_address(), None);
    }

    #[test]
    fn rejected_reinitialize_does_not_overwrite_config_or_addresses() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let other = Address::generate(&env);
        let limits_id = env.register(spending_limits::Contract, ());
        let categories_id = env.register(spending_categories::Contract, ());
        let zk_id = env.register(zk_verifier::ZkVerifierContract, ());
        let rules_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &rules_id);
        client.initialize(&admin, &limits_id, &categories_id, &zk_id);

        // Point a second initialize at entirely different dependencies.
        let other_limits = env.register(spending_limits::Contract, ());
        let other_categories = env.register(spending_categories::Contract, ());
        let other_zk = env.register(zk_verifier::ZkVerifierContract, ());

        assert_eq!(
            client.try_initialize(&other, &other_limits, &other_categories, &other_zk),
            Err(Ok(Error::AlreadyInitialized))
        );

        // A rejected re-initialize must not hand over the admin or repoint
        // the contract at the second set of dependencies.
        let config = storage::read_config(&env).expect("config must still be present");
        assert_eq!(config.admin, admin);
        assert_eq!(client.get_spending_limits_address(), Some(limits_id));
        assert_eq!(client.get_spending_categories_address(), Some(categories_id));
        assert_eq!(client.get_zk_verifier_address(), Some(zk_id));
    }
}

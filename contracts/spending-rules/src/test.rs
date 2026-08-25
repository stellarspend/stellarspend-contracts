#[cfg(test)]
mod tests {
    use crate::{Contract, ContractClient, Error};
    use soroban_sdk::{testutils::Address as _, Address, Bytes, Env, Symbol};

    const XLM: &str = "XLM";
    const GROCERIES: &str = "Groceries";
    const WEEKLY: &str = "weekly";

    // UltraHonk proof format constants (documented in contracts/zk-verifier/
    // README.md). These are duplicated here so the engine can be tested against
    // the real zk-verifier contract without modifying that contract (which is
    // explicitly out of scope for this issue).
    const ULTRAHONK_MAGIC: [u8; 4] = [0x55, 0x48, 0x6e, 0x6b];
    const EXPECTED_PROOF_VERSION: u8 = 0x01;
    const MIN_PROOF_LENGTH: u32 = 4096;
    const VERIFYING_KEY_COMMITMENT: [u8; 32] = [
        0x3e, 0x8a, 0x1f, 0x5e, 0x9c, 0x2b, 0x8d, 0x4a, 0x6e, 0x3f, 0x0c, 0x7b, 0x5a, 0x9d, 0x1e,
        0x4f, 0x2c, 0x8b, 0x6a, 0x0d, 0x7e, 0x3f, 0x1c, 0x5b, 0x9a, 0x4d, 0x8e, 0x2f, 0x0a, 0x6c,
        0x1b, 0x7d,
    ];

    fn sym(env: &Env, s: &str) -> Symbol {
        Symbol::new(env, s)
    }

    /// Registers the three composed contracts plus the engine, and returns
    /// everything a test needs. `mock_all_auths` lets user-authorized calls
    /// (`set_rule`, `set_limit`, `record_category_spend`) proceed.
    fn setup<'a>() -> (Env, ContractClient<'a>, Address, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);

        let limits_id = env.register(spending_limits::Contract, ());
        let categories_id = env.register(spending_categories::Contract, ());
        let zk_id = env.register(zk_verifier::ZkVerifierContract, ());

        spending_limits::ContractClient::new(&env, &limits_id).initialize(&admin);
        spending_categories::ContractClient::new(&env, &categories_id).initialize(&admin);

        let rules_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &rules_id);
        client.initialize(&admin, &limits_id, &categories_id, &zk_id);

        (env, client, admin, limits_id, categories_id)
    }

    /// Configures the user's rule (weekly cap 200, ZK threshold 100) and a
    /// generous wallet-level XLM cap so the wallet check never binds in these
    /// scenarios (the category cap is what the acceptance criteria exercise).
    fn configure_user(env: &Env, client: &ContractClient, limits_id: &Address, user: &Address) {
        client.set_rule(user, &sym(env, GROCERIES), &200, &100);
        spending_limits::ContractClient::new(env, limits_id).set_limit(
            user,
            &sym(env, XLM),
            &10_000,
            &sym(env, WEEKLY),
        );
    }

    /// Records `amount` of prior spend against the user's "Groceries" category,
    /// so the weekly category cap sees it as already-spent.
    fn record_category_spend(
        env: &Env,
        categories_id: &Address,
        user: &Address,
        tx_id: u64,
        amount: i128,
    ) {
        let client = spending_categories::ContractClient::new(env, categories_id);
        client.set_category(user, &tx_id, &sym(env, GROCERIES));
        client.record_category_spend(user, &tx_id, &amount);
    }

    /// Builds a proof that the real zk-verifier will accept for `user`.
    /// Format: [magic 4][version 1][commitment 32][body N], where
    /// commitment = SHA-256(body || user_bytes || vk_commitment).
    fn build_valid_proof(env: &Env, user: &Address) -> Bytes {
        let mut body = Bytes::new(env);
        for i in 0..MIN_PROOF_LENGTH {
            body.push_back((i % 251) as u8);
        }

        let user_str: soroban_sdk::String = user.to_string();
        let len = user_str.len() as usize;
        let mut raw = [0u8; 56];
        user_str.copy_into_slice(&mut raw[..len]);
        let mut user_bytes = Bytes::new(env);
        for b in raw.iter().take(len) {
            user_bytes.push_back(*b);
        }

        let mut preimage = Bytes::new(env);
        preimage.append(&body);
        preimage.append(&user_bytes);
        for b in VERIFYING_KEY_COMMITMENT.iter() {
            preimage.push_back(*b);
        }
        let commitment = env.crypto().sha256(&preimage);

        let mut proof = Bytes::new(env);
        for b in ULTRAHONK_MAGIC.iter() {
            proof.push_back(*b);
        }
        proof.push_back(EXPECTED_PROOF_VERSION);
        let commitment_array = commitment.to_array();
        for b in commitment_array.iter() {
            proof.push_back(*b);
        }
        proof.append(&body);
        proof
    }

    /// Builds a structurally plausible but invalid proof (wrong commitment).
    fn build_invalid_proof(env: &Env) -> Bytes {
        let mut proof = Bytes::new(env);
        for b in ULTRAHONK_MAGIC.iter() {
            proof.push_back(*b);
        }
        proof.push_back(EXPECTED_PROOF_VERSION);
        for _ in 0..32 {
            proof.push_back(0x00);
        }
        for _ in 0..MIN_PROOF_LENGTH {
            proof.push_back(0xAB);
        }
        proof
    }

    // ── The issue's four acceptance scenarios ─────────────────────────────

    #[test]
    fn payment_under_all_thresholds_succeeds_without_proof() {
        let (env, client, _admin, limits_id, _categories_id) = setup();
        let user = Address::generate(&env);
        configure_user(&env, &client, &limits_id, &user);

        // 50 XLM Groceries: below the 100 ZK threshold and the 200 weekly cap.
        client.evaluate(&user, &sym(&env, GROCERIES), &50, &None);
    }

    #[test]
    fn payment_above_zk_threshold_fails_without_proof() {
        let (env, client, _admin, limits_id, _categories_id) = setup();
        let user = Address::generate(&env);
        configure_user(&env, &client, &limits_id, &user);

        // 150 XLM Groceries: above the 100 ZK threshold, no proof supplied.
        assert_eq!(
            client.try_evaluate(&user, &sym(&env, GROCERIES), &150, &None),
            Err(Ok(Error::ZkProofRequired))
        );
    }

    #[test]
    fn payment_above_zk_threshold_succeeds_with_valid_proof() {
        let (env, client, _admin, limits_id, _categories_id) = setup();
        let user = Address::generate(&env);
        configure_user(&env, &client, &limits_id, &user);

        let proof = build_valid_proof(&env, &user);
        client.evaluate(&user, &sym(&env, GROCERIES), &150, &Some(proof));
    }

    #[test]
    fn payment_exceeding_weekly_cap_fails_even_with_valid_proof() {
        let (env, client, _admin, limits_id, categories_id) = setup();
        let user = Address::generate(&env);
        configure_user(&env, &client, &limits_id, &user);

        // 80 XLM already spent this week in "Groceries".
        record_category_spend(&env, &categories_id, &user, 1, 80);

        let proof = build_valid_proof(&env, &user);
        // 80 + 170 = 250 > 200 -> category cap breached.
        assert_eq!(
            client.try_evaluate(&user, &sym(&env, GROCERIES), &170, &Some(proof)),
            Err(Ok(Error::CategoryLimitExceeded))
        );
    }

    // ── Supporting behavior ────────────────────────────────────────────────

    #[test]
    fn payment_above_zk_threshold_with_invalid_proof_fails() {
        let (env, client, _admin, limits_id, _categories_id) = setup();
        let user = Address::generate(&env);
        configure_user(&env, &client, &limits_id, &user);

        let proof = build_invalid_proof(&env);
        assert_eq!(
            client.try_evaluate(&user, &sym(&env, GROCERIES), &150, &Some(proof)),
            Err(Ok(Error::ZkProofInvalid))
        );
    }

    #[test]
    fn evaluate_for_a_category_without_a_rule_fails() {
        let (env, client, _admin, limits_id, _categories_id) = setup();
        let user = Address::generate(&env);
        configure_user(&env, &client, &limits_id, &user);

        // "Rent" has no rule configured.
        assert_eq!(
            client.try_evaluate(&user, &sym(&env, "Rent"), &10, &None),
            Err(Ok(Error::RuleNotFound))
        );
    }

    #[test]
    fn evaluate_rejects_a_non_positive_amount() {
        let (env, client, _admin, limits_id, _categories_id) = setup();
        let user = Address::generate(&env);
        configure_user(&env, &client, &limits_id, &user);

        assert_eq!(
            client.try_evaluate(&user, &sym(&env, GROCERIES), &0, &None),
            Err(Ok(Error::InvalidAmount))
        );
    }

    // ── Rule management ────────────────────────────────────────────────────

    #[test]
    fn set_and_get_rule_round_trips() {
        let (env, client, _admin, _limits_id, _categories_id) = setup();
        let user = Address::generate(&env);

        client.set_rule(&user, &sym(&env, GROCERIES), &200, &100);
        let rule = client.get_rule(&user, &sym(&env, GROCERIES)).unwrap();
        assert_eq!(rule.weekly_limit, 200);
        assert_eq!(rule.zk_required_above, 100);
    }

    #[test]
    fn set_rule_rejects_negative_limits() {
        let (env, client, _admin, _limits_id, _categories_id) = setup();
        let user = Address::generate(&env);

        assert_eq!(
            client.try_set_rule(&user, &sym(&env, GROCERIES), &-1, &100),
            Err(Ok(Error::InvalidAmount))
        );
    }
}

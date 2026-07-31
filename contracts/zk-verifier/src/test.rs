#[cfg(test)]
mod test {
    use soroban_sdk::{
        testutils::{Address as _, Bytes as _},
        Address, Bytes, Env,
    };

    use crate::verification::{
        verify_spending_proof, EXPECTED_PROOF_VERSION, MAX_PROOF_LENGTH, MIN_PROOF_LENGTH,
        ULTRAHONK_MAGIC, VERIFYING_KEY_COMMITMENT,
    };

    // ── Test Helpers ──────────────────────────────────────────────────────────

    /// Builds a valid-looking proof for testing.
    ///
    /// The proof has the format:
    ///   [magic: 4B][version: 1B][commitment: 32B][proof_body: N bytes]
    ///
    /// The commitment is SHA-256(proof_body || user_bytes || vk_commitment),
    /// making it cryptographically valid.
    fn build_valid_proof(env: &Env, user: &Address) -> Bytes {
        build_valid_proof_with_seed(env, user, 0xDEAD_BEEF)
    }

    /// Builds a valid proof with a specific LCG seed for the proof body.
    /// Different seeds produce different (but still valid) proofs.
    fn build_valid_proof_with_seed(env: &Env, user: &Address, seed: u32) -> Bytes {
        let proof_body = build_proof_body_with_seed(env, seed);

        let user_bytes = user_to_test_bytes(env, user);

        // Build preimage using bulk append for efficiency
        let mut preimage = Bytes::new(env);
        preimage.append(&proof_body);
        preimage.append(&user_bytes);
        for byte in VERIFYING_KEY_COMMITMENT.iter() {
            preimage.push_back(*byte);
        }

        let commitment = env.crypto().sha256(&preimage);

        // Assemble the full proof
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

    /// Builds a proof with an explicitly provided (incorrect) commitment.
    /// Used for testing proofs that are structurally valid but have the wrong hash.
    fn build_proof_with_commitment(env: &Env, commitment: &soroban_sdk::BytesN<32>) -> Bytes {
        let proof_body = build_proof_body_with_seed(env, 0xBEEF_CAFE);

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

    /// Creates a realistic proof body byte sequence with a given LCG seed.
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

    /// Converts a Soroban Address to test bytes.
    fn user_to_test_bytes(env: &Env, user: &Address) -> Bytes {
        let s = user.to_string();
        let mut b = Bytes::new(env);
        for byte in s.as_bytes() {
            b.push_back(*byte);
        }
        b
    }

    // ── Positive Tests ────────────────────────────────────────────────────────

    #[test]
    fn valid_proof_verifies_successfully() {
        let env = Env::default();
        let user = Address::generate(&env);

        let proof = build_valid_proof(&env, &user);
        assert!(verify_spending_proof(&env, &user, &proof));
    }

    #[test]
    fn valid_proof_for_different_user_succeeds_when_bound_to_that_user() {
        let env = Env::default();
        let user1 = Address::generate(&env);
        let user2 = Address::generate(&env);

        let proof1 = build_valid_proof(&env, &user1);
        let proof2 = build_valid_proof(&env, &user2);

        assert!(verify_spending_proof(&env, &user1, &proof1));
        assert!(verify_spending_proof(&env, &user2, &proof2));
    }

    #[test]
    fn two_different_proofs_for_same_user_both_pass() {
        let env = Env::default();
        let user = Address::generate(&env);

        // Two proofs with different proof bodies (different seeds)
        let proof1 = build_valid_proof_with_seed(&env, &user, 0xAAAA);
        let proof2 = build_valid_proof_with_seed(&env, &user, 0xBBBB);

        assert_ne!(proof1, proof2, "proofs should differ due to different seeds");
        assert!(verify_spending_proof(&env, &user, &proof1));
        assert!(verify_spending_proof(&env, &user, &proof2));
    }

    // ── Negative Tests: Empty / Zero-Length ──────────────────────────────────

    #[test]
    fn empty_proof_fails() {
        let env = Env::default();
        let user = Address::generate(&env);
        let proof = Bytes::new(&env);

        assert!(!verify_spending_proof(&env, &user, &proof));
    }

    #[test]
    fn proof_below_min_length_fails() {
        let env = Env::default();
        let user = Address::generate(&env);

        let mut proof = Bytes::new(&env);
        for _ in 0..MIN_PROOF_LENGTH - 1 {
            proof.push_back(0x00);
        }

        assert!(!verify_spending_proof(&env, &user, &proof));
    }

    // ── Negative Tests: Malformed / Truncated ────────────────────────────────

    #[test]
    fn truncated_proof_fails() {
        let env = Env::default();
        let user = Address::generate(&env);

        let valid_proof = build_valid_proof(&env, &user);
        let original_len = valid_proof.len();
        let half = original_len / 2;

        // Truncate: take only the first half of bytes
        let truncated = valid_proof.slice(0..half);

        assert!(!verify_spending_proof(&env, &user, &truncated));
    }

    #[test]
    fn proof_with_wrong_magic_fails() {
        let env = Env::default();
        let user = Address::generate(&env);

        let valid_proof = build_valid_proof(&env, &user);
        let mut bad_magic = Bytes::new(&env);

        // "BADC" instead of "UHnk"
        bad_magic.push_back(0xBA);
        bad_magic.push_back(0xDC);
        bad_magic.push_back(0xAF);
        bad_magic.push_back(0xFE);
        // Copy the rest
        for i in 4..valid_proof.len() {
            if let Some(b) = valid_proof.get(i) {
                bad_magic.push_back(b);
            }
        }

        assert!(!verify_spending_proof(&env, &user, &bad_magic));
    }

    #[test]
    fn proof_with_wrong_version_fails() {
        let env = Env::default();
        let user = Address::generate(&env);

        let valid_proof = build_valid_proof(&env, &user);
        let mut bad_version = Bytes::new(&env);

        for i in 0..4 {
            if let Some(b) = valid_proof.get(i) {
                bad_version.push_back(b);
            }
        }
        bad_version.push_back(0xFF); // wrong version
        for i in 5..valid_proof.len() {
            if let Some(b) = valid_proof.get(i) {
                bad_version.push_back(b);
            }
        }

        assert!(!verify_spending_proof(&env, &user, &bad_version));
    }

    // ── Negative Tests: Byte Mutation ─────────────────────────────────────────

    #[test]
    fn single_byte_mutation_in_commitment_fails() {
        let env = Env::default();
        let user = Address::generate(&env);

        let valid_proof = build_valid_proof(&env, &user);
        let valid_len = valid_proof.len();

        // Mutate byte at offset 10 (inside the commitment, bytes 5..37)
        let mut mutated = Bytes::new(&env);
        for i in 0..valid_len {
            if let Some(b) = valid_proof.get(i) {
                if i == 10 {
                    mutated.push_back(b.wrapping_add(1));
                } else {
                    mutated.push_back(b);
                }
            }
        }

        assert!(!verify_spending_proof(&env, &user, &mutated));
    }

    #[test]
    fn single_byte_mutation_in_body_fails() {
        let env = Env::default();
        let user = Address::generate(&env);

        let valid_proof = build_valid_proof(&env, &user);
        let valid_len = valid_proof.len();

        // Mutate byte at offset 100 (inside the proof body)
        let mut mutated = Bytes::new(&env);
        for i in 0..valid_len {
            if let Some(b) = valid_proof.get(i) {
                if i == 100 {
                    mutated.push_back(b.wrapping_add(1));
                } else {
                    mutated.push_back(b);
                }
            }
        }

        assert!(!verify_spending_proof(&env, &user, &mutated));
    }

    // ── Negative Tests: Wrong User / Cross-User Replay ────────────────────────

    #[test]
    fn proof_bound_to_user_a_fails_for_user_b() {
        let env = Env::default();
        let user_a = Address::generate(&env);
        let user_b = Address::generate(&env);

        let proof_a = build_valid_proof(&env, &user_a);

        assert!(!verify_spending_proof(&env, &user_b, &proof_a));
    }

    #[test]
    fn cross_user_replay_attack_fails() {
        let env = Env::default();
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        let alice_proof = build_valid_proof(&env, &alice);

        assert!(!verify_spending_proof(&env, &bob, &alice_proof));
    }

    // ── Negative Tests: Wrong Verifying Key / Different Circuit ─────────────

    #[test]
    fn proof_with_wrong_vk_commitment_fails() {
        let env = Env::default();
        let user = Address::generate(&env);

        let proof_body = build_proof_body_with_seed(&env, 0x1234);
        let user_bytes = user_to_test_bytes(&env, &user);

        // Use a completely different VK commitment (all 0xFF)
        let wrong_vk: [u8; 32] = [0xFF; 32];

        let mut preimage = Bytes::new(&env);
        preimage.append(&proof_body);
        preimage.append(&user_bytes);
        for byte in wrong_vk.iter() {
            preimage.push_back(*byte);
        }

        let wrong_commitment = env.crypto().sha256(&preimage);

        let proof = build_proof_with_commitment(&env, &wrong_commitment);

        // Should fail: verifier uses REAL vk_commitment, not wrong_vk
        assert!(!verify_spending_proof(&env, &user, &proof));
    }

    #[test]
    fn proof_from_different_circuit_format_fails() {
        // Simulates a proof generated by a different proving system
        // (e.g., Groth16 or Plonk instead of UltraHonk)
        let env = Env::default();
        let user = Address::generate(&env);

        // Use "PLNK" magic instead of "UHnk"
        let mut proof = Bytes::new(&env);
        proof.push_back(b'P');
        proof.push_back(b'L');
        proof.push_back(b'N');
        proof.push_back(b'K');
        proof.push_back(EXPECTED_PROOF_VERSION);
        // Fill with enough bytes to reach MIN_PROOF_LENGTH
        for _ in 5..MIN_PROOF_LENGTH {
            proof.push_back(0xAB);
        }

        assert!(!verify_spending_proof(&env, &user, &proof));
    }

    // ── Negative Tests: Different Spending Limit ──────────────────────────────

    #[test]
    fn proof_with_arbitrary_commitment_not_matching_vk_fails() {
        // If someone generates a proof with the right format but an arbitrary
        // commitment (not derived from our VK), it should fail.
        let env = Env::default();
        let user = Address::generate(&env);

        let proof_body = build_proof_body_with_seed(&env, 0x9999);
        let user_bytes = user_to_test_bytes(&env, &user);

        // Compute commitment with user but a ZERO VK (wrong circuit)
        let zero_vk: [u8; 32] = [0x00; 32];

        let mut preimage = Bytes::new(&env);
        preimage.append(&proof_body);
        preimage.append(&user_bytes);
        for byte in zero_vk.iter() {
            preimage.push_back(*byte);
        }

        let arbitrary_commitment = env.crypto().sha256(&preimage);
        let proof = build_proof_with_commitment(&env, &arbitrary_commitment);

        assert!(!verify_spending_proof(&env, &user, &proof));
    }

    // ── Negative Tests: Corrupted Public Inputs ───────────────────────────────

    #[test]
    fn proof_with_corrupted_commitment_fails() {
        let env = Env::default();
        let user = Address::generate(&env);

        let valid_proof = build_valid_proof(&env, &user);
        let valid_len = valid_proof.len();

        // Zero out the commitment bytes (offsets 5..37)
        let mut corrupted = Bytes::new(&env);
        for i in 0..valid_len {
            if let Some(b) = valid_proof.get(i) {
                if i >= 5 && i < 37 {
                    corrupted.push_back(0x00);
                } else {
                    corrupted.push_back(b);
                }
            }
        }

        assert!(!verify_spending_proof(&env, &user, &corrupted));
    }

    // ── Negative Tests: Invalid Serialization / Garbage ──────────────────────

    #[test]
    fn all_zero_bytes_fails() {
        let env = Env::default();
        let user = Address::generate(&env);

        let mut proof = Bytes::new(&env);
        for _ in 0..MIN_PROOF_LENGTH {
            proof.push_back(0x00);
        }

        assert!(!verify_spending_proof(&env, &user, &proof));
    }

    #[test]
    fn random_garbage_bytes_fails() {
        let env = Env::default();
        let user = Address::generate(&env);

        let mut proof = Bytes::new(&env);
        let mut state: u32 = 0xCAFE_BABE;
        for _ in 0..MIN_PROOF_LENGTH {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            proof.push_back((state >> 16) as u8);
        }

        assert!(!verify_spending_proof(&env, &user, &proof));
    }

    #[test]
    fn proof_too_large_fails() {
        let env = Env::default();
        let user = Address::generate(&env);

        let base = build_valid_proof(&env, &user);
        let mut oversized = Bytes::new(&env);
        oversized.append(&base);
        // Pad to exceed MAX_PROOF_LENGTH
        let needed = (MAX_PROOF_LENGTH + 1).saturating_sub(oversized.len());
        for _ in 0..needed {
            oversized.push_back(0x00);
        }

        assert!(!verify_spending_proof(&env, &user, &oversized));
    }

    // ── Edge Cases ────────────────────────────────────────────────────────────

    #[test]
    fn proof_exactly_min_length_with_wrong_structure_fails() {
        let env = Env::default();
        let user = Address::generate(&env);

        let mut proof = Bytes::new(&env);
        proof.push_back(0xDE);
        proof.push_back(0xAD);
        proof.push_back(0xBE);
        proof.push_back(0xEF);
        for _ in 4..MIN_PROOF_LENGTH {
            proof.push_back(0x00);
        }

        assert!(!verify_spending_proof(&env, &user, &proof));
    }

    #[test]
    fn header_only_no_body_fails() {
        let env = Env::default();
        let user = Address::generate(&env);

        // Just the 37-byte header, no proof body (below MIN_PROOF_LENGTH)
        let mut proof = Bytes::new(&env);
        for byte in ULTRAHONK_MAGIC.iter() {
            proof.push_back(*byte);
        }
        proof.push_back(EXPECTED_PROOF_VERSION);
        for _ in 0..32 {
            proof.push_back(0x00);
        }

        assert!(!verify_spending_proof(&env, &user, &proof));
    }
}

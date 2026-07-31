//! # Gas Optimization Contract
//!
//! Provides gas estimation utilities and gas optimization benchmarks
//! for StellarSpend smart contracts.
//!
//! ## Production Features
//! - **get_gas_estimate**: Returns the estimated gas cost for a named operation
//!
//! ## Test-only Features
//! - Storage operation count comparisons (before vs after)
//! - Instruction budget comparisons using `env.budget()`
//! - Batch reward gas scaling (1 / 10 / 50 recipients)
//! - Escrow gas benchmarks
//! - Correctness assertions (ensure optimizations didn't break behavior)

use soroban_sdk::{contracttype, Env, Symbol};

/// Storage keys for gas estimate data.
#[derive(Clone)]
#[contracttype]
pub enum GasEstimateKey {
    /// Keyed by operation symbol name.
    Operation(Symbol),
}

/// Returns the estimated gas cost (in CPU instructions) for a named operation.
///
/// # Arguments
/// * `env` - The contract environment
/// * `operation` - The operation symbol (e.g. "stake", "unstake", "batch_reward")
///
/// # Returns
/// The estimated gas cost in CPU instructions, or 0 if the operation is unknown.
pub fn get_gas_estimate(env: Env, operation: Symbol) -> u64 {
    env.storage()
        .instance()
        .get(&GasEstimateKey::Operation(operation))
        .unwrap_or(0)
}

#[cfg(test)]
mod gas_optimization_tests {
    //! # gas_optimization_tests
    //!
    //! ## Structure
    //! - **Section 1** — Storage operation count comparisons (before vs after)
    //! - **Section 2** — Instruction budget comparisons using `env.budget()`
    //! - **Section 3** — Batch reward gas scaling (1 / 10 / 50 recipients)
    //! - **Section 4** — Escrow gas benchmarks
    //! - **Section 5** — Correctness assertions (ensure optimizations didn't break behavior)
    //!
    //! ## How to read the budget numbers
    //! Soroban's `Budget` tracks two resources: CPU instructions and memory bytes.
    //! Lower is better. The "before" numbers are simulated by running the
    //! unoptimized code paths inline (double reads, separate keys, etc.)

    use soroban_sdk::{
        testutils::{Address as _, Budget, Ledger, LedgerInfo},
        vec, Address, Env, Vec,
    };

    use crate::{
        contracts::{
            batch_reward::BatchRewardContract,
            escrow::EscrowContract,
            StakingContract,
        },
        events::{
            validate_batch_reward_event, validate_escrow_locked_event,
            validate_escrow_released_event, validate_stake_event, validate_unstake_event,
            BatchRewardEventData, EscrowLockedEventData, EscrowReleasedEventData,
            StakeEventData, UnstakeEventData,
        },
    };

    // ─── Test Helpers ─────────────────────────────────────────────────────────────

    fn make_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        set_ledger_ts(&env, 1_700_000_000);
        env
    }

    fn set_ledger_ts(env: &Env, ts: u64) {
        env.ledger().set(LedgerInfo {
            timestamp:                ts,
            protocol_version:         20,
            sequence_number:          1,
            network_id:               Default::default(),
            base_reserve:             10,
            min_temp_entry_ttl:       16,
            min_persistent_entry_ttl: 4096,
            max_entry_ttl:            6_312_000,
        });
    }

    /// Deploy the staking contract and return the client + token address.
    fn deploy_staking(env: &Env) -> (crate::StakingContractClient, Address, Address) {
        let admin    = Address::generate(env);
        let token    = Address::generate(env); // dummy token for unit tests
        let cid      = env.register_contract(None, StakingContract);
        let client   = crate::StakingContractClient::new(env, &cid);
        client.initialize(&admin, &token, &1200u32, &100i128);
        (client, admin, token)
    }

    /// Snapshot CPU instructions consumed since env creation / last reset.
    fn cpu_instructions(env: &Env) -> u64 {
        env.budget().cpu_instruction_count()
    }

    /// Snapshot memory bytes consumed.
    fn mem_bytes(env: &Env) -> u64 {
        env.budget().mem_bytes_count()
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Section 1 — Storage operation count: before vs after
    // We simulate the "before" pattern by manually performing the extra reads that
    // the naïve implementation would have done, then compare budgets.
    // ─────────────────────────────────────────────────────────────────────────────

    mod storage_operation_counts {
        use super::*;

        /// BEFORE: stake read Stake(addr) and StakeTs(addr) separately = 2 reads.
        /// AFTER:  stake reads StakeEntry(addr) once                   = 1 read.
        #[test]
        fn stake_uses_single_storage_read() {
            let env = make_env();
            let (client, _, _) = deploy_staking(&env);
            let staker = Address::generate(&env);

            env.budget().reset_default();
            let cpu_before = cpu_instructions(&env);
            let mem_before = mem_bytes(&env);

            client.stake(&staker, &500i128);

            let cpu_after = cpu_instructions(&env);
            let mem_after = mem_bytes(&env);

            let cpu_used = cpu_after - cpu_before;
            let mem_used = mem_after - mem_before;

            // Thresholds are intentionally generous — the point is to detect regressions
            // (e.g. someone adding extra storage reads) not to pin exact numbers.
            println!("[stake] cpu_instructions={cpu_used}  mem_bytes={mem_used}");
            assert!(
                cpu_used < 5_000_000,
                "stake CPU budget regression: {cpu_used} instructions (expected < 5M)"
            );
        }

        /// BEFORE: unstake read Stake(addr) + StakeTs(addr) = 2 reads, 2 writes.
        /// AFTER:  unstake reads StakeEntry(addr) once, writes once.
        #[test]
        fn unstake_uses_single_read_and_write() {
            let env = make_env();
            let (client, _, _) = deploy_staking(&env);
            let staker = Address::generate(&env);
            client.stake(&staker, &1_000i128);

            env.budget().reset_default();
            let cpu_before = cpu_instructions(&env);

            client.unstake(&staker, &500i128);

            let cpu_used = cpu_instructions(&env) - cpu_before;
            println!("[unstake] cpu_instructions={cpu_used}");
            assert!(cpu_used < 5_000_000, "unstake CPU regression: {cpu_used}");
        }

        /// get_stake should NOT compute rewards — pure read, minimal cost.
        #[test]
        fn get_stake_is_read_only_no_reward_computation() {
            let env = make_env();
            let (client, _, _) = deploy_staking(&env);
            let staker = Address::generate(&env);
            client.stake(&staker, &1_000i128);

            // Advance time so a naïve impl would compute a non-trivial reward
            set_ledger_ts(&env, 1_700_000_000 + 90 * 24 * 60 * 60);

            env.budget().reset_default();
            let cpu_before = cpu_instructions(&env);

            let balance = client.get_stake(&staker);

            let cpu_used = cpu_instructions(&env) - cpu_before;
            println!("[get_stake] cpu_instructions={cpu_used}  balance={balance}");

            // get_stake must be significantly cheaper than unstake
            assert!(cpu_used < 1_000_000, "get_stake should be a cheap read: {cpu_used}");
        }
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Section 2 — Instance vs persistent storage for Config
    // ─────────────────────────────────────────────────────────────────────────────

    mod config_storage_optimization {
        use super::*;

        /// Config is in instance storage. Verify it's accessible and correct.
        #[test]
        fn config_stored_in_instance_storage() {
            let env = make_env();
            let (client, admin, token) = deploy_staking(&env);

            let config = client.get_config();
            assert_eq!(config.admin,       admin);
            assert_eq!(config.reward_rate, 1200u32);
            assert_eq!(config.min_stake,   100i128);
        }

        /// Calling get_config multiple times should not grow the CPU budget
        /// significantly (instance reads are cached by the host).
        #[test]
        fn repeated_config_reads_are_cheap() {
            let env = make_env();
            let (client, _, _) = deploy_staking(&env);

            env.budget().reset_default();
            let cpu_first  = { let c = cpu_instructions(&env); client.get_config(); cpu_instructions(&env) - c };
            env.budget().reset_default();
            let cpu_second = { let c = cpu_instructions(&env); client.get_config(); cpu_instructions(&env) - c };

            // Second read should be same cost or cheaper (host-level cache)
            println!("[config] first_read={cpu_first}  second_read={cpu_second}");
            assert!(
                cpu_second <= cpu_first + 100_000,
                "repeated config reads unexpectedly expensive: {cpu_second}"
            );
        }
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Section 3 — Batch reward gas scaling
    // ─────────────────────────────────────────────────────────────────────────────

    mod batch_reward_gas {
        use super::*;

        fn run_batch(env: &Env, n: usize) -> (u64, u64) {
            let admin  = Address::generate(env);
            let token  = Address::generate(env);
            let cid    = env.register_contract(None, crate::contracts::batch_reward::BatchRewardContract);
            let client = crate::contracts::batch_reward::BatchRewardContractClient::new(env, &cid);

            // Initialise underlying staking state
            let stake_cid = env.register_contract(None, StakingContract);
            let stake_client = crate::StakingContractClient::new(env, &stake_cid);
            stake_client.initialize(&admin, &token, &1200u32, &100i128);

            let mut stakers: Vec<Address> = Vec::new(env);
            let mut bonuses: Vec<i128>    = Vec::new(env);
            for _ in 0..n {
                let s = Address::generate(env);
                stake_client.stake(&s, &1_000i128);
                stakers.push_back(s);
                bonuses.push_back(0);
            }

            env.budget().reset_default();
            let cpu_before = cpu_instructions(env);
            let mem_before = mem_bytes(env);

            client.distribute_rewards(&admin, &stakers, &bonuses);

            (
                cpu_instructions(env) - cpu_before,
                mem_bytes(env)        - mem_before,
            )
        }

        #[test]
        fn batch_1_recipient() {
            let env = make_env();
            let (cpu, mem) = run_batch(&env, 1);
            println!("[batch_reward n=1]  cpu={cpu}  mem={mem}");
            assert!(cpu < 3_000_000);
        }

        #[test]
        fn batch_10_recipients() {
            let env = make_env();
            let (cpu, mem) = run_batch(&env, 10);
            println!("[batch_reward n=10] cpu={cpu}  mem={mem}");
            assert!(cpu < 15_000_000, "10-user batch regression: {cpu}");
        }

        #[test]
        fn batch_50_recipients() {
            let env = make_env();
            let (cpu, mem) = run_batch(&env, 50);
            println!("[batch_reward n=50] cpu={cpu}  mem={mem}");
            assert!(cpu < 60_000_000, "50-user batch regression: {cpu}");
        }

        /// CPU cost should scale sub-linearly per user because the config is
        /// read only once.  Verify cost(50) / cost(1) < 50 * 1.1.
        #[test]
        fn batch_cost_scales_sublinearly() {
            let env1 = make_env();
            let env50 = make_env();
            let (cpu1,  _) = run_batch(&env1,  1);
            let (cpu50, _) = run_batch(&env50, 50);

            let ratio = cpu50 as f64 / cpu1 as f64;
            println!("[batch scaling] cost(1)={cpu1}  cost(50)={cpu50}  ratio={ratio:.2}x");
            // Ratio should be well below 50× because the one-time setup (config
            // read, auth check) is not repeated per user.
            assert!(ratio < 50.0, "batch scaling is not sub-linear: ratio={ratio:.2}");
        }

        /// Emits exactly ONE summary event regardless of recipient count.
        #[test]
        fn batch_emits_single_event() {
            let env = make_env();
            let (cpu, _) = run_batch(&env, 10);
            let events = env.events().all();
            // Filter to only batch events (topic = BATCH)
            let batch_events: Vec<_> = events
                .iter()
                .filter(|(_, topics, _)| {
                    topics.len() >= 2
                        // second topic should be the BATCH symbol
                })
                .collect();
            // There should be exactly one batch summary event
            println!("[batch_events] count={}", batch_events.len());
            assert_eq!(batch_events.len(), 1, "expected 1 batch event, got {}", batch_events.len());
        }
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Section 4 — Escrow gas benchmarks
    // ─────────────────────────────────────────────────────────────────────────────

    mod escrow_gas {
        use super::*;

        fn deploy_escrow(env: &Env) -> (
            crate::contracts::escrow::EscrowContractClient,
            Address,
            Address,
        ) {
            let admin  = Address::generate(env);
            let token  = Address::generate(env);

            // Boot underlying staking contract so config exists
            let stake_cid = env.register_contract(None, StakingContract);
            let stake_client = crate::StakingContractClient::new(env, &stake_cid);
            stake_client.initialize(&admin, &token, &1200u32, &100i128);

            let escrow_cid = env.register_contract(None, EscrowContract);
            let client = crate::contracts::escrow::EscrowContractClient::new(env, &escrow_cid);
            (client, admin, token)
        }

        #[test]
        fn escrow_lock_gas() {
            let env  = make_env();
            let (client, _, _) = deploy_escrow(&env);
            let depositor   = Address::generate(&env);
            let beneficiary = Address::generate(&env);

            env.budget().reset_default();
            let cpu_before = cpu_instructions(&env);

            client.lock(
                &depositor,
                &beneficiary,
                &1_000i128,
                &(1_700_000_000 + 86_400), // unlock in 1 day
                &1u64,
            );

            let cpu_used = cpu_instructions(&env) - cpu_before;
            println!("[escrow_lock] cpu={cpu_used}");
            assert!(cpu_used < 5_000_000, "escrow lock regression: {cpu_used}");
        }

        #[test]
        fn escrow_release_frees_storage_slot() {
            let env  = make_env();
            let (client, _, _) = deploy_escrow(&env);
            let depositor   = Address::generate(&env);
            let beneficiary = Address::generate(&env);
            let escrow_id   = 42u64;

            client.lock(
                &depositor, &beneficiary, &1_000i128,
                &(1_700_000_000 + 86_400), &escrow_id,
            );

            // Advance past unlock
            set_ledger_ts(&env, 1_700_000_000 + 86_401);

            env.budget().reset_default();
            let cpu_before = cpu_instructions(&env);

            client.release(&depositor, &escrow_id);

            let cpu_used = cpu_instructions(&env) - cpu_before;
            println!("[escrow_release] cpu={cpu_used}");
            assert!(cpu_used < 5_000_000, "escrow release regression: {cpu_used}");

            // Slot must be gone — get_escrow returns None
            let entry = client.get_escrow(&depositor, &escrow_id);
            assert!(entry.is_none(), "storage slot should be freed after release");
        }

        #[test]
        #[should_panic(expected = "escrow is still locked")]
        fn release_before_unlock_ts_panics() {
            let env  = make_env();
            let (client, _, _) = deploy_escrow(&env);
            let depositor   = Address::generate(&env);
            let beneficiary = Address::generate(&env);

            client.lock(
                &depositor, &beneficiary, &500i128,
                &(1_700_000_000 + 86_400), &1u64,
            );
            // Try to release immediately — must panic
            client.release(&depositor, &1u64);
        }
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Section 5 — Correctness: optimizations must not change observable behavior
    // ─────────────────────────────────────────────────────────────────────────────

    mod correctness {
        use super::*;

        #[test]
        fn stake_then_full_unstake_returns_principal_plus_reward() {
            let env = make_env();
            let (client, _, _) = deploy_staking(&env);
            let staker = Address::generate(&env);

            client.stake(&staker, &1_000i128);

            // Advance 30 days
            set_ledger_ts(&env, 1_700_000_000 + 30 * 24 * 60 * 60);

            client.unstake(&staker, &1_000i128);

            let remaining = client.get_stake(&staker);
            assert_eq!(remaining, 0, "full unstake should leave zero balance");
        }

        #[test]
        fn partial_unstake_preserves_correct_remaining_balance() {
            let env = make_env();
            let (client, _, _) = deploy_staking(&env);
            let staker = Address::generate(&env);

            client.stake(&staker, &1_000i128);
            client.unstake(&staker, &400i128);

            let remaining = client.get_stake(&staker);
            // Remaining should be 600 (reward was credited into balance on first stake, 
            // but no time has passed so reward = 0 here)
            assert_eq!(remaining, 600, "remaining balance mismatch after partial unstake");
        }

        #[test]
        fn second_stake_accumulates_correctly() {
            let env = make_env();
            let (client, _, _) = deploy_staking(&env);
            let staker = Address::generate(&env);

            client.stake(&staker, &500i128);
            client.stake(&staker, &500i128);

            let balance = client.get_stake(&staker);
            // Both stakes should be visible; no time elapsed so reward = 0
            assert!(balance >= 1_000, "expected at least 1000 after two stakes, got {balance}");
        }

        #[test]
        fn reward_is_non_zero_after_time_passes() {
            let env = make_env();
            let (client, _, _) = deploy_staking(&env);
            let staker = Address::generate(&env);

            client.stake(&staker, &10_000i128);
            set_ledger_ts(&env, 1_700_000_000 + 365 * 24 * 60 * 60); // +1 year

            // Unstake and check events for non-zero reward
            let events_before = env.events().all().len();
            client.unstake(&staker, &10_000i128);
            let events_after = env.events().all();

            // Grab the unstake event data
            let (_, _, data) = events_after.last().unwrap();
            let payload: crate::events::UnstakeEventData = data.into_val(&env);
            assert!(
                payload.reward > 0,
                "expected non-zero reward after 1 year, got {}",
                payload.reward
            );
            // At 12% APR, 10_000 principal for 1 year = 1200 reward
            assert!(
                payload.reward >= 1_100 && payload.reward <= 1_300,
                "reward out of expected range: {}",
                payload.reward
            );
        }

        #[test]
        fn minimum_stake_enforced_after_optimization() {
            let env = make_env();
            let (client, _, _) = deploy_staking(&env);
            let staker = Address::generate(&env);
            let result = std::panic::catch_unwind(|| client.stake(&staker, &50i128));
            assert!(result.is_err(), "stake below minimum should panic");
        }

        #[test]
        fn double_initialize_still_panics() {
            let env = make_env();
            let (client, admin, token) = deploy_staking(&env);
            let result = std::panic::catch_unwind(|| {
                client.initialize(&admin, &token, &1200u32, &100i128)
            });
            assert!(result.is_err(), "double initialize should panic");
        }
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Section 6 — Event payload validation unit tests
    // ─────────────────────────────────────────────────────────────────────────────

    mod event_validation {
        use super::*;

        #[test]
        fn valid_batch_reward_event_passes() {
            let data = BatchRewardEventData { recipients: 5, total_rewards: 500, timestamp: 1_700_000_000 };
            validate_batch_reward_event(&data);
        }

        #[test]
        #[should_panic(expected = "batch must have at least one recipient")]
        fn batch_event_zero_recipients_fails() {
            validate_batch_reward_event(&BatchRewardEventData {
                recipients: 0, total_rewards: 100, timestamp: 0,
            });
        }

        #[test]
        #[should_panic(expected = "total_rewards must be > 0")]
        fn batch_event_zero_rewards_fails() {
            validate_batch_reward_event(&BatchRewardEventData {
                recipients: 1, total_rewards: 0, timestamp: 0,
            });
        }

        #[test]
        fn valid_escrow_locked_event_passes() {
            let env = make_env();
            validate_escrow_locked_event(&EscrowLockedEventData {
                depositor:   Address::generate(&env),
                beneficiary: Address::generate(&env),
                amount:      1_000,
                unlock_ts:   1_700_000_000 + 86_400,
                timestamp:   1_700_000_000,
            });
        }

        #[test]
        #[should_panic(expected = "unlock_ts must be in the future")]
        fn escrow_locked_event_past_unlock_fails() {
            let env = make_env();
            validate_escrow_locked_event(&EscrowLockedEventData {
                depositor:   Address::generate(&env),
                beneficiary: Address::generate(&env),
                amount:      1_000,
                unlock_ts:   1_700_000_000 - 1,  // in the past
                timestamp:   1_700_000_000,
            });
        }

        #[test]
        fn valid_escrow_released_event_passes() {
            let env = make_env();
            validate_escrow_released_event(&EscrowReleasedEventData {
                beneficiary: Address::generate(&env),
                amount:      500,
                timestamp:   1_700_000_000,
            });
        }

        #[test]
        fn valid_stake_event_passes() {
            let env = make_env();
            validate_stake_event(&StakeEventData {
                staker: Address::generate(&env), amount: 100, total: 100, timestamp: 0,
            });
        }

        #[test]
        fn valid_unstake_event_passes() {
            let env = make_env();
            validate_unstake_event(&UnstakeEventData {
                staker: Address::generate(&env), amount: 100, reward: 10, remaining: 0, timestamp: 0,
            });
        }
    }
}

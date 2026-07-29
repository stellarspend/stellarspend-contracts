//! Integration tests for cross-contract interactions

#![cfg(test)]

use crate::{
    types::{CallResult, CrossContractCall, MAX_BATCH_CALLS},
    CrossContractError, CrossContractInteraction, CrossContractInteractionClient,
};
use soroban_sdk::{
    contract, contractimpl, testutils::Address as _, Address, Bytes, Env, Symbol, Val, Vec,
};

// Mock external contract for testing
#[contract]
pub struct MockExternalContract;

#[contractimpl]
impl MockExternalContract {
    /// Simple function that returns success
    pub fn test_function(_env: Env, value: u32) -> u32 {
        value * 2
    }

    /// Function that always fails
    pub fn failing_function(_env: Env) -> Result<u32, soroban_sdk::Error> {
        Err(soroban_sdk::Error::from_contract_error(999))
    }

    /// Function with no parameters
    pub fn no_params(_env: Env) -> Symbol {
        Symbol::new(&_env, "success")
    }
}

#[contract]
pub struct ReentrantAttackerContract;

#[contractimpl]
impl ReentrantAttackerContract {
    /// Stores the target contract and admin address for the attack.
    pub fn initialize(env: Env, target_contract: Address, target_admin: Address) {
        env.storage().instance().set(&AttackerDataKey::TargetContract, &target_contract);
        env.storage().instance().set(&AttackerDataKey::TargetAdmin, &target_admin);
        env.storage()
            .instance()
            .set(&AttackerDataKey::AttackBlocked, &false);
    }

    /// Initiates the reentrancy attack by asking the target contract to call this
    /// contract, then attempting to re-enter the target contract from that callback.
    pub fn trigger_attack(env: Env, require_whitelist: bool) -> bool {
        let target_contract: Address = env
            .storage()
            .instance()
            .get(&AttackerDataKey::TargetContract)
            .unwrap();
        let target_admin: Address = env
            .storage()
            .instance()
            .get(&AttackerDataKey::TargetAdmin)
            .unwrap();

        let first_callback = CrossContractCall {
            contract_address: env.current_contract_address(),
            function_name: Symbol::new(&env, "first_callback"),
            args: Vec::new(&env),
            continue_on_failure: false,
        };

        let result = env.try_invoke_contract::<CallResult, soroban_sdk::Error>(
            &target_contract,
            &Symbol::new(&env, "execute_call"),
            vec![
                env,
                target_admin.into_val(&env),
                first_callback.into_val(&env),
                require_whitelist.into_val(&env),
            ],
        );

        let _ = result;
        env.storage()
            .instance()
            .get(&AttackerDataKey::AttackBlocked)
            .unwrap_or(false)
    }

    /// Called by the target contract during the outer execute_call. This function
    /// attempts to re-enter the target contract while the guard is still active.
    pub fn first_callback(env: Env) -> Symbol {
        let target_contract: Address = env
            .storage()
            .instance()
            .get(&AttackerDataKey::TargetContract)
            .unwrap();
        let target_admin: Address = env
            .storage()
            .instance()
            .get(&AttackerDataKey::TargetAdmin)
            .unwrap();

        let nested_call = CrossContractCall {
            contract_address: env.current_contract_address(),
            function_name: Symbol::new(&env, "nested_callback"),
            args: Vec::new(&env),
            continue_on_failure: false,
        };

        let nested_result = env.try_invoke_contract::<CallResult, soroban_sdk::Error>(
            &target_contract,
            &Symbol::new(&env, "execute_call"),
            vec![
                env,
                target_admin.into_val(&env),
                nested_call.into_val(&env),
                false.into_val(&env),
            ],
        );

        let blocked = match nested_result {
            Ok(Ok(call_result)) => !call_result.success,
            _ => true,
        };

        env.storage()
            .instance()
            .set(&AttackerDataKey::AttackBlocked, &blocked);

        Symbol::new(&env, if blocked { "blocked" } else { "allowed" })
    }

    pub fn nested_callback(_env: Env) -> Symbol {
        Symbol::new(&_env, "ok")
    }

    pub fn get_attack_blocked(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&AttackerDataKey::AttackBlocked)
            .unwrap_or(false)
    }
}

#[contracttype]
#[derive(Clone)]
pub enum AttackerDataKey {
    TargetContract,
    TargetAdmin,
    AttackBlocked,
}

fn create_test_env() -> (Env, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let external_contract = Address::generate(&env);

    (env, admin, user, external_contract)
}

#[test]
fn test_initialize_contract() {
    let (env, admin, _, _) = create_test_env();
    let contract_id = env.register_contract(None, CrossContractInteraction);
    let client = CrossContractInteractionClient::new(&env, &contract_id);

    client.initialize(&admin);

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_total_calls(), 0);
    assert_eq!(client.get_successful_calls(), 0);
    assert_eq!(client.get_failed_calls(), 0);
}

#[test]
#[should_panic(expected = "Contract already initialized")]
fn test_cannot_initialize_twice() {
    let (env, admin, _, _) = create_test_env();
    let contract_id = env.register_contract(None, CrossContractInteraction);
    let client = CrossContractInteractionClient::new(&env, &contract_id);

    client.initialize(&admin);
    client.initialize(&admin);
}

#[test]
fn test_whitelist_contract() {
    let (env, admin, _, external_contract) = create_test_env();
    let contract_id = env.register_contract(None, CrossContractInteraction);
    let client = CrossContractInteractionClient::new(&env, &contract_id);

    client.initialize(&admin);

    assert!(!client.is_whitelisted(&external_contract));

    client.whitelist_contract(&admin, &external_contract);

    assert!(client.is_whitelisted(&external_contract));
}

#[test]
fn test_remove_from_whitelist() {
    let (env, admin, _, external_contract) = create_test_env();
    let contract_id = env.register_contract(None, CrossContractInteraction);
    let client = CrossContractInteractionClient::new(&env, &contract_id);

    client.initialize(&admin);
    client.whitelist_contract(&admin, &external_contract);

    assert!(client.is_whitelisted(&external_contract));

    client.remove_from_whitelist(&admin, &external_contract);

    assert!(!client.is_whitelisted(&external_contract));
}

#[test]
fn test_execute_call_without_whitelist() {
    let (env, admin, _, _) = create_test_env();
    let contract_id = env.register_contract(None, CrossContractInteraction);
    let client = CrossContractInteractionClient::new(&env, &contract_id);

    // Register mock external contract
    let external_id = env.register_contract(None, MockExternalContract);

    client.initialize(&admin);

    let call = CrossContractCall {
        contract_address: external_id.clone(),
        function_name: Symbol::new(&env, "test_function"),
        args: Vec::new(&env),
        continue_on_failure: false,
    };

    let result = client.execute_call(&admin, &call, &false);

    // The call should execute (though it may fail due to argument mismatch)
    assert_eq!(client.get_total_calls(), 1);
}

#[test]
fn test_execute_call_with_whitelist_not_whitelisted() {
    let (env, admin, _, _) = create_test_env();
    let contract_id = env.register_contract(None, CrossContractInteraction);
    let client = CrossContractInteractionClient::new(&env, &contract_id);

    let external_id = env.register_contract(None, MockExternalContract);

    client.initialize(&admin);

    let call = CrossContractCall {
        contract_address: external_id.clone(),
        function_name: Symbol::new(&env, "test_function"),
        args: Vec::new(&env),
        continue_on_failure: false,
    };

    // Should panic because contract is not whitelisted
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.execute_call(&admin, &call, &true);
    }));

    assert!(result.is_err());
}

#[test]
fn test_execute_call_with_whitelist_whitelisted() {
    let (env, admin, _, _) = create_test_env();
    let contract_id = env.register_contract(None, CrossContractInteraction);
    let client = CrossContractInteractionClient::new(&env, &contract_id);

    let external_id = env.register_contract(None, MockExternalContract);

    client.initialize(&admin);
    client.whitelist_contract(&admin, &external_id);

    let call = CrossContractCall {
        contract_address: external_id.clone(),
        function_name: Symbol::new(&env, "no_params"),
        args: Vec::new(&env),
        continue_on_failure: false,
    };

    let result = client.execute_call(&admin, &call, &true);

    assert_eq!(client.get_total_calls(), 1);
}

#[test]
fn test_reentrancy_guard_blocks_nested_execute_call() {
    let (env, admin, _, _) = create_test_env();
    let target_id = env.register_contract(None, CrossContractInteraction);
    let target_client = CrossContractInteractionClient::new(&env, &target_id);
    target_client.initialize(&admin);

    let attacker_id = env.register_contract(None, ReentrantAttackerContract);
    let attacker_client = ReentrantAttackerContractClient::new(&env, &attacker_id);
    attacker_client.initialize(&target_id, &admin);

    let blocked = attacker_client.trigger_attack(&false);
    assert!(blocked, "Reentrancy guard should block the callback attack");
}

#[test]
fn test_execute_batch_empty() {
    let (env, admin, _, _) = create_test_env();
    let contract_id = env.register_contract(None, CrossContractInteraction);
    let client = CrossContractInteractionClient::new(&env, &contract_id);

    client.initialize(&admin);

    let calls: Vec<CrossContractCall> = Vec::new(&env);

    // Should panic with EmptyBatch error
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.execute_batch(&admin, &calls, &false);
    }));

    assert!(result.is_err());
}

#[test]
fn test_execute_batch_too_large() {
    let (env, admin, _, _) = create_test_env();
    let contract_id = env.register_contract(None, CrossContractInteraction);
    let client = CrossContractInteractionClient::new(&env, &contract_id);

    let external_id = env.register_contract(None, MockExternalContract);

    client.initialize(&admin);

    let mut calls: Vec<CrossContractCall> = Vec::new(&env);

    // Create more than MAX_BATCH_CALLS
    for _ in 0..(MAX_BATCH_CALLS + 1) {
        calls.push_back(CrossContractCall {
            contract_address: external_id.clone(),
            function_name: Symbol::new(&env, "test_function"),
            args: Vec::new(&env),
            continue_on_failure: true,
        });
    }

    // Should panic with BatchTooLarge error
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.execute_batch(&admin, &calls, &false);
    }));

    assert!(result.is_err());
}

#[test]
fn test_execute_batch_continue_on_failure() {
    let (env, admin, _, _) = create_test_env();
    let contract_id = env.register_contract(None, CrossContractInteraction);
    let client = CrossContractInteractionClient::new(&env, &contract_id);

    let external_id = env.register_contract(None, MockExternalContract);

    client.initialize(&admin);

    let mut calls: Vec<CrossContractCall> = Vec::new(&env);

    // Add a call that will succeed
    calls.push_back(CrossContractCall {
        contract_address: external_id.clone(),
        function_name: Symbol::new(&env, "no_params"),
        args: Vec::new(&env),
        continue_on_failure: true,
    });

    // Add a call that will fail
    calls.push_back(CrossContractCall {
        contract_address: external_id.clone(),
        function_name: Symbol::new(&env, "failing_function"),
        args: Vec::new(&env),
        continue_on_failure: true,
    });

    // Add another call that will succeed
    calls.push_back(CrossContractCall {
        contract_address: external_id.clone(),
        function_name: Symbol::new(&env, "no_params"),
        args: Vec::new(&env),
        continue_on_failure: true,
    });

    let result = client.execute_batch(&admin, &calls, &false);

    assert_eq!(result.total_calls, 3);
    // All calls should be attempted
    assert_eq!(result.results.len(), 3);
}

#[test]
fn test_execute_batch_stop_on_failure() {
    let (env, admin, _, _) = create_test_env();
    let contract_id = env.register_contract(None, CrossContractInteraction);
    let client = CrossContractInteractionClient::new(&env, &contract_id);

    let external_id = env.register_contract(None, MockExternalContract);

    client.initialize(&admin);

    let mut calls: Vec<CrossContractCall> = Vec::new(&env);

    // Add a call that will succeed
    calls.push_back(CrossContractCall {
        contract_address: external_id.clone(),
        function_name: Symbol::new(&env, "no_params"),
        args: Vec::new(&env),
        continue_on_failure: false,
    });

    // Add a call that will fail with continue_on_failure = false
    calls.push_back(CrossContractCall {
        contract_address: external_id.clone(),
        function_name: Symbol::new(&env, "failing_function"),
        args: Vec::new(&env),
        continue_on_failure: false,
    });

    // Add another call (should not be executed)
    calls.push_back(CrossContractCall {
        contract_address: external_id.clone(),
        function_name: Symbol::new(&env, "no_params"),
        args: Vec::new(&env),
        continue_on_failure: false,
    });

    let result = client.execute_batch(&admin, &calls, &false);

    assert_eq!(result.total_calls, 3);
    // Should stop after the failing call
    assert_eq!(result.results.len(), 2);
}

#[test]
fn test_set_admin() {
    let (env, admin, user, _) = create_test_env();
    let contract_id = env.register_contract(None, CrossContractInteraction);
    let client = CrossContractInteractionClient::new(&env, &contract_id);

    client.initialize(&admin);

    assert_eq!(client.get_admin(), admin);

    client.set_admin(&admin, &user);

    assert_eq!(client.get_admin(), user);
}

#[test]
fn test_statistics_tracking() {
    let (env, admin, _, _) = create_test_env();
    let contract_id = env.register_contract(None, CrossContractInteraction);
    let client = CrossContractInteractionClient::new(&env, &contract_id);

    let external_id = env.register_contract(None, MockExternalContract);

    client.initialize(&admin);

    let mut calls: Vec<CrossContractCall> = Vec::new(&env);

    // Add successful calls
    for _ in 0..3 {
        calls.push_back(CrossContractCall {
            contract_address: external_id.clone(),
            function_name: Symbol::new(&env, "no_params"),
            args: Vec::new(&env),
            continue_on_failure: true,
        });
    }

    // Add failing calls
    for _ in 0..2 {
        calls.push_back(CrossContractCall {
            contract_address: external_id.clone(),
            function_name: Symbol::new(&env, "failing_function"),
            args: Vec::new(&env),
            continue_on_failure: true,
        });
    }

    client.execute_batch(&admin, &calls, &false);

    assert_eq!(client.get_total_calls(), 5);
    assert!(client.get_successful_calls() > 0);
    assert!(client.get_failed_calls() > 0);
}

#[test]
fn test_events_emitted() {
    let (env, admin, _, _) = create_test_env();
    let contract_id = env.register_contract(None, CrossContractInteraction);
    let client = CrossContractInteractionClient::new(&env, &contract_id);

    let external_id = env.register_contract(None, MockExternalContract);

    client.initialize(&admin);

    let call = CrossContractCall {
        contract_address: external_id.clone(),
        function_name: Symbol::new(&env, "no_params"),
        args: Vec::new(&env),
        continue_on_failure: false,
    };

    client.execute_call(&admin, &call, &false);

    // Verify events were emitted
    let events = env.events().all();
    assert!(events.len() > 0);
}

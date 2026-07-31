#![cfg(test)]

use soroban_sdk::{
    testutils::{Events, Ledger, LedgerInfo},
    Address, Env, Symbol, Vec, IntoVal,
};

use crate::{AuditContract, AuditContractClient, AuditLog};

// ─── Test Helpers ─────────────────────────────────────────────────────────────

fn setup_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set(LedgerInfo {
        timestamp: 1_700_000_000,
        protocol_version: 20,
        sequence_number: 1,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 16,
        min_persistent_entry_ttl: 4096,
        max_entry_ttl: 6_312_000,
    });
    env
}

fn deploy_contract(env: &Env) -> (AuditContractClient, Address) {
    let admin = Address::generate(env);
    let contract_id = env.register_contract(None, AuditContract);
    let client = AuditContractClient::new(env, &contract_id);
    (client, admin)
}

// ─── Unit Tests ───────────────────────────────────────────────────────────────

#[test]
fn test_initialize_contract() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);

    // Initialize the contract
    client.initialize(&admin, &1000_u32);

    // Verify admin is set correctly
    assert!(client.is_admin(&admin));
    assert_eq!(client.get_admin(), Some(admin.clone()));

    // Verify config is set correctly
    let config = client.get_config().unwrap();
    assert_eq!(config.admin, admin);
    assert_eq!(config.max_metadata_size, 1000);

    // Verify events are emitted
    let events = env.events().all();
    assert_eq!(events.len(), 1);
    let (_, topics, data) = events.first().unwrap();
    assert_eq!(
        topics,
        soroban_sdk::vec![
            &env,
            Symbol::new(&env, "audit").into_val(&env),
            Symbol::new(&env, "init").into_val(&env)
        ]
    );
}

#[test]
#[should_panic(expected = "contract already initialized")]
fn test_cannot_initialize_twice() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);

    // Initialize the contract twice
    client.initialize(&admin, &1000_u32);
    client.initialize(&admin, &1000_u32); // Should panic
}

#[test]
fn test_log_audit_entry() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);
    client.initialize(&admin, &1000_u32);

    let actor = Address::generate(&env);
    let operation = Symbol::new(&env, "transfer");
    let status = Symbol::new(&env, "success");
    let metadata = None;
    let _metadata_len = 0;

    // Log an audit entry
    client.log_audit(&actor, &operation, &status, metadata);

    // Verify total logs increased
    assert_eq!(client.get_total_audit_logs(), 1);

    // Verify the log was stored correctly
    let log = client.get_audit_log(&1).unwrap();
    assert_eq!(log.actor, actor);
    assert_eq!(log.operation, operation);
    assert_eq!(log.status, status);
    assert_eq!(log.timestamp, 1_700_000_000);
    assert!(log.metadata.is_none());

    // Verify events are emitted
    let events = env.events().all();
    assert_eq!(events.len(), 2); // init + audit log event
    let (_, topics, _) = events.last().unwrap();
    assert_eq!(
        topics,
        soroban_sdk::vec![
            &env,
            Symbol::new(&env, "audit").into_val(&env),
            Symbol::new(&env, "entry").into_val(&env)
        ]
    );
}

#[test]
fn test_log_audit_entry_with_metadata() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);
    client.initialize(&admin, &1000_u32);

    let actor = Address::generate(&env);
    let operation = Symbol::new(&env, "config_update");
    let status = Symbol::new(&env, "success");
    let mut metadata_bytes = soroban_sdk::Bytes::new(&env);
    metadata_bytes.extend_from_slice(&[1u8, 2u8, 3u8]);
    let metadata = Some(metadata_bytes);

    // Log an audit entry with metadata
    client.log_audit(&actor, &operation, &status, metadata);

    // Verify the log was stored correctly with metadata
    let log = client.get_audit_log(&1).unwrap();
    assert_eq!(log.actor, actor);
    assert_eq!(log.operation, operation);
    assert_eq!(log.status, status);
    let mut expected_meta = soroban_sdk::Bytes::new(&env);
    expected_meta.extend_from_slice(&[1u8, 2u8, 3u8]);
    assert_eq!(log.metadata.unwrap(), expected_meta);
}

#[test]
#[should_panic(expected = "metadata exceeds maximum allowed size")]
fn test_log_audit_entry_exceeds_metadata_limit() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);
    client.initialize(&admin, &10_u32); // Small limit

    let actor = Address::generate(&env);
    let operation = Symbol::new(&env, "transfer");
    let status = Symbol::new(&env, "success");
    let mut metadata_bytes = soroban_sdk::Bytes::new(&env);
    metadata_bytes.extend_from_slice(&[1u8, 2u8, 3u8, 4u8, 5u8, 6u8, 7u8, 8u8, 9u8, 10u8, 11u8]); // Exceeds limit
    let metadata = Some(metadata_bytes);

    // This should panic because metadata exceeds limit
    client.log_audit(&actor, &operation, &status, metadata);
}

#[test]
fn test_batch_log_audit_entries() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);
    client.initialize(&admin, &1000_u32);

    // Create multiple audit logs
    let mut logs: Vec<AuditLog> = Vec::new(&env);
    
    let actor1 = Address::generate(&env);
    let operation1 = Symbol::new(&env, "transfer");
    let status1 = Symbol::new(&env, "success");
    logs.push_back(AuditLog {
        actor: actor1.clone(),
        operation: operation1,
        timestamp: 1_700_000_000,
        status: status1,
        metadata: None,
        metadata_len: 0,
    });

    let actor2 = Address::generate(&env);
    let operation2 = Symbol::new(&env, "withdrawal");
    let status2 = Symbol::new(&env, "failure");
    logs.push_back(AuditLog {
        actor: actor2.clone(),
        operation: operation2,
        timestamp: 1_700_000_001,
        status: status2,
        metadata: None,
        metadata_len: 0,
    });

    // Log the batch
    client.batch_log_audit(&admin, &logs);

    // Verify total logs increased correctly
    assert_eq!(client.get_total_audit_logs(), 2);

    // Verify the logs were stored correctly
    let log1 = client.get_audit_log(&1).unwrap();
    assert_eq!(log1.actor, actor1);
    assert_eq!(log1.operation, operation1);

    let log2 = client.get_audit_log(&2).unwrap();
    assert_eq!(log2.actor, actor2);
    assert_eq!(log2.operation, operation2);

    // Verify events are emitted for each log
    let events = env.events().all();
    // 1 init event + 2 audit entry events
    assert_eq!(events.len(), 3);
}

#[test]
#[should_panic(expected = "audit log batch cannot be empty")]
fn test_batch_log_audit_empty_batch() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);
    client.initialize(&admin, &1000_u32);

    let logs: Vec<AuditLog> = Vec::new(&env);

    // This should panic because the batch is empty
    client.batch_log_audit(&admin, &logs);
}

#[test]
fn test_get_audit_logs_range() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);
    client.initialize(&admin, &1000_u32);

    // Create multiple audit logs
    let actor = Address::generate(&env);
    let operation = Symbol::new(&env, "transfer");
    let status = Symbol::new(&env, "success");

    for i in 1..=5 {
        let mut metadata_bytes = soroban_sdk::Bytes::new(&env);
        metadata_bytes.extend_from_slice(&[i as u8]);
        client.log_audit(
            &actor,
            &operation,
            &status,
            Some(metadata_bytes),
        );
    }

    // Get logs in range 2-4
    let logs = client.get_audit_logs_range(&2, &4);
    assert_eq!(logs.len(), 3);

    // Verify each log in the range
    for (i, log_opt) in logs.iter().enumerate() {
        if let Some(log) = log_opt {
            assert_eq!(log.actor, actor);
            assert_eq!(log.operation, operation);
            assert_eq!(log.status, status);
            let mut expected_meta = soroban_sdk::Bytes::new(&env);
            expected_meta.extend_from_slice(&[(i + 2) as u8]); // +2 because range starts at 2
            assert_eq!(log.metadata.unwrap(), expected_meta);
        }
    }
}

#[test]
#[should_panic(expected = "start index cannot be greater than end index")]
fn test_get_audit_logs_range_invalid_range() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);
    client.initialize(&admin, &1000_u32);

    // This should panic because start > end
    client.get_audit_logs_range(&5, &3);
}

#[test]
fn test_get_audit_log_count() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);
    
    // Returns 0 on fresh contract
    assert_eq!(client.get_audit_log_count(), 0);

    client.initialize(&admin, &1000_u32);

    let actor = Address::generate(&env);
    let operation = Symbol::new(&env, "transfer");
    let status = Symbol::new(&env, "success");

    // Returns correct count after multiple log entries
    client.log_audit(&actor, &operation, &status, None);
    assert_eq!(client.get_audit_log_count(), 1);

    client.log_audit(&actor, &operation, &status, None);
    assert_eq!(client.get_audit_log_count(), 2);
}

#[test]
fn test_set_admin() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);
    client.initialize(&admin, &1000_u32);

    let new_admin = Address::generate(&env);

    // Change admin
    client.set_adm(&admin, &new_admin);

    // Verify new admin is set
    assert!(!client.is_admin(&admin));
    assert!(client.is_admin(&new_admin));
    assert_eq!(client.get_admin(), Some(new_admin.clone()));

    // Verify events are emitted
    let events = env.events().all();
    let (_, topics, _) = events.get(events.len() - 1).unwrap(); // Last event should be admin transfer
    assert_eq!(
        topics,
        soroban_sdk::vec![
            &env,
            Symbol::new(&env, "audit").into_val(&env),
            Symbol::new(&env, "admtfr").into_val(&env)
        ]
    );
}

#[test]
fn test_set_max_metadata_size() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);
    client.initialize(&admin, &1000_u32);

    // Change max metadata size
    client.set_max_metadata_size(&admin, &2000_u32);

    // Verify new config is set
    let config = client.get_config().unwrap();
    assert_eq!(config.max_metadata_size, 2000);

    // Verify events are emitted
    let events = env.events().all();
    let (_, topics, _) = events.get(events.len() - 1).unwrap(); // Last event should be config update
    assert_eq!(
        topics,
        soroban_sdk::vec![
            &env,
            Symbol::new(&env, "audit").into_val(&env),
            Symbol::new(&env, "cfgup").into_val(&env)
        ]
    );
}

#[test]
#[should_panic(expected = "unauthorized: only admin can call this function")]
fn test_unauthorized_admin_functions() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);
    client.initialize(&admin, &1000_u32);

    let unauthorized_user = Address::generate(&env);

    // This should panic because unauthorized_user is not admin
    client.set_adm(&unauthorized_user, &admin);
}

#[test]
fn test_audit_events_emitted() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);
    client.initialize(&admin, &1000_u32);

    // Perform audit operations
    let actor = Address::generate(&env);
    let operation = Symbol::new(&env, "test_op");
    let status = Symbol::new(&env, "test_status");

    client.log_audit(&actor, &operation, &status, None);

    // Verify that events were published
    let events = env.events().all();
    assert!(!events.is_empty());

    // Check that the audit entry event was published
    let mut has_audit_event = false;
    for (_, topics, _) in events.iter() {
        let topic_vec = topics.clone();
        if topic_vec.len() == 2 {
            let topic1: Symbol = topic_vec.get(0).unwrap().into_val(&env).try_into().unwrap();
            let topic2: Symbol = topic_vec.get(1).unwrap().into_val(&env).try_into().unwrap();
            
            if topic1 == Symbol::new(&env, "audit") && topic2 == Symbol::new(&env, "entry") {
                has_audit_event = true;
                break;
            }
        }
    }
    assert!(has_audit_event);
}

#[test]
#[should_panic(expected = "audit log timestamp cannot be in the future")]
fn test_timestamp_validation_in_batch() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);
    client.initialize(&admin, &1000_u32);

    // Create a log with a future timestamp
    let actor = Address::generate(&env);
    let operation = Symbol::new(&env, "future_op");
    let status = Symbol::new(&env, "pending");
    
    let future_log = AuditLog {
        actor: actor.clone(),
        operation,
        timestamp: 2_000_000_000, // Future timestamp
        status,
        metadata: None,
        metadata_len: 0,
    };

    let mut logs: Vec<AuditLog> = Vec::new(&env);
    logs.push_back(future_log);

    // This should panic because the timestamp is in the future
    client.batch_log_audit(&admin, &logs);
}

// ─── Audit Log Integrity Verification Tests ──────────────────────────────────

#[test]
fn test_verify_audit_log_integrity_valid() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);
    client.initialize(&admin, &1000_u32);

    let actor = Address::generate(&env);
    let operation = Symbol::new(&env, "transfer");
    let status = Symbol::new(&env, "success");
    let metadata = None;

    // Log an audit entry
    client.log_audit(&actor, &operation, &status, metadata);

    // Verify the integrity of the logged entry
    assert!(client.verify_audit_log_integrity(&1));
}

#[test]
fn test_verify_audit_log_integrity_with_metadata() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);
    client.initialize(&admin, &1000_u32);

    let actor = Address::generate(&env);
    let operation = Symbol::new(&env, "config_update");
    let status = Symbol::new(&env, "success");
    let mut metadata_bytes = soroban_sdk::Bytes::new(&env);
    metadata_bytes.extend_from_slice(&[1u8, 2u8, 3u8, 4u8, 5u8]);
    let metadata = Some(metadata_bytes);

    // Log an audit entry with metadata
    client.log_audit(&actor, &operation, &status, metadata.clone());

    // Verify the integrity of the logged entry
    assert!(client.verify_audit_log_integrity(&1));

    // Verify the metadata length is correct
    let log = client.get_audit_log(&1).unwrap();
    assert_eq!(log.metadata_len, 5);
    assert_eq!(log.metadata.unwrap().len(), 5);
}

#[test]
fn test_verify_audit_log_integrity_nonexistent() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);
    client.initialize(&admin, &1000_u32);

    // Try to verify a non-existent log
    assert!(!client.verify_audit_log_integrity(&999));
}

#[test]
fn test_verify_audit_logs_range_integrity() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);
    client.initialize(&admin, &1000_u32);

    let actor = Address::generate(&env);
    let operation = Symbol::new(&env, "operation");
    let status = Symbol::new(&env, "success");

    // Create multiple audit logs
    for i in 0..5 {
        let mut metadata_bytes = soroban_sdk::Bytes::new(&env);
        metadata_bytes.extend_from_slice(&[i as u8]);
        client.log_audit(
            &actor,
            &operation,
            &status,
            Some(metadata_bytes),
        );
    }

    // Verify all logs in range 1-5 are valid
    let (valid_count, invalid_count) = client.verify_audit_logs_range(&1, &5);
    assert_eq!(valid_count, 5);
    assert_eq!(invalid_count, 0);
}

#[test]
fn test_verify_audit_immutability() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);
    client.initialize(&admin, &1000_u32);

    let actor = Address::generate(&env);
    let operation = Symbol::new(&env, "transfer");
    let status = Symbol::new(&env, "success");

    // Log an audit entry
    client.log_audit(&actor, &operation, &status, None);

    // Verify immutability with correct data
    assert!(client.verify_audit_immutability(&1, &actor, &operation));

    // Verify immutability fails with different actor
    let different_actor = Address::generate(&env);
    assert!(!client.verify_audit_immutability(&1, &different_actor, &operation));

    // Verify immutability fails with different operation
    let different_operation = Symbol::new(&env, "different");
    assert!(!client.verify_audit_immutability(&1, &actor, &different_operation));
}

#[test]
fn test_verify_audit_immutability_consistency_check() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);
    client.initialize(&admin, &1000_u32);

    // Create multiple logs
    let actor1 = Address::generate(&env);
    let actor2 = Address::generate(&env);
    let op1 = Symbol::new(&env, "transfer");
    let op2 = Symbol::new(&env, "withdrawal");
    let status = Symbol::new(&env, "success");

    client.log_audit(&actor1, &op1, &status, None);
    client.log_audit(&actor2, &op2, &status, None);
    client.log_audit(&actor1, &op2, &status, None);

    // Verify each log's immutability
    assert!(client.verify_audit_immutability(&1, &actor1, &op1));
    assert!(client.verify_audit_immutability(&2, &actor2, &op2));
    assert!(client.verify_audit_immutability(&3, &actor1, &op2));

    // Verify cross-contamination doesn't occur
    assert!(!client.verify_audit_immutability(&1, &actor2, &op1));
    assert!(!client.verify_audit_immutability(&2, &actor1, &op2));
}

// ─── Unauthorized Audit Operation Tests ──────────────────────────────────────

#[test]
#[should_panic(expected = "unauthorized: only admin can call this function")]
fn test_unauthorized_batch_log_audit() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);
    client.initialize(&admin, &1000_u32);

    let unauthorized_user = Address::generate(&env);
    let actor = Address::generate(&env);
    let operation = Symbol::new(&env, "transfer");
    let status = Symbol::new(&env, "success");

    // Create a log
    let audit_log = AuditLog {
        actor: actor.clone(),
        operation,
        timestamp: 1_700_000_000,
        status,
        metadata: None,
        metadata_len: 0,
    };

    let mut logs: Vec<AuditLog> = Vec::new(&env);
    logs.push_back(audit_log);

    // This should panic because unauthorized_user is not admin
    client.batch_log_audit(&unauthorized_user, &logs);
}

#[test]
#[should_panic(expected = "unauthorized: only admin can call this function")]
fn test_unauthorized_set_max_metadata_size() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);
    client.initialize(&admin, &1000_u32);

    let unauthorized_user = Address::generate(&env);

    // This should panic because unauthorized_user is not admin
    client.set_max_metadata_size(&unauthorized_user, &2000_u32);
}

#[test]
fn test_audit_log_cannot_be_overwritten() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);
    client.initialize(&admin, &1000_u32);

    let actor = Address::generate(&env);
    let operation1 = Symbol::new(&env, "transfer");
    let operation2 = Symbol::new(&env, "withdrawal");
    let status = Symbol::new(&env, "success");

    // Log initial entry
    client.log_audit(&actor, &operation1, &status, None);

    // Verify the first entry
    let log1 = client.get_audit_log(&1).unwrap();
    assert_eq!(log1.operation, operation1);
    assert!(client.verify_audit_immutability(&1, &actor, &operation1));

    // Log a second entry with different operation
    client.log_audit(&actor, &operation2, &status, None);

    // Verify first entry still has original operation (not overwritten)
    let log1_after = client.get_audit_log(&1).unwrap();
    assert_eq!(log1_after.operation, operation1);
    assert!(client.verify_audit_immutability(&1, &actor, &operation1));

    // Verify second entry has different operation
    let log2 = client.get_audit_log(&2).unwrap();
    assert_eq!(log2.operation, operation2);
    assert!(client.verify_audit_immutability(&2, &actor, &operation2));
}

#[test]
fn test_audit_log_integrity_batch_consistency() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);
    client.initialize(&admin, &1000_u32);

    let actor1 = Address::generate(&env);
    let actor2 = Address::generate(&env);
    let operation = Symbol::new(&env, "transfer");
    let status = Symbol::new(&env, "success");

    // Create a batch of logs
    let mut logs: Vec<AuditLog> = Vec::new(&env);
    
    for i in 1..=3 {
        let mut metadata_bytes = soroban_sdk::Bytes::new(&env);
        metadata_bytes.extend_from_slice(&[i as u8; 5]);
        
        let audit_log = AuditLog {
            actor: if i % 2 == 0 { actor2.clone() } else { actor1.clone() },
            operation: operation.clone(),
            timestamp: 1_700_000_000 + i as u64,
            status: status.clone(),
            metadata: Some(metadata_bytes),
            metadata_len: 5,
        };
        logs.push_back(audit_log);
    }

    // Log the batch
    client.batch_log_audit(&admin, &logs);

    // Verify all logs in batch are intact
    let (valid_count, invalid_count) = client.verify_audit_logs_range(&1, &3);
    assert_eq!(valid_count, 3);
    assert_eq!(invalid_count, 0);

    // Verify each log's immutability
    assert!(client.verify_audit_immutability(&1, &actor1, &operation));
    assert!(client.verify_audit_immutability(&2, &actor2, &operation));
    assert!(client.verify_audit_immutability(&3, &actor1, &operation));
}

#[test]
fn test_get_logs_by_user_empty() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);
    client.initialize(&admin, &1000_u32);

    let unknown = Address::generate(&env);
    let logs = client.get_logs_by_user(&unknown);
    assert_eq!(logs.len(), 0);
}

#[test]
fn test_get_logs_by_user_returns_only_own_logs() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);
    client.initialize(&admin, &1000_u32);

    let actor_a = Address::generate(&env);
    let actor_b = Address::generate(&env);
    let op = Symbol::new(&env, "transfer");
    let status = Symbol::new(&env, "success");

    client.log_audit(&actor_a, &op, &status, None);
    client.log_audit(&actor_b, &op, &status, None);
    client.log_audit(&actor_a, &op, &status, None);

    let logs_a = client.get_logs_by_user(&actor_a);
    let logs_b = client.get_logs_by_user(&actor_b);

    assert_eq!(logs_a.len(), 2);
    assert_eq!(logs_b.len(), 1);

    // All returned logs must belong to actor_a
    for log in logs_a.iter() {
        assert_eq!(log.actor, actor_a);
    }
}

#[test]
fn test_get_logs_by_range_empty_result() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);
    client.initialize(&admin, &1000_u32);

    // Log at timestamp 1_700_000_000; query a range entirely in the future
    let actor = Address::generate(&env);
    client.log_audit(&actor, &Symbol::new(&env, "op"), &Symbol::new(&env, "ok"), None);

    let logs = client.get_logs_by_range(&1_800_000_000, &1_900_000_000);
    assert_eq!(logs.len(), 0);
}

#[test]
fn test_get_logs_by_range_inclusive_bounds() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);
    client.initialize(&admin, &1000_u32);

    let actor = Address::generate(&env);
    let op = Symbol::new(&env, "op");
    let status = Symbol::new(&env, "ok");

    // All three logs share timestamp 1_700_000_000 (set in setup_env)
    client.log_audit(&actor, &op, &status, None);
    client.log_audit(&actor, &op, &status, None);
    client.log_audit(&actor, &op, &status, None);

    // Exact match on both bounds
    let logs = client.get_logs_by_range(&1_700_000_000, &1_700_000_000);
    assert_eq!(logs.len(), 3);
}

#[test]
fn test_get_logs_by_range_no_logs() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);
    client.initialize(&admin, &1000_u32);

    // No logs written at all
    let logs = client.get_logs_by_range(&0, &9_999_999_999);
    assert_eq!(logs.len(), 0);
}

#[test]
#[should_panic(expected = "start timestamp cannot be greater than end timestamp")]
fn test_get_logs_by_range_invalid_bounds() {
    let env = setup_env();
    let (client, admin) = deploy_contract(&env);
    client.initialize(&admin, &1000_u32);

    client.get_logs_by_range(&1_700_000_001, &1_700_000_000);
}
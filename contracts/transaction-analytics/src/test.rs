//! Integration tests for the Transaction Analytics Contract.

#![cfg(test)]

use crate::{
    BundledTransaction, BundleResult, Transaction, TransactionAnalyticsContract,
    TransactionAnalyticsContractClient, ValidationResult,
};
use soroban_sdk::{
    testutils::{Address as _, Events},
    Address, Env, Symbol, Vec,
};

/// Creates a test environment with the contract deployed and initialized.
fn setup_test_env() -> (Env, Address, TransactionAnalyticsContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(TransactionAnalyticsContract, ());
    let client = TransactionAnalyticsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    (env, admin, client)
}

/// Helper to create a test transaction.
fn create_transaction(
    env: &Env,
    tx_id: u64,
    amount: i128,
    category: &str,
) -> Transaction {
    Transaction {
        tx_id,
        from: Address::generate(env),
        to: Address::generate(env),
        amount,
        timestamp: env.ledger().sequence() as u64,
        category: Symbol::new(env, category),
    }
}

/// Helper to create a transaction with specific addresses.
fn create_transaction_with_addresses(
    env: &Env,
    tx_id: u64,
    from: Address,
    to: Address,
    amount: i128,
    category: &str,
) -> Transaction {
    Transaction {
        tx_id,
        from,
        to,
        amount,
        timestamp: env.ledger().sequence() as u64,
        category: Symbol::new(env, category),
    }
}

// ============================================================================
// Initialization Tests
// ============================================================================

#[test]
fn test_initialize_contract() {
    let (_env, admin, client) = setup_test_env();

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_last_batch_id(), 0);
    assert_eq!(client.get_total_transactions_processed(), 0);
}

#[test]
#[should_panic(expected = "Contract already initialized")]
fn test_cannot_initialize_twice() {
    let (env, _admin, client) = setup_test_env();

    let new_admin = Address::generate(&env);
    client.initialize(&new_admin);
}

// ============================================================================
// Batch Processing Tests
// ============================================================================

#[test]
fn test_process_single_transaction_batch() {
    let (env, admin, client) = setup_test_env();

    let mut transactions: Vec<Transaction> = Vec::new(&env);
    transactions.push_back(create_transaction(&env, 1, 1000, "transfer"));

    let metrics = client.process_batch(&admin, &transactions, &None);

    assert_eq!(metrics.tx_count, 1);
    assert_eq!(metrics.total_volume, 1000);
    assert_eq!(metrics.avg_amount, 1000);
    assert_eq!(metrics.min_amount, 1000);
    assert_eq!(metrics.max_amount, 1000);
    assert_eq!(metrics.unique_senders, 1);
    assert_eq!(metrics.unique_recipients, 1);
    // 0.1% of 1000 = 1
    assert_eq!(metrics.total_fees, 1);
}

#[test]
fn test_process_multiple_transactions_batch() {
    let (env, admin, client) = setup_test_env();

    let mut transactions: Vec<Transaction> = Vec::new(&env);
    transactions.push_back(create_transaction(&env, 1, 100, "transfer"));
    transactions.push_back(create_transaction(&env, 2, 200, "budget"));
    transactions.push_back(create_transaction(&env, 3, 300, "savings"));
    transactions.push_back(create_transaction(&env, 4, 400, "transfer"));

    let metrics = client.process_batch(&admin, &transactions, &None);

    assert_eq!(metrics.tx_count, 4);
    assert_eq!(metrics.total_volume, 1000);
    assert_eq!(metrics.avg_amount, 250);
    assert_eq!(metrics.min_amount, 100);
    assert_eq!(metrics.max_amount, 400);
    assert_eq!(metrics.unique_senders, 4);
    assert_eq!(metrics.unique_recipients, 4);
    
    // Fees: 100 + 200 + 300 + 400 = 1000. 
    // Fee = 1000 / 1000 = 1.
    assert_eq!(metrics.total_fees, 1);
}

#[test]
fn test_process_batch_with_shared_addresses() {
    let (env, admin, client) = setup_test_env();

    let sender1 = Address::generate(&env);
    let sender2 = Address::generate(&env);
    let recipient = Address::generate(&env);

    let mut transactions: Vec<Transaction> = Vec::new(&env);
    transactions.push_back(create_transaction_with_addresses(
        &env, 1, sender1.clone(), recipient.clone(), 100, "transfer",
    ));
    transactions.push_back(create_transaction_with_addresses(
        &env, 2, sender1.clone(), recipient.clone(), 200, "transfer",
    ));
    transactions.push_back(create_transaction_with_addresses(
        &env, 3, sender2.clone(), recipient.clone(), 300, "transfer",
    ));

    let metrics = client.process_batch(&admin, &transactions, &None);

    assert_eq!(metrics.tx_count, 3);
    assert_eq!(metrics.unique_senders, 2);
    assert_eq!(metrics.unique_recipients, 1);
}

#[test]
fn test_batch_id_increments() {
    let (env, admin, client) = setup_test_env();

    let mut transactions: Vec<Transaction> = Vec::new(&env);
    transactions.push_back(create_transaction(&env, 1, 100, "transfer"));

    assert_eq!(client.get_last_batch_id(), 0);

    client.process_batch(&admin, &transactions, &None);
    assert_eq!(client.get_last_batch_id(), 1);

    client.process_batch(&admin, &transactions, &None);
    assert_eq!(client.get_last_batch_id(), 2);

    client.process_batch(&admin, &transactions, &None);
    assert_eq!(client.get_last_batch_id(), 3);
}

#[test]
fn test_total_transactions_accumulates() {
    let (env, admin, client) = setup_test_env();

    let mut batch1: Vec<Transaction> = Vec::new(&env);
    batch1.push_back(create_transaction(&env, 1, 100, "transfer"));
    batch1.push_back(create_transaction(&env, 2, 200, "transfer"));

    let mut batch2: Vec<Transaction> = Vec::new(&env);
    batch2.push_back(create_transaction(&env, 3, 300, "budget"));
    batch2.push_back(create_transaction(&env, 4, 400, "budget"));
    batch2.push_back(create_transaction(&env, 5, 500, "budget"));

    client.process_batch(&admin, &batch1, &None);
    assert_eq!(client.get_total_transactions_processed(), 2);

    client.process_batch(&admin, &batch2, &None);
    assert_eq!(client.get_total_transactions_processed(), 5);
}

// ============================================================================
// High Value Alert Tests
// ============================================================================

#[test]
fn test_high_value_threshold_triggers_alerts() {
    let (env, admin, client) = setup_test_env();

    let mut transactions: Vec<Transaction> = Vec::new(&env);
    transactions.push_back(create_transaction(&env, 1, 100, "transfer"));
    transactions.push_back(create_transaction(&env, 2, 5000, "transfer"));
    transactions.push_back(create_transaction(&env, 3, 10000, "budget"));

    let threshold = Some(1000i128);
    let metrics = client.process_batch(&admin, &transactions, &threshold);

    // Verify basic metrics still computed correctly
    assert_eq!(metrics.tx_count, 3);
    assert_eq!(metrics.total_volume, 15100);
}

#[test]
fn test_no_alerts_when_below_threshold() {
    let (env, admin, client) = setup_test_env();

    let mut transactions: Vec<Transaction> = Vec::new(&env);
    transactions.push_back(create_transaction(&env, 1, 100, "transfer"));
    transactions.push_back(create_transaction(&env, 2, 200, "transfer"));

    let threshold = Some(1000i128);
    let metrics = client.process_batch(&admin, &transactions, &threshold);

    assert_eq!(metrics.tx_count, 2);
}

// ============================================================================
// Metrics Retrieval Tests
// ============================================================================

#[test]
fn test_get_batch_metrics_after_processing() {
    let (env, admin, client) = setup_test_env();

    let mut transactions: Vec<Transaction> = Vec::new(&env);
    transactions.push_back(create_transaction(&env, 1, 500, "transfer"));
    transactions.push_back(create_transaction(&env, 2, 500, "transfer"));

    let processed_metrics = client.process_batch(&admin, &transactions, &None);
    let stored_metrics = client.get_batch_metrics(&1).unwrap();

    assert_eq!(stored_metrics.tx_count, processed_metrics.tx_count);
    assert_eq!(stored_metrics.total_volume, processed_metrics.total_volume);
}

#[test]
fn test_get_nonexistent_batch_metrics() {
    let (_, _, client) = setup_test_env();

    let metrics = client.get_batch_metrics(&999);
    assert!(metrics.is_none());
}

// ============================================================================
// Simulate Batch Tests
// ============================================================================

#[test]
fn test_simulate_batch_does_not_store() {
    let (env, _admin, client) = setup_test_env();

    let mut transactions: Vec<Transaction> = Vec::new(&env);
    transactions.push_back(create_transaction(&env, 1, 1000, "transfer"));

    // Simulate should not increment batch ID or total transactions
    let metrics = client.simulate_batch(&transactions);

    assert_eq!(metrics.tx_count, 1);
    assert_eq!(metrics.total_volume, 1000);
    assert_eq!(client.get_last_batch_id(), 0);
    assert_eq!(client.get_total_transactions_processed(), 0);
}

// ============================================================================
// Admin Tests
// ============================================================================

#[test]
fn test_set_admin() {
    let (env, admin, client) = setup_test_env();

    let new_admin = Address::generate(&env);
    client.set_admin(&admin, &new_admin);

    assert_eq!(client.get_admin(), new_admin);
}

#[test]
#[should_panic]
fn test_unauthorized_process_batch() {
    let (env, _, client) = setup_test_env();

    let unauthorized = Address::generate(&env);
    let mut transactions: Vec<Transaction> = Vec::new(&env);
    transactions.push_back(create_transaction(&env, 1, 100, "transfer"));

    // This should panic due to unauthorized access
    client.process_batch(&unauthorized, &transactions, &None);
}

// ============================================================================
// Edge Cases and Error Handling
// ============================================================================

#[test]
#[should_panic]
fn test_empty_batch_rejected() {
    let (env, admin, client) = setup_test_env();

    let transactions: Vec<Transaction> = Vec::new(&env);
    client.process_batch(&admin, &transactions, &None);
}

#[test]
fn test_large_batch_processing() {
    let (env, admin, client) = setup_test_env();

    // Create a batch with 50 transactions
    let mut transactions: Vec<Transaction> = Vec::new(&env);
    for i in 0..50 {
        transactions.push_back(create_transaction(&env, i, (i as i128 + 1) * 100, "transfer"));
    }

    let metrics = client.process_batch(&admin, &transactions, &None);

    assert_eq!(metrics.tx_count, 50);
    // Sum of 100 + 200 + ... + 5000 = 100 * (1 + 2 + ... + 50) = 100 * 1275 = 127500
    assert_eq!(metrics.total_volume, 127500);
    assert_eq!(metrics.avg_amount, 2550);
    assert_eq!(metrics.min_amount, 100);
    assert_eq!(metrics.max_amount, 5000);
}

#[test]
fn test_zero_amount_transactions() {
    let (env, admin, client) = setup_test_env();

    let mut transactions: Vec<Transaction> = Vec::new(&env);
    transactions.push_back(create_transaction(&env, 1, 0, "transfer"));
    transactions.push_back(create_transaction(&env, 2, 100, "transfer"));

    let metrics = client.process_batch(&admin, &transactions, &None);

    assert_eq!(metrics.tx_count, 2);
    assert_eq!(metrics.total_volume, 100);
    assert_eq!(metrics.min_amount, 0);
    assert_eq!(metrics.avg_amount, 50);
    assert_eq!(metrics.total_fees, 0);
}

#[test]
fn test_fee_calculation() {
    let (env, admin, client) = setup_test_env();

    let mut transactions: Vec<Transaction> = Vec::new(&env);
    // 10000 -> 10 fee
    transactions.push_back(create_transaction(&env, 1, 10000, "transfer"));
    // 5500 -> 5 fee
    transactions.push_back(create_transaction(&env, 2, 5500, "savings"));
    // 999 -> 0 fee (integer rounds down)
    transactions.push_back(create_transaction(&env, 3, 999, "budget"));

    let metrics = client.process_batch(&admin, &transactions, &None);

    assert_eq!(metrics.tx_count, 3);
    assert_eq!(metrics.total_volume, 16499);
    // Calculated on total volume: 16499 / 1000 = 16
    assert_eq!(metrics.total_fees, 16);
}

// ============================================================================
// Event Emission Tests
// ============================================================================

#[test]
fn test_events_emitted_on_process() {
    let (env, admin, client) = setup_test_env();

    let mut transactions: Vec<Transaction> = Vec::new(&env);
    transactions.push_back(create_transaction(&env, 1, 1000, "transfer"));

    client.process_batch(&admin, &transactions, &None);

    let events = env.events().all();

    // Should have multiple events: analytics_started, batch_processed,
    // category_analytics, analytics_completed
    assert!(events.len() >= 4);
}

// ============================================================================
// Category Metrics Tests
// ============================================================================

#[test]
fn test_multiple_categories_processed() {
    let (env, admin, client) = setup_test_env();

    let mut transactions: Vec<Transaction> = Vec::new(&env);
    transactions.push_back(create_transaction(&env, 1, 500, "transfer"));
    transactions.push_back(create_transaction(&env, 2, 300, "budget"));
    transactions.push_back(create_transaction(&env, 3, 200, "savings"));

    let metrics = client.process_batch(&admin, &transactions, &None);

    assert_eq!(metrics.tx_count, 3);
    assert_eq!(metrics.total_volume, 1000);
}

#[test]
fn test_same_category_aggregation() {
    let (env, admin, client) = setup_test_env();

    let mut transactions: Vec<Transaction> = Vec::new(&env);
    transactions.push_back(create_transaction(&env, 1, 100, "transfer"));
    transactions.push_back(create_transaction(&env, 2, 200, "transfer"));
    transactions.push_back(create_transaction(&env, 3, 300, "transfer"));

    let metrics = client.process_batch(&admin, &transactions, &None);

    assert_eq!(metrics.tx_count, 3);
    assert_eq!(metrics.total_volume, 600);
}

// ============================================================================
// Audit Log Tests
// ============================================================================

#[test]
fn test_batch_audit_log_success() {
    let (env, admin, client) = setup_test_env();

    let actor = Address::generate(&env);
    let mut logs: Vec<crate::AuditLog> = Vec::new(&env);
    logs.push_back(crate::AuditLog {
        actor: actor.clone(),
        operation: Symbol::new(&env, "login"),
        timestamp: 1000,
        status: Symbol::new(&env, "success"),
    });
    logs.push_back(crate::AuditLog {
        actor: actor.clone(),
        operation: Symbol::new(&env, "update_profile"),
        timestamp: 1005,
        status: Symbol::new(&env, "success"),
    });

    client.batch_audit_log(&admin, &logs);

    assert_eq!(client.get_total_audit_logs(), 2);
    
    let log1 = client.get_audit_log(&1).unwrap();
    assert_eq!(log1.actor, actor);
    assert_eq!(log1.operation, Symbol::new(&env, "login"));
    
    let log2 = client.get_audit_log(&2).unwrap();
    assert_eq!(log2.actor, actor);
    assert_eq!(log2.operation, Symbol::new(&env, "update_profile"));
// Transaction Bundling Tests
// ============================================================================

/// Helper to create a bundled transaction.
fn create_bundled_transaction(
    env: &Env,
    tx_id: u64,
    amount: i128,
    category: &str,
) -> BundledTransaction {
    BundledTransaction {
        transaction: create_transaction(env, tx_id, amount, category),
        memo: None,
    }
}

/// Helper to create a bundled transaction with memo.
fn create_bundled_transaction_with_memo(
    env: &Env,
    tx_id: u64,
    amount: i128,
    category: &str,
    memo: &str,
) -> BundledTransaction {
    BundledTransaction {
        transaction: create_transaction(env, tx_id, amount, category),
        memo: Some(Symbol::new(env, memo)),
    }
}

/// Helper to create a bundled transaction with specific addresses.
fn create_bundled_transaction_with_addresses(
    env: &Env,
    tx_id: u64,
    from: Address,
    to: Address,
    amount: i128,
    category: &str,
) -> BundledTransaction {
    BundledTransaction {
        transaction: create_transaction_with_addresses(env, tx_id, from, to, amount, category),
        memo: None,
    }
}

#[test]
fn test_bundle_transactions_success() {
    let (env, admin, client) = setup_test_env();

    let mut bundled_txs: Vec<BundledTransaction> = Vec::new(&env);
    bundled_txs.push_back(create_bundled_transaction(&env, 1, 1000, "transfer"));
    bundled_txs.push_back(create_bundled_transaction(&env, 2, 2000, "budget"));
    bundled_txs.push_back(create_bundled_transaction(&env, 3, 3000, "savings"));

    let result = client.bundle_transactions(&admin, &bundled_txs);

    assert_eq!(result.bundle_id, 1);
    assert_eq!(result.total_count, 3);
    assert_eq!(result.valid_count, 3);
    assert_eq!(result.invalid_count, 0);
    assert_eq!(result.can_bundle, true);
    assert_eq!(result.total_volume, 6000);
    assert_eq!(result.validation_results.len(), 3);

    // All transactions should be valid
    for result_item in result.validation_results.iter() {
        assert_eq!(result_item.is_valid, true);
    }
}

#[test]
fn test_bundle_transactions_with_partial_failures() {
    let (env, admin, client) = setup_test_env();

    let mut bundled_txs: Vec<BundledTransaction> = Vec::new(&env);
    bundled_txs.push_back(create_bundled_transaction(&env, 1, 1000, "transfer"));
    // Create a transaction with same from/to address (should fail validation)
    let sender = Address::generate(&env);
    bundled_txs.push_back(create_bundled_transaction_with_addresses(
        &env, 2, sender.clone(), sender.clone(), 2000, "budget",
    ));
    bundled_txs.push_back(create_bundled_transaction(&env, 3, 3000, "savings"));

    let result = client.bundle_transactions(&admin, &bundled_txs);

    assert_eq!(result.bundle_id, 1);
    assert_eq!(result.total_count, 3);
    assert_eq!(result.valid_count, 2);
    assert_eq!(result.invalid_count, 1);
    assert_eq!(result.can_bundle, false); // Not all valid
    assert_eq!(result.total_volume, 4000); // Only valid transactions
    assert_eq!(result.validation_results.len(), 3);

    // Check validation results
    let result_1 = result.validation_results.get(0).unwrap();
    assert_eq!(result_1.tx_id, 1);
    assert_eq!(result_1.is_valid, true);

    let result_2 = result.validation_results.get(1).unwrap();
    assert_eq!(result_2.tx_id, 2);
    assert_eq!(result_2.is_valid, false);

    let result_3 = result.validation_results.get(2).unwrap();
    assert_eq!(result_3.tx_id, 3);
    assert_eq!(result_3.is_valid, true);
}

#[test]
fn test_bundle_transactions_with_negative_amount() {
    let (env, admin, client) = setup_test_env();

    let mut bundled_txs: Vec<BundledTransaction> = Vec::new(&env);
    bundled_txs.push_back(create_bundled_transaction(&env, 1, 1000, "transfer"));
    // Create a transaction with negative amount (should fail validation)
    let mut invalid_tx = create_bundled_transaction(&env, 2, -100, "budget");
    bundled_txs.push_back(invalid_tx);
    bundled_txs.push_back(create_bundled_transaction(&env, 3, 3000, "savings"));

    let result = client.bundle_transactions(&admin, &bundled_txs);

    assert_eq!(result.valid_count, 2);
    assert_eq!(result.invalid_count, 1);
    assert_eq!(result.can_bundle, false);

    // Check that the invalid transaction has the correct error
    let invalid_result = result.validation_results.get(1).unwrap();
    assert_eq!(invalid_result.tx_id, 2);
    assert_eq!(invalid_result.is_valid, false);
}

#[test]
fn test_bundle_id_increments() {
    let (env, admin, client) = setup_test_env();

    let mut bundled_txs: Vec<BundledTransaction> = Vec::new(&env);
    bundled_txs.push_back(create_bundled_transaction(&env, 1, 1000, "transfer"));

    assert_eq!(client.get_last_bundle_id(), 0);

    client.bundle_transactions(&admin, &bundled_txs);
    assert_eq!(client.get_last_bundle_id(), 1);

    client.bundle_transactions(&admin, &bundled_txs);
    assert_eq!(client.get_last_bundle_id(), 2);
}

#[test]
fn test_get_bundle_result() {
    let (env, admin, client) = setup_test_env();

    let mut bundled_txs: Vec<BundledTransaction> = Vec::new(&env);
    bundled_txs.push_back(create_bundled_transaction(&env, 1, 1000, "transfer"));
    bundled_txs.push_back(create_bundled_transaction(&env, 2, 2000, "budget"));

    let created_result = client.bundle_transactions(&admin, &bundled_txs);
    let retrieved_result = client.get_bundle_result(&1).unwrap();

    assert_eq!(retrieved_result.bundle_id, created_result.bundle_id);
    assert_eq!(retrieved_result.total_count, created_result.total_count);
    assert_eq!(retrieved_result.valid_count, created_result.valid_count);
    assert_eq!(retrieved_result.can_bundle, created_result.can_bundle);
}

#[test]
fn test_get_nonexistent_bundle_result() {
    let (_, _, client) = setup_test_env();

    let result = client.get_bundle_result(&999);
    assert!(result.is_none());
}

#[test]
#[should_panic]
fn test_batch_audit_log_unauthorized() {
    let (env, _, client) = setup_test_env();

    let unauthorized = Address::generate(&env);
    let logs: Vec<crate::AuditLog> = Vec::new(&env);
    // This should panic due to unauthorized access
    client.batch_audit_log(&unauthorized, &logs);
}

#[test]
#[should_panic]
fn test_batch_audit_log_empty_rejected() {
    let (env, admin, client) = setup_test_env();

    let logs: Vec<crate::AuditLog> = Vec::new(&env);
    client.batch_audit_log(&admin, &logs);
}

#[test]
#[should_panic]
fn test_batch_audit_log_invalid_timestamp() {
    let (env, admin, client) = setup_test_env();

    let mut logs: Vec<crate::AuditLog> = Vec::new(&env);
    logs.push_back(crate::AuditLog {
        actor: Address::generate(&env),
        operation: Symbol::new(&env, "op"),
        timestamp: 0, // Invalid
        status: Symbol::new(&env, "status"),
    });

    client.batch_audit_log(&admin, &logs);
}

#[test]
fn test_audit_log_events_emitted() {
    let (env, admin, client) = setup_test_env();

    let mut logs: Vec<crate::AuditLog> = Vec::new(&env);
    logs.push_back(crate::AuditLog {
        actor: Address::generate(&env),
        operation: Symbol::new(&env, "op"),
        timestamp: 100,
        status: Symbol::new(&env, "ok"),
    });

    client.batch_audit_log(&admin, &logs);

    let events = env.events().all();
    // At least one audit log event should be emitted
    assert!(events.len() >= 1);
=======
fn test_bundle_empty_transactions() {
    let (env, admin, client) = setup_test_env();

    let bundled_txs: Vec<BundledTransaction> = Vec::new(&env);
    client.bundle_transactions(&admin, &bundled_txs);
}

#[test]
#[should_panic]
fn test_unauthorized_bundle_transactions() {
    let (env, _, client) = setup_test_env();

    let unauthorized = Address::generate(&env);
    let mut bundled_txs: Vec<BundledTransaction> = Vec::new(&env);
    bundled_txs.push_back(create_bundled_transaction(&env, 1, 1000, "transfer"));

    // This should panic due to unauthorized access
    client.bundle_transactions(&unauthorized, &bundled_txs);
}

#[test]
fn test_bundle_events_emitted() {
    let (env, admin, client) = setup_test_env();

    let mut bundled_txs: Vec<BundledTransaction> = Vec::new(&env);
    bundled_txs.push_back(create_bundled_transaction(&env, 1, 1000, "transfer"));
    bundled_txs.push_back(create_bundled_transaction(&env, 2, 2000, "budget"));

    client.bundle_transactions(&admin, &bundled_txs);

    let events = env.events().all();

    // Should have multiple events: bundling_started, transaction_validated (x2),
    // bundle_created, bundling_completed
    assert!(events.len() >= 5);
}

#[test]
fn test_bundle_large_number_of_transactions() {
    let (env, admin, client) = setup_test_env();

    // Create a bundle with 50 transactions
    let mut bundled_txs: Vec<BundledTransaction> = Vec::new(&env);
    for i in 0..50 {
        bundled_txs.push_back(create_bundled_transaction(
            &env,
            i,
            (i as i128 + 1) * 100,
            "transfer",
        ));
    }

    let result = client.bundle_transactions(&admin, &bundled_txs);

    assert_eq!(result.total_count, 50);
    assert_eq!(result.valid_count, 50);
    assert_eq!(result.invalid_count, 0);
    assert_eq!(result.can_bundle, true);
    // Sum of 100 + 200 + ... + 5000 = 100 * (1 + 2 + ... + 50) = 100 * 1275 = 127500
    assert_eq!(result.total_volume, 127500);
}

#[test]
fn test_bundle_with_memo() {
    let (env, admin, client) = setup_test_env();

    let mut bundled_txs: Vec<BundledTransaction> = Vec::new(&env);
    bundled_txs.push_back(create_bundled_transaction_with_memo(
        &env, 1, 1000, "transfer", "payment",
    ));
    bundled_txs.push_back(create_bundled_transaction(&env, 2, 2000, "budget"));

    let result = client.bundle_transactions(&admin, &bundled_txs);

    assert_eq!(result.valid_count, 2);
    assert_eq!(result.can_bundle, true);
}

#[test]
fn test_bundle_all_transactions_invalid() {
    let (env, admin, client) = setup_test_env();

    // Create transactions that will all fail validation
    let mut bundled_txs: Vec<BundledTransaction> = Vec::new(&env);
    let sender = Address::generate(&env);
    bundled_txs.push_back(create_bundled_transaction_with_addresses(
        &env, 1, sender.clone(), sender.clone(), 1000, "transfer",
    ));
    bundled_txs.push_back(create_bundled_transaction_with_addresses(
        &env, 2, sender.clone(), sender.clone(), 2000, "budget",
    ));

    let result = client.bundle_transactions(&admin, &bundled_txs);

    assert_eq!(result.valid_count, 0);
    assert_eq!(result.invalid_count, 2);
    assert_eq!(result.can_bundle, false);
    assert_eq!(result.total_volume, 0);
}

#[test]
fn test_bundle_zero_amount_transactions() {
    let (env, admin, client) = setup_test_env();

    let mut bundled_txs: Vec<BundledTransaction> = Vec::new(&env);
    bundled_txs.push_back(create_bundled_transaction(&env, 1, 0, "transfer"));
    bundled_txs.push_back(create_bundled_transaction(&env, 2, 1000, "budget"));

    let result = client.bundle_transactions(&admin, &bundled_txs);

    // Zero amount transactions are allowed
    assert_eq!(result.valid_count, 2);
    assert_eq!(result.can_bundle, true);
    assert_eq!(result.total_volume, 1000);
}

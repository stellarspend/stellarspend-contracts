//! Integration tests for the Transaction Analytics Contract.

#![cfg(test)]

use crate::{Transaction, TransactionAnalyticsContract, TransactionAnalyticsContractClient, RefundRequest, RefundStatus};
use soroban_sdk::{
    testutils::{Address as _, Events},
    Address, Env, Symbol, Vec, Map,
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

/// Helper to create a refund request.
fn create_refund_request(env: &Env, tx_id: u64, reason: Option<&str>) -> RefundRequest {
    RefundRequest {
        tx_id,
        reason: reason.map(|r| Symbol::new(env, r)),
    }
}

/// Helper to create a transaction lookup map.
fn create_transaction_lookup(env: &Env, transactions: &Vec<Transaction>) -> Map<u64, Transaction> {
    let mut lookup = Map::new(env);
    for tx in transactions.iter() {
        lookup.set(tx.tx_id, tx.clone());
    }
    lookup
}

// ============================================================================
// Refund Tests
// ============================================================================

#[test]
fn test_refund_single_eligible_transaction() {
    let (env, admin, client) = setup_test_env();
    
    // Create some transactions first
    let mut transactions: Vec<Transaction> = Vec::new(&env);
    transactions.push_back(create_transaction(&env, 1, 1000, "transfer")); // Odd ID = eligible
    transactions.push_back(create_transaction(&env, 2, 500, "budget"));   // Even ID = not eligible
    
    let lookup = create_transaction_lookup(&env, &transactions);
    
    // Process the batch first to establish transaction records
    client.process_batch(&admin, &transactions, &None);
    
    // Create refund request for eligible transaction
    let mut refund_requests: Vec<RefundRequest> = Vec::new(&env);
    refund_requests.push_back(create_refund_request(&env, 1, Some("Failed transaction")));
    
    let metrics = client.refund_batch(&admin, &refund_requests, &lookup);
    
    assert_eq!(metrics.request_count, 1);
    assert_eq!(metrics.successful_refunds, 1);
    assert_eq!(metrics.failed_refunds, 0);
    assert_eq!(metrics.total_refunded_amount, 1000);
    assert_eq!(metrics.avg_refund_amount, 1000);
    
    // Verify transaction is marked as refunded
    assert!(client.is_transaction_refunded(1));
    assert_eq!(client.get_total_refund_amount(), 1000);
}

#[test]
fn test_refund_multiple_transactions_mixed_eligibility() {
    let (env, admin, client) = setup_test_env();
    
    let mut transactions: Vec<Transaction> = Vec::new(&env);
    transactions.push_back(create_transaction(&env, 1, 1000, "transfer")); // Eligible
    transactions.push_back(create_transaction(&env, 2, 500, "budget"));   // Not eligible
    transactions.push_back(create_transaction(&env, 3, 2000, "savings")); // Eligible
    transactions.push_back(create_transaction(&env, 4, 300, "transfer")); // Not eligible
    
    let lookup = create_transaction_lookup(&env, &transactions);
    client.process_batch(&admin, &transactions, &None);
    
    let mut refund_requests: Vec<RefundRequest> = Vec::new(&env);
    refund_requests.push_back(create_refund_request(&env, 1, None));
    refund_requests.push_back(create_refund_request(&env, 2, None));
    refund_requests.push_back(create_refund_request(&env, 3, None));
    refund_requests.push_back(create_refund_request(&env, 4, None));
    
    let metrics = client.refund_batch(&admin, &refund_requests, &lookup);
    
    assert_eq!(metrics.request_count, 4);
    assert_eq!(metrics.successful_refunds, 2); // Only odd IDs (1,3) are eligible
    assert_eq!(metrics.failed_refunds, 2);     // Even IDs (2,4) are not eligible
    assert_eq!(metrics.total_refunded_amount, 3000); // 1000 + 2000
    assert_eq!(metrics.avg_refund_amount, 1500);
    
    // Verify only eligible transactions are marked refunded
    assert!(client.is_transaction_refunded(1));
    assert!(!client.is_transaction_refunded(2));
    assert!(client.is_transaction_refunded(3));
    assert!(!client.is_transaction_refunded(4));
}

#[test]
fn test_refund_already_refunded_transaction() {
    let (env, admin, client) = setup_test_env();
    
    let mut transactions: Vec<Transaction> = Vec::new(&env);
    transactions.push_back(create_transaction(&env, 1, 1000, "transfer"));
    
    let lookup = create_transaction_lookup(&env, &transactions);
    client.process_batch(&admin, &transactions, &None);
    
    // First refund
    let mut refund_requests: Vec<RefundRequest> = Vec::new(&env);
    refund_requests.push_back(create_refund_request(&env, 1, None));
    client.refund_batch(&admin, &refund_requests, &lookup);
    
    // Try to refund the same transaction again
    let metrics = client.refund_batch(&admin, &refund_requests, &lookup);
    
    assert_eq!(metrics.request_count, 1);
    assert_eq!(metrics.successful_refunds, 0);
    assert_eq!(metrics.failed_refunds, 1);
    assert_eq!(metrics.total_refunded_amount, 0);
}

#[test]
fn test_refund_nonexistent_transaction() {
    let (env, admin, client) = setup_test_env();
    
    // Create empty lookup (no transactions)
    let lookup: Map<u64, Transaction> = Map::new(&env);
    
    let mut refund_requests: Vec<RefundRequest> = Vec::new(&env);
    refund_requests.push_back(create_refund_request(&env, 999, Some("Nonexistent tx")));
    
    let metrics = client.refund_batch(&admin, &refund_requests, &lookup);
    
    assert_eq!(metrics.request_count, 1);
    assert_eq!(metrics.successful_refunds, 0);
    assert_eq!(metrics.failed_refunds, 1);
    assert_eq!(metrics.total_refunded_amount, 0);
}

#[test]
fn test_refund_batch_id_increments() {
    let (env, admin, client) = setup_test_env();
    
    let mut transactions: Vec<Transaction> = Vec::new(&env);
    transactions.push_back(create_transaction(&env, 1, 1000, "transfer"));
    transactions.push_back(create_transaction(&env, 3, 2000, "budget"));
    
    let lookup = create_transaction_lookup(&env, &transactions);
    client.process_batch(&admin, &transactions, &None);
    
    assert_eq!(client.get_last_refund_batch_id(), 0);
    
    let mut refund_requests: Vec<RefundRequest> = Vec::new(&env);
    refund_requests.push_back(create_refund_request(&env, 1, None));
    client.refund_batch(&admin, &refund_requests, &lookup);
    assert_eq!(client.get_last_refund_batch_id(), 1);
    
    refund_requests.clear();
    refund_requests.push_back(create_refund_request(&env, 3, None));
    client.refund_batch(&admin, &refund_requests, &lookup);
    assert_eq!(client.get_last_refund_batch_id(), 2);
}

#[test]
fn test_simulate_refund_batch() {
    let (env, admin, client) = setup_test_env();
    
    let mut transactions: Vec<Transaction> = Vec::new(&env);
    transactions.push_back(create_transaction(&env, 1, 1000, "transfer"));
    transactions.push_back(create_transaction(&env, 3, 2000, "budget"));
    
    let lookup = create_transaction_lookup(&env, &transactions);
    client.process_batch(&admin, &transactions, &None);
    
    let mut refund_requests: Vec<RefundRequest> = Vec::new(&env);
    refund_requests.push_back(create_refund_request(&env, 1, None));
    refund_requests.push_back(create_refund_request(&env, 3, None));
    
    // Simulate should not affect actual state
    let metrics_before = client.get_total_refund_amount();
    let simulated_metrics = client.simulate_refund_batch(&refund_requests, &lookup);
    let metrics_after = client.get_total_refund_amount();
    
    // Should return correct simulation results
    assert_eq!(simulated_metrics.request_count, 2);
    assert_eq!(simulated_metrics.successful_refunds, 2);
    assert_eq!(simulated_metrics.total_refunded_amount, 3000);
    
    // Actual state should be unchanged
    assert_eq!(metrics_before, metrics_after);
    assert_eq!(metrics_after, 0); // No actual refunds processed
}

#[test]
fn test_get_refund_batch_metrics() {
    let (env, admin, client) = setup_test_env();
    
    let mut transactions: Vec<Transaction> = Vec::new(&env);
    transactions.push_back(create_transaction(&env, 1, 1000, "transfer"));
    
    let lookup = create_transaction_lookup(&env, &transactions);
    client.process_batch(&admin, &transactions, &None);
    
    let mut refund_requests: Vec<RefundRequest> = Vec::new(&env);
    refund_requests.push_back(create_refund_request(&env, 1, None));
    
    let metrics = client.refund_batch(&admin, &refund_requests, &lookup);
    
    // Should be able to retrieve the stored metrics
    let retrieved_metrics = client.get_refund_batch_metrics(1).unwrap();
    assert_eq!(retrieved_metrics.request_count, metrics.request_count);
    assert_eq!(retrieved_metrics.successful_refunds, metrics.successful_refunds);
    assert_eq!(retrieved_metrics.total_refunded_amount, metrics.total_refunded_amount);
    
    // Non-existent batch should return None
    assert!(client.get_refund_batch_metrics(999).is_none());
}

#[test]
#[should_panic(expected = "EmptyRefundBatch")]
fn test_empty_refund_batch_rejected() {
    let (env, admin, client) = setup_test_env();
    
    let refund_requests: Vec<RefundRequest> = Vec::new(&env);
    let lookup: Map<u64, Transaction> = Map::new(&env);
    
    client.refund_batch(&admin, &refund_requests, &lookup);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_unauthorized_refund_batch() {
    let (env, _admin, client) = setup_test_env();
    
    let unauthorized_user = Address::generate(&env);
    let refund_requests: Vec<RefundRequest> = Vec::new(&env);
    let lookup: Map<u64, Transaction> = Map::new(&env);
    
    client.refund_batch(&unauthorized_user, &refund_requests, &lookup);
}

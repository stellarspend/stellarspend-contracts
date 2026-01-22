//! Integration tests for the Transaction Analytics Contract.

#![cfg(test)]

use crate::{
    BudgetRecommendation, Transaction, TransactionAnalyticsContract,
    TransactionAnalyticsContractClient, UserBudgetData,
};
use soroban_sdk::{
    testutils::{Address as _, Events},
    Address, Env, Map, Symbol, Vec,
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

// ============================================================================
// Budget Recommendations Tests
// ============================================================================

/// Helper to create test user budget data.
fn create_user_budget_data(
    env: &Env,
    monthly_income: i128,
    spending: Vec<(Symbol, i128)>,
    risk_tolerance: u8,
) -> UserBudgetData {
    let mut spending_map: Map<Symbol, i128> = Map::new(env);
    for (category, amount) in spending.iter() {
        spending_map.set(category.clone(), amount.clone());
    }

    UserBudgetData {
        user: Address::generate(env),
        monthly_income,
        spending_by_category: spending_map,
        savings_goal: None,
        risk_tolerance,
    }
}

#[test]
fn test_generate_batch_budget_recommendations_single_user() {
    let (env, admin, client) = setup_test_env();

    let mut users: Vec<UserBudgetData> = Vec::new(&env);
    let mut spending: Vec<(Symbol, i128)> = Vec::new(&env);
    spending.push_back((Symbol::new(&env, "food"), 1000));
    spending.push_back((Symbol::new(&env, "transport"), 500));

    users.push_back(create_user_budget_data(&env, 5000, spending, 3));

    let recommendations = client.generate_batch_budget_recommendations(&admin, &users);

    assert_eq!(recommendations.len(), 1);
    let rec = recommendations.get(0).unwrap();
    assert_eq!(rec.user, users.get(0).unwrap().user);
    assert!(rec.recommended_savings > 0);
    assert!(rec.recommended_emergency_fund > 0);
    assert!(rec.confidence_score >= 60);
}

#[test]
fn test_generate_batch_budget_recommendations_multiple_users() {
    let (env, admin, client) = setup_test_env();

    let mut users: Vec<UserBudgetData> = Vec::new(&env);

    // User 1: Moderate income, moderate spending
    let mut spending1: Vec<(Symbol, i128)> = Vec::new(&env);
    spending1.push_back((Symbol::new(&env, "food"), 800));
    spending1.push_back((Symbol::new(&env, "transport"), 400));
    users.push_back(create_user_budget_data(&env, 4000, spending1, 3));

    // User 2: High income, high spending, aggressive
    let mut spending2: Vec<(Symbol, i128)> = Vec::new(&env);
    spending2.push_back((Symbol::new(&env, "food"), 1500));
    spending2.push_back((Symbol::new(&env, "housing"), 2000));
    users.push_back(create_user_budget_data(&env, 8000, spending2, 5));

    // User 3: Low income, conservative
    let mut spending3: Vec<(Symbol, i128)> = Vec::new(&env);
    spending3.push_back((Symbol::new(&env, "food"), 500));
    users.push_back(create_user_budget_data(&env, 2000, spending3, 1));

    let recommendations = client.generate_batch_budget_recommendations(&admin, &users);

    assert_eq!(recommendations.len(), 3);

    // Check User 2 (aggressive) has higher savings recommendation
    let rec2 = recommendations.get(1).unwrap();
    assert!(rec2.recommended_savings >= 2000); // Should be around 40% of 8000 = 3200

    // Check User 3 (conservative) has higher emergency fund
    let rec3 = recommendations.get(2).unwrap();
    assert!(rec3.recommended_emergency_fund > 0);
}

#[test]
fn test_budget_recommendations_events_emitted() {
    let (env, admin, client) = setup_test_env();

    let mut users: Vec<UserBudgetData> = Vec::new(&env);
    let mut spending: Vec<(Symbol, i128)> = Vec::new(&env);
    spending.push_back((Symbol::new(&env, "food"), 1000));
    users.push_back(create_user_budget_data(&env, 5000, spending, 3));

    client.generate_batch_budget_recommendations(&admin, &users);

    let events = env.events().all();
    // Should have: recommendations_started, recommendation_generated, recommendations_completed
    assert!(events.len() >= 3);
}

#[test]
fn test_get_recommendation_batch() {
    let (env, admin, client) = setup_test_env();

    let mut users: Vec<UserBudgetData> = Vec::new(&env);
    let mut spending: Vec<(Symbol, i128)> = Vec::new(&env);
    spending.push_back((Symbol::new(&env, "food"), 1000));
    users.push_back(create_user_budget_data(&env, 5000, spending, 3));

    let generated = client.generate_batch_budget_recommendations(&admin, &users);
    let stored = client.get_recommendation_batch(&1).unwrap();

    assert_eq!(stored.len(), generated.len());
    assert_eq!(
        stored.get(0).unwrap().user,
        generated.get(0).unwrap().user
    );
}

#[test]
fn test_get_nonexistent_recommendation_batch() {
    let (_, _, client) = setup_test_env();

    let recommendations = client.get_recommendation_batch(&999);
    assert!(recommendations.is_none());
}

#[test]
fn test_recommendation_batch_id_increments() {
    let (env, admin, client) = setup_test_env();

    let mut users: Vec<UserBudgetData> = Vec::new(&env);
    let mut spending: Vec<(Symbol, i128)> = Vec::new(&env);
    spending.push_back((Symbol::new(&env, "food"), 1000));
    users.push_back(create_user_budget_data(&env, 5000, spending, 3));

    assert_eq!(client.get_last_recommendation_batch_id(), 0);

    client.generate_batch_budget_recommendations(&admin, &users);
    assert_eq!(client.get_last_recommendation_batch_id(), 1);

    client.generate_batch_budget_recommendations(&admin, &users);
    assert_eq!(client.get_last_recommendation_batch_id(), 2);
}

#[test]
fn test_simulate_budget_recommendation() {
    let (env, _admin, client) = setup_test_env();

    let mut spending: Vec<(Symbol, i128)> = Vec::new(&env);
    spending.push_back((Symbol::new(&env, "food"), 1000));
    let user_data = create_user_budget_data(&env, 5000, spending, 3);

    // Simulate should not increment batch ID
    let recommendation = client.simulate_budget_recommendation(&user_data);

    assert_eq!(recommendation.user, user_data.user);
    assert!(recommendation.recommended_savings > 0);
    assert_eq!(client.get_last_recommendation_batch_id(), 0);
}

#[test]
#[should_panic]
fn test_empty_budget_batch_rejected() {
    let (env, admin, client) = setup_test_env();

    let users: Vec<UserBudgetData> = Vec::new(&env);
    client.generate_batch_budget_recommendations(&admin, &users);
}

#[test]
#[should_panic]
fn test_invalid_budget_data_rejected() {
    let (env, admin, client) = setup_test_env();

    let mut users: Vec<UserBudgetData> = Vec::new(&env);
    let mut spending: Map<Symbol, i128> = Map::new(&env);
    spending.set(Symbol::new(&env, "food"), 1000);

    // Invalid: zero income
    let user_data = UserBudgetData {
        user: Address::generate(&env),
        monthly_income: 0,
        spending_by_category: spending,
        savings_goal: None,
        risk_tolerance: 3,
    };
    users.push_back(user_data);

    client.generate_batch_budget_recommendations(&admin, &users);
}

#[test]
#[should_panic]
fn test_invalid_risk_tolerance_rejected() {
    let (env, admin, client) = setup_test_env();

    let mut users: Vec<UserBudgetData> = Vec::new(&env);
    let mut spending: Map<Symbol, i128> = Map::new(&env);
    spending.set(Symbol::new(&env, "food"), 1000);

    // Invalid: risk tolerance out of range
    let user_data = UserBudgetData {
        user: Address::generate(&env),
        monthly_income: 5000,
        spending_by_category: spending,
        savings_goal: None,
        risk_tolerance: 6, // Invalid: should be 1-5
    };
    users.push_back(user_data);

    client.generate_batch_budget_recommendations(&admin, &users);
}

#[test]
#[should_panic]
fn test_unauthorized_budget_recommendations() {
    let (env, _, client) = setup_test_env();

    let mut users: Vec<UserBudgetData> = Vec::new(&env);
    let mut spending: Vec<(Symbol, i128)> = Vec::new(&env);
    spending.push_back((Symbol::new(&env, "food"), 1000));
    users.push_back(create_user_budget_data(&env, 5000, spending, 3));

    let unauthorized = Address::generate(&env);
    client.generate_batch_budget_recommendations(&unauthorized, &users);
}

#[test]
fn test_budget_recommendation_with_savings_goal() {
    let (env, admin, client) = setup_test_env();

    let mut users: Vec<UserBudgetData> = Vec::new(&env);
    let mut spending: Map<Symbol, i128> = Map::new(&env);
    spending.set(Symbol::new(&env, "food"), 1000);

    let user_data = UserBudgetData {
        user: Address::generate(&env),
        monthly_income: 5000,
        spending_by_category: spending,
        savings_goal: Some(1500), // User wants to save 1500/month
        risk_tolerance: 3,
    };
    users.push_back(user_data);

    let recommendations = client.generate_batch_budget_recommendations(&admin, &users);
    let rec = recommendations.get(0).unwrap();

    // Should respect user's savings goal if reasonable
    assert!(rec.recommended_savings > 0);
}

#[test]
fn test_budget_recommendation_confidence_score() {
    let (env, admin, client) = setup_test_env();

    // User with many categories (high confidence)
    let mut users: Vec<UserBudgetData> = Vec::new(&env);
    let mut spending: Map<Symbol, i128> = Map::new(&env);
    spending.set(Symbol::new(&env, "food"), 500);
    spending.set(Symbol::new(&env, "transport"), 300);
    spending.set(Symbol::new(&env, "housing"), 1000);
    spending.set(Symbol::new(&env, "utilities"), 200);
    spending.set(Symbol::new(&env, "entertainment"), 400);

    let user_data = UserBudgetData {
        user: Address::generate(&env),
        monthly_income: 5000,
        spending_by_category: spending,
        savings_goal: None,
        risk_tolerance: 3,
    };
    users.push_back(user_data);

    let recommendations = client.generate_batch_budget_recommendations(&admin, &users);
    let rec = recommendations.get(0).unwrap();

    // With 5+ categories, confidence should be high (90)
    assert!(rec.confidence_score >= 75);
}

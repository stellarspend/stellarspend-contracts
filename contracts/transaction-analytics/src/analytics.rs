//! Core batch analytics computation logic.
//!
//! This module provides optimized batch processing for transaction analytics,
//! following Soroban best practices:
//! - Minimizes storage operations by accumulating changes locally
//! - Uses fixed-size structures where possible
//! - Batches computations to reduce gas costs

use soroban_sdk::{Address, Env, Map, Symbol, Vec};

use crate::types::{AuditLog, BatchMetrics, CategoryMetrics, Transaction, MAX_BATCH_SIZE};
use crate::types::{
    BatchMetrics, BundleResult, BundledTransaction, CategoryMetrics, Transaction,
    ValidationResult, MAX_BATCH_SIZE,
};

/// Calculates the processing fee for a transaction amount.
///
/// Current fee model: 0.1% (10 basis points)
pub fn calculate_fee(amount: i128) -> i128 {
    if amount <= 0 {
        return 0;
    }
    // 0.1% = amount * 10 / 10000 = amount / 1000
    amount / 1000
}

/// Computes aggregated metrics for a batch of transactions.
///
/// Optimized to perform a single pass over the transaction data,
/// computing all metrics in O(n) time complexity.
pub fn compute_batch_metrics(
    env: &Env,
    transactions: &Vec<Transaction>,
    processed_at: u64,
) -> BatchMetrics {
    let tx_count = transactions.len();

    if tx_count == 0 {
        return BatchMetrics {
            tx_count: 0,
            total_volume: 0,
            avg_amount: 0,
            min_amount: 0,
            max_amount: 0,
            unique_senders: 0,
            unique_recipients: 0,
            total_fees: 0,
            processed_at,
        };
    }

    // Accumulate metrics in a single pass (optimization: avoid multiple iterations)
    let mut total_volume: i128 = 0;
    let mut min_amount: i128 = i128::MAX;
    let mut max_amount: i128 = i128::MIN;

    // Use maps to track unique addresses (more efficient than vectors for lookups)
    let mut senders: Map<Address, bool> = Map::new(env);
    let mut recipients: Map<Address, bool> = Map::new(env);

    for tx in transactions.iter() {
        // Accumulate volume
        total_volume = total_volume.checked_add(tx.amount).unwrap_or(i128::MAX);

        // Track min/max
        if tx.amount < min_amount {
            min_amount = tx.amount;
        }
        if tx.amount > max_amount {
            max_amount = tx.amount;
        }

        // Track unique addresses
        if !senders.contains_key(tx.from.clone()) {
            senders.set(tx.from.clone(), true);
        }
        if !recipients.contains_key(tx.to.clone()) {
            recipients.set(tx.to.clone(), true);
        }
    }

    // Calculate average (avoiding division by zero)
    let avg_amount = total_volume / (tx_count as i128);

    // Calculate fees on total volume for batch efficiency
    let total_fees = calculate_fee(total_volume);

    BatchMetrics {
        tx_count,
        total_volume,
        avg_amount,
        min_amount,
        max_amount,
        unique_senders: senders.len(),
        unique_recipients: recipients.len(),
        total_fees,
        processed_at,
    }
}

/// Computes category-specific metrics for analytics breakdown.
///
/// Groups transactions by category and computes volume distribution.
pub fn compute_category_metrics(
    env: &Env,
    transactions: &Vec<Transaction>,
    total_volume: i128,
) -> Vec<CategoryMetrics> {
    // Map stores (tx_count, total_volume)
    let mut category_map: Map<Symbol, (u32, i128)> = Map::new(env);

    // Single pass to aggregate by category
    for tx in transactions.iter() {
        let current = category_map.get(tx.category.clone()).unwrap_or((0, 0));
        category_map.set(
            tx.category.clone(),
            (
                current.0 + 1,
                current.1.checked_add(tx.amount).unwrap_or(i128::MAX),
            ),
        );
    }

    // Convert to CategoryMetrics vector
    let mut result: Vec<CategoryMetrics> = Vec::new(env);

    for (category, (tx_count, volume)) in category_map.iter() {
        // Calculate percentage in basis points (10000 = 100%)
        let volume_percentage_bps = if total_volume > 0 {
            ((volume * 10000) / total_volume) as u32
        } else {
            0
        };

        // Calculate fees on category volume
        let total_fees = calculate_fee(volume);

        result.push_back(CategoryMetrics {
            category,
            tx_count,
            total_volume: volume,
            total_fees,
            volume_percentage_bps,
        });
    }

    result
}

/// Identifies high-value transactions that exceed a threshold.
///
/// Returns a vector of (tx_id, amount) tuples for transactions above the threshold.
pub fn find_high_value_transactions(
    env: &Env,
    transactions: &Vec<Transaction>,
    threshold: i128,
) -> Vec<(u64, i128)> {
    let mut high_value: Vec<(u64, i128)> = Vec::new(env);

    for tx in transactions.iter() {
        if tx.amount >= threshold {
            high_value.push_back((tx.tx_id, tx.amount));
        }
    }

    high_value
}

/// Validates a batch of transactions before processing.
///
/// Returns Ok(()) if valid, or an error message if invalid.
pub fn validate_batch(transactions: &Vec<Transaction>) -> Result<(), &'static str> {
    let count = transactions.len();

    if count == 0 {
        return Err("Batch cannot be empty");
    }

    if count > MAX_BATCH_SIZE {
        return Err("Batch exceeds maximum size");
    }

    // Validate individual transactions
    for tx in transactions.iter() {
        if tx.amount < 0 {
            return Err("Transaction amount cannot be negative");
        }
    }

    Ok(())
}

/// Validates a batch of audit logs.
pub fn validate_audit_logs(logs: &Vec<AuditLog>) -> Result<(), &'static str> {
    if logs.len() == 0 {
        return Err("Audit logs batch cannot be empty");
    }

    if logs.len() > MAX_BATCH_SIZE {
        return Err("Audit logs batch exceeds maximum size");
    }

    for log in logs.iter() {
        // Simple check: operation cannot be empty (dummy symbol check if needed)
        // In Soroban, Symbols are usually non-empty if they represent meaningful strings.
        if log.timestamp == 0 {
            return Err("Audit log timestamp cannot be zero");
        }
    }

    Ok(())
}

/// Computes a simple checksum for batch integrity verification.
pub fn compute_batch_checksum(transactions: &Vec<Transaction>) -> u64 {
    let mut checksum: u64 = 0;

    for tx in transactions.iter() {
        // XOR tx_id and lower bits of amount for simple integrity check
        checksum ^= tx.tx_id;
        checksum ^= (tx.amount & 0xFFFFFFFF) as u64;
    }

    checksum
}

/// Validates a single transaction for bundling.
///
/// Returns a ValidationResult indicating whether the transaction is valid
/// and providing an error message if invalid.
pub fn validate_transaction_for_bundle(
    env: &Env,
    bundled_tx: &BundledTransaction,
) -> ValidationResult {
    let tx = &bundled_tx.transaction;

    // Validate transaction amount
    if tx.amount < 0 {
        return ValidationResult {
            tx_id: tx.tx_id,
            is_valid: false,
            error: Symbol::new(env, "invalid_amount"),
        };
    }

    // Validate addresses (cannot be the same)
    if tx.from == tx.to {
        return ValidationResult {
            tx_id: tx.tx_id,
            is_valid: false,
            error: Symbol::new(env, "same_address"),
        };
    }

    // Validate amount is not zero (optional - you might want to allow zero)
    // For now, we'll allow zero amounts

    // Transaction is valid
    ValidationResult {
        tx_id: tx.tx_id,
        is_valid: true,
        error: Symbol::new(env, ""),
    }
}

/// Validates all transactions in a bundle and returns validation results.
///
/// This function handles partial failures gracefully by validating each
/// transaction independently and returning results for all transactions.
pub fn validate_bundle_transactions(
    env: &Env,
    bundled_transactions: &Vec<BundledTransaction>,
) -> Vec<ValidationResult> {
    let mut results: Vec<ValidationResult> = Vec::new(env);

    for bundled_tx in bundled_transactions.iter() {
        let result = validate_transaction_for_bundle(env, &bundled_tx);
        results.push_back(result);
    }

    results
}

/// Creates a bundle result from validation results and transactions.
///
/// Computes bundle metrics and determines if the bundle can be created.
pub fn create_bundle_result(
    env: &Env,
    bundle_id: u64,
    bundled_transactions: &Vec<BundledTransaction>,
    validation_results: &Vec<ValidationResult>,
    created_at: u64,
) -> BundleResult {
    let total_count = bundled_transactions.len() as u32;
    let mut valid_count: u32 = 0;
    let mut invalid_count: u32 = 0;
    let mut total_volume: i128 = 0;

    // Count valid/invalid and compute total volume of valid transactions
    let mut index: u32 = 0;
    for result in validation_results.iter() {
        if result.is_valid {
            valid_count += 1;
            if let Some(bundled_tx) = bundled_transactions.get(index) {
                total_volume = total_volume
                    .checked_add(bundled_tx.transaction.amount)
                    .unwrap_or(i128::MAX);
            }
        } else {
            invalid_count += 1;
        }
        index += 1;
    }

    let can_bundle = valid_count > 0 && invalid_count == 0;

    BundleResult {
        bundle_id,
        total_count,
        valid_count,
        invalid_count,
        validation_results: validation_results.clone(),
        can_bundle,
        total_volume,
        created_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    fn create_test_transaction(env: &Env, tx_id: u64, amount: i128, category: &str) -> Transaction {
        Transaction {
            tx_id,
            from: Address::generate(env),
            to: Address::generate(env),
            amount,
            timestamp: 12345,
            category: Symbol::new(env, category),
        }
    }

    #[test]
    fn test_compute_batch_metrics_single_tx() {
        let env = Env::default();
        let mut transactions: Vec<Transaction> = Vec::new(&env);
        transactions.push_back(create_test_transaction(&env, 1, 1000, "transfer"));

        let metrics = compute_batch_metrics(&env, &transactions, 100);

        assert_eq!(metrics.tx_count, 1);
        assert_eq!(metrics.total_volume, 1000);
        assert_eq!(metrics.avg_amount, 1000);
        assert_eq!(metrics.min_amount, 1000);
        assert_eq!(metrics.max_amount, 1000);
        assert_eq!(metrics.unique_senders, 1);
        assert_eq!(metrics.unique_recipients, 1);
    }

    #[test]
    fn test_compute_batch_metrics_multiple_tx() {
        let env = Env::default();
        let mut transactions: Vec<Transaction> = Vec::new(&env);
        transactions.push_back(create_test_transaction(&env, 1, 100, "transfer"));
        transactions.push_back(create_test_transaction(&env, 2, 200, "transfer"));
        transactions.push_back(create_test_transaction(&env, 3, 300, "budget"));

        let metrics = compute_batch_metrics(&env, &transactions, 100);

        assert_eq!(metrics.tx_count, 3);
        assert_eq!(metrics.total_volume, 600);
        assert_eq!(metrics.avg_amount, 200);
        assert_eq!(metrics.min_amount, 100);
        assert_eq!(metrics.max_amount, 300);
    }

    #[test]
    fn test_compute_batch_metrics_empty() {
        let env = Env::default();
        let transactions: Vec<Transaction> = Vec::new(&env);

        let metrics = compute_batch_metrics(&env, &transactions, 100);

        assert_eq!(metrics.tx_count, 0);
        assert_eq!(metrics.total_volume, 0);
    }

    #[test]
    fn test_compute_category_metrics() {
        let env = Env::default();
        let mut transactions: Vec<Transaction> = Vec::new(&env);
        transactions.push_back(create_test_transaction(&env, 1, 500, "transfer"));
        transactions.push_back(create_test_transaction(&env, 2, 300, "transfer"));
        transactions.push_back(create_test_transaction(&env, 3, 200, "budget"));

        let category_metrics = compute_category_metrics(&env, &transactions, 1000);

        assert_eq!(category_metrics.len(), 2);
    }

    #[test]
    fn test_find_high_value_transactions() {
        let env = Env::default();
        let mut transactions: Vec<Transaction> = Vec::new(&env);
        transactions.push_back(create_test_transaction(&env, 1, 100, "transfer"));
        transactions.push_back(create_test_transaction(&env, 2, 5000, "transfer"));
        transactions.push_back(create_test_transaction(&env, 3, 10000, "budget"));

        let high_value = find_high_value_transactions(&env, &transactions, 1000);

        assert_eq!(high_value.len(), 2);
        assert_eq!(high_value.get(0).unwrap(), (2, 5000));
        assert_eq!(high_value.get(1).unwrap(), (3, 10000));
    }

    #[test]
    fn test_validate_batch_valid() {
        let env = Env::default();
        let mut transactions: Vec<Transaction> = Vec::new(&env);
        transactions.push_back(create_test_transaction(&env, 1, 100, "transfer"));

        assert!(validate_batch(&transactions).is_ok());
    }

    #[test]
    fn test_validate_batch_empty() {
        let env = Env::default();
        let transactions: Vec<Transaction> = Vec::new(&env);

        assert_eq!(validate_batch(&transactions), Err("Batch cannot be empty"));
    }

    #[test]
    fn test_validate_batch_negative_amount() {
        let env = Env::default();
        let mut transactions: Vec<Transaction> = Vec::new(&env);
        transactions.push_back(create_test_transaction(&env, 1, -100, "transfer"));

        assert_eq!(
            validate_batch(&transactions),
            Err("Transaction amount cannot be negative")
        );
    }

    #[test]
    fn test_compute_batch_checksum() {
        let env = Env::default();
        let mut transactions: Vec<Transaction> = Vec::new(&env);
        transactions.push_back(create_test_transaction(&env, 1, 100, "transfer"));
        transactions.push_back(create_test_transaction(&env, 2, 200, "transfer"));

        let checksum1 = compute_batch_checksum(&transactions);
        let checksum2 = compute_batch_checksum(&transactions);

        // Same batch should produce same checksum
        assert_eq!(checksum1, checksum2);
    }
}

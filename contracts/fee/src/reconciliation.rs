//! Compares stored fee-accounting balances against independently
//! calculated/on-chain balances to surface discrepancies. Every `pub fn`
//! below already carries a `///` doc comment; this module doc summarizes
//! the file as a whole for `cargo doc`.

use soroban_sdk::{contracttype, Env, token};

use crate::storage::{read_escrow_balance, read_total_collected, read_total_released, read_token, read_treasury};

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct ReconciliationResult {
    pub stored_balance: i128,
    pub calculated_balance: i128,
    pub discrepancy: i128,
    pub is_reconciled: bool,
}

/// Reconciles fee accounting by comparing the stored escrow balance with the
/// balance calculated from total collected fees minus total released fees.
///
/// The result reports both balances, their discrepancy, and whether the stored
/// value matches the calculated accounting state.
pub fn reconcile(env: &Env) -> ReconciliationResult {
    let stored_balance = read_escrow_balance(env);
    let total_collected = read_total_collected(env);
    let total_released = read_total_released(env);

    let calculated_balance = total_collected - total_released;
    let discrepancy = stored_balance - calculated_balance;

    ReconciliationResult {
        stored_balance,
        calculated_balance,
        discrepancy,
        is_reconciled: discrepancy == 0,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct ReconciliationReport {
    pub stored_balance: i128,
    pub actual_balance: i128,
    pub difference: i128,
    pub is_match: bool,
    pub timestamp: u64,
}

/// Compare the stored treasury fees (total released) against the actual token balance
/// of the treasury address. Returns a report describing any mismatch.
pub fn reconcile_treasury(env: &Env) -> ReconciliationReport {
    let stored_balance = read_total_released(env);
    
    let token_id = read_token(env);
    let token_client = token::Client::new(env, &token_id);
    let treasury = read_treasury(env);
    let actual_balance = token_client.balance(&treasury);

    let difference = actual_balance - stored_balance;

    ReconciliationReport {
        stored_balance,
        actual_balance,
        difference,
        is_match: difference == 0,
        timestamp: env.ledger().timestamp(),
    }
}

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

/// Compare the stored escrow balance against the calculated balance
/// (total_collected - total_released). Returns a result describing any
/// discrepancy between the two values.
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

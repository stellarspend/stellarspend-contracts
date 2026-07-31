//! Simulation module for read-only transaction validation and outcome projection.

use crate::transactions::{Transaction, TransactionOutcome};
use std::cell::RefCell;
use std::collections::BTreeMap;

thread_local! {
    /// Simulation outcomes keyed by simulation id.
    static SIMULATION_RESULTS: RefCell<BTreeMap<u64, TransactionOutcome>> =
        RefCell::new(BTreeMap::new());
}

/// Simulate a transaction without mutating state or writing to the ledger.
pub fn simulate_transaction(tx: &Transaction) -> Result<TransactionOutcome, SimulationError> {
    // Validate parameters
    if !tx.is_valid() {
        return Err(SimulationError::InvalidParameters);
    }
    // Project outcome (read-only)
    let outcome = tx.project_outcome();
    Ok(outcome)
}

/// Store a computed simulation outcome under `sim_id`, replacing any previous
/// entry for that id. Returns the outcome it replaced, if any.
pub fn store_simulation_result(
    sim_id: u64,
    outcome: TransactionOutcome,
) -> Option<TransactionOutcome> {
    SIMULATION_RESULTS.with(|results| results.borrow_mut().insert(sim_id, outcome))
}

/// Return the stored outcome for `sim_id`, or `None` if no simulation has been
/// stored under that id.
pub fn get_simulation_result(sim_id: u64) -> Option<TransactionOutcome> {
    SIMULATION_RESULTS.with(|results| results.borrow().get(&sim_id).cloned())
}

#[derive(Debug, PartialEq)]
pub enum SimulationError {
    InvalidParameters,
    // Add more error types as needed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transactions::{Transaction, TransactionOutcome};

    #[test]
    fn test_simulate_valid_transaction() {
        let tx = Transaction::mock_valid();
        let result = simulate_transaction(&tx);
        assert!(result.is_ok());
        let outcome = result.unwrap();
        assert_eq!(outcome, tx.project_outcome());
    }

    #[test]
    fn test_simulate_invalid_transaction() {
        let tx = Transaction::mock_invalid();
        let result = simulate_transaction(&tx);
        assert_eq!(result, Err(SimulationError::InvalidParameters));
    }

    #[test]
    fn test_get_simulation_result_returns_stored_outcome() {
        let tx = Transaction::mock_valid();
        let outcome = simulate_transaction(&tx).unwrap();
        store_simulation_result(1, outcome.clone());
        assert_eq!(get_simulation_result(1), Some(outcome));
    }

    #[test]
    fn test_get_simulation_result_unknown_id() {
        assert_eq!(get_simulation_result(u64::MAX), None);
    }
}

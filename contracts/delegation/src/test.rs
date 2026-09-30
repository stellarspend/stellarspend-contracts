//! Delegation contract tests.

use super::*;
use soroban_sdk::{Env, Address};
use crate::error::Error;

/// Test that `grant` rejects an invalid address.
#[test]
fn test_grant_invalid_address() {
    // Set up the test environment.
    let env = Env::default();

    // Register the contract.
    let contract_id = env.register_contract(None, Contract);
    let client = ContractClient::new(&env, &contract_id);

    // Create an address that is not a valid Ed25519 public key.
    // Using all zero bytes is considered invalid by the contract.
    let invalid_addr = Address::from_binary(&[0u8; 32]);

    // Attempt to grant delegation to the invalid address.
    let result = client.grant(&invalid_addr, &10);

    // The contract should return `InvalidAddress`.
    assert_eq!(result, Err(Error::InvalidAddress));
}

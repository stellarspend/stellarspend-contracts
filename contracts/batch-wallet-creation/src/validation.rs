//! Validation utilities for batch wallet creation.

use crate::types::WalletCreateRequest;
use soroban_sdk::{Address, Env, Vec};

/// Validates an owner address.
pub fn validate_address(_address: &Address) -> Result<(), ()> {
    // For now, assume all addresses are valid
    Ok(())
}

/// Checks if a wallet already exists for the given address.
pub fn wallet_exists(env: &Env, address: &Address) -> bool {
    use crate::types::DataKey;
    env.storage()
        .persistent()
        .has(&DataKey::Wallets(address.clone()))
}

/// Checks for duplicate wallet creation requests within a batch.
///
/// # Arguments
/// * `requests` - The vector of wallet creation requests to check
///
/// # Returns
/// Ok(()) if no duplicates, or Err(duplicate_address) if a duplicate is found
pub fn check_batch_duplicates(requests: &Vec<WalletCreateRequest>) -> Result<(), Address> {
    // Simple O(n^2) duplicate check for batch
    for i in 0..requests.len() {
        let request_i = requests.get(i).unwrap();

        for j in (i + 1)..requests.len() {
            let request_j = requests.get(j).unwrap();

            if request_i.owner == request_j.owner {
                // Found duplicate - return the duplicate address
                return Err(request_i.owner.clone());
            }
        }
    }
    Ok(())
}

/// Returns the first duplicate wallet address found in the batch, or None.
pub fn find_duplicate_wallet(
    env: &Env,
    wallets: &[soroban_sdk::Address],
) -> Option<soroban_sdk::Address> {
    let mut seen = soroban_sdk::Vec::<soroban_sdk::Address>::new(env);
    for wallet in wallets {
        if seen.iter().any(|item| item == *wallet) {
            return Some(wallet.clone());
        }
        seen.push_back(wallet.clone());
    }
    None
}

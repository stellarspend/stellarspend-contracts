<<<<<<< HEAD
use alloc::format;
use soroban_sdk::{Env, Symbol};
=======
use soroban_sdk::{Env, Symbol};
extern crate alloc;
>>>>>>> 067107d (fix(contracts): fix CI compilation errors across batch-transfer, spending-limits, multi-currency-wallet, and batch-rewards)

/// Generate a unique transaction ID
pub fn generate_transaction_id(env: &Env) -> Symbol {
    // Use a persistent counter for unique IDs
    let mut counter: u64 = env
        .storage()
        .persistent()
        .get(&crate::storage::DataKey::TransactionCounter)
        .unwrap_or(0);

    counter += 1;
<<<<<<< HEAD

=======
    
    // Create ID string
    let id_str = alloc::format!("tx{}", counter);
    
>>>>>>> 067107d (fix(contracts): fix CI compilation errors across batch-transfer, spending-limits, multi-currency-wallet, and batch-rewards)
    // Update counter
    env.storage()
        .persistent()
        .set(&crate::storage::DataKey::TransactionCounter, &counter);

    Symbol::new(env, &format!("tx{}", counter))
}

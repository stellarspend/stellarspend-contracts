#![no_std]

extern crate alloc;

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, Address,
    Env, Map, String, Symbol, Vec,
};

mod storage;
mod utils;

pub use storage::{
    clear_user_transactions, create_transaction, get_all_transactions, get_last_transaction,
    get_total_transactions_count, get_transaction, get_transaction_memo, get_transaction_metadata,
    get_transaction_timestamp, get_transactions_paginated, get_user_transactions,
    get_user_transactions_filtered, is_transaction_owner, set_transaction_metadata,
    transaction_exists, update_transaction_status, Transaction, TransactionStatus,
};

#[cfg(test)]
mod test;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum TransactionError {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
    TransactionNotFound = 4,
    InvalidAmount = 5,
    InvalidId = 6,
    TransactionLimitReached = 7,
    InvalidNoteLength = 8,
    DuplicateTransaction = 9,
}

const MAX_NOTE_LENGTH: u32 = 256;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
}

#[contract]
pub struct TransactionsContract;

#[contractimpl]
impl TransactionsContract {
    /// Initialize the transactions contract with an admin address
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&env, TransactionError::AlreadyInitialized);
        }

        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);

        env.events()
            .publish((symbol_short!("tx"), symbol_short!("init")), admin);
    }

    /// Create a new transaction
    pub fn create_transaction(
        env: Env,
        from: Address,
        to: Address,
        amount: i128,
        note: String,
        memo: String,
        tags: Vec<String>,
        tx_type: Symbol,
        is_public: bool,
        metadata: Map<Symbol, String>,
    ) -> Symbol {
        from.require_auth();

        if amount < storage::MIN_TRANSACTION_AMOUNT {
            panic_with_error!(&env, TransactionError::InvalidAmount);
        }

        if amount > storage::MAX_TRANSACTION_AMOUNT {
            panic_with_error!(&env, TransactionError::InvalidAmount);
        }

        if note.len() as u32 > MAX_NOTE_LENGTH {
            panic_with_error!(&env, TransactionError::InvalidNoteLength);
        }

        // Check for duplicate transactions
        if storage::is_duplicate_transaction(&env, from.clone(), to.clone(), amount, memo.clone()) {
            panic_with_error!(&env, TransactionError::DuplicateTransaction);
        }

        let transaction = create_transaction(
            &env,
            from.clone(),
            to,
            amount,
            note,
            memo,
            tags,
            tx_type,
            is_public,
            metadata,
        );

        env.events().publish(
            (symbol_short!("tx"), symbol_short!("created")),
            (
                transaction.id.clone(),
                transaction.from.clone(),
                transaction.to.clone(),
                transaction.amount,
                transaction.timestamp,
            ),
        );

        transaction.id
    }

    /// Set (replace) the metadata key-value pairs for an existing transaction.
    /// Only the transaction owner can update metadata.
    pub fn set_metadata(
        env: Env,
        id: Symbol,
        caller: Address,
        metadata: Map<Symbol, String>,
    ) -> bool {
        caller.require_auth();

        if !transaction_exists(&env, id.clone()) {
            panic_with_error!(&env, TransactionError::TransactionNotFound);
        }

        let success = storage::set_transaction_metadata(&env, id.clone(), caller, metadata);

        if success {
            env.events()
                .publish((symbol_short!("tx"), symbol_short!("meta_set")), id);
        }

        success
    }

    /// Get the metadata key-value pairs for a transaction.
    pub fn get_metadata(env: Env, id: Symbol) -> Option<Map<Symbol, String>> {
        storage::get_transaction_metadata(&env, id)
    }

    /// Update the note attached to a transaction (only transaction owner can update)
    pub fn update_transaction_note(env: Env, id: Symbol, caller: Address, note: String) -> bool {
        caller.require_auth();

        if note.len() as u32 > MAX_NOTE_LENGTH {
            panic_with_error!(&env, TransactionError::InvalidNoteLength);
        }

        if !transaction_exists(&env, id.clone()) {
            panic_with_error!(&env, TransactionError::TransactionNotFound);
        }

        let success = storage::update_transaction_note(&env, id.clone(), caller, note);

        if success {
            env.events()
                .publish((symbol_short!("tx"), symbol_short!("note_upd")), id.clone());
        }

        success
    }

    /// Get all transactions for a user, sorted by timestamp (descending)
    pub fn get_user_transactions_sorted(env: Env, user: Address) -> Vec<Transaction> {
        let mut transactions = get_user_transactions(&env, user);

        // Simple bubble sort for demonstration (on-chain sorting can be expensive)
        let n = transactions.len();
        if n > 1 {
            for i in 0..n {
                for j in 0..n - i - 1 {
                    let tx_j = transactions.get(j).unwrap();
                    let tx_next = transactions.get(j + 1).unwrap();
                    if tx_j.timestamp < tx_next.timestamp {
                        transactions.set(j, tx_next);
                        transactions.set(j + 1, tx_j);
                    }
                }
            }
        }
        transactions
    }

    /// Get the last (most recent) transaction for a user
    pub fn get_last_transaction(env: Env, user: Address) -> Option<Transaction> {
        get_last_transaction(&env, user)
    }

    /// Get the total number of transactions recorded in the contract
    pub fn get_total_transactions_count(env: Env) -> u64 {
        get_total_transactions_count(&env)
    }

    /// Get all transactions in the contract
    pub fn get_all_transactions(env: Env) -> Vec<Transaction> {
        get_all_transactions(&env)
    }

    /// Get the total income from all transactions
    pub fn get_total_income(env: Env) -> i128 {
        storage::get_total_income(&env)
    }

    /// Get the total expense from all transactions
    pub fn get_total_expense(env: Env) -> i128 {
        storage::get_total_expense(&env)
    }

    /// Get a paginated subset of all transactions.
    ///
    /// - `offset`: number of transactions to skip (0-based)
    /// - `limit`:  maximum number of transactions to return (capped at 100)
    pub fn get_transactions_paginated(env: Env, offset: u32, limit: u32) -> Vec<Transaction> {
        get_transactions_paginated(&env, offset, limit)
    }

    /// Clear all transactions for a user (only user can perform this action)
    pub fn clear_user_transactions(env: Env, user: Address) -> bool {
        user.require_auth();

        let success = clear_user_transactions(&env, user.clone());

        if success {
            env.events()
                .publish((symbol_short!("tx"), symbol_short!("cleared")), user);
        }

        success
    }
    /// Update the amount for a transaction (only transaction owner can update)
    pub fn update_transaction_amount(env: Env, id: Symbol, caller: Address, amount: i128) -> bool {
        caller.require_auth();

        if amount < storage::MIN_TRANSACTION_AMOUNT || amount > storage::MAX_TRANSACTION_AMOUNT {
            panic_with_error!(&env, TransactionError::InvalidAmount);
        }

        if !transaction_exists(&env, id.clone()) {
            panic_with_error!(&env, TransactionError::TransactionNotFound);
        }

        let success = storage::update_transaction_amount(&env, id.clone(), caller, amount);

        if success {
            env.events().publish(
                (symbol_short!("tx"), symbol_short!("amount_up")),
                id.clone(),
            );
        }

        success
    }

    /// Get the admin address
    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::Admin)
    }
    /// Get the timestamp of a transaction
    pub fn get_transaction_timestamp(env: Env, id: Symbol) -> Option<u64> {
        get_transaction_timestamp(&env, id)
    }

    /// Get a transaction by ID
    pub fn get_transaction(env: Env, id: Symbol) -> Option<Transaction> {
        get_transaction(&env, id)
    }

    /// Get all transactions for a user
    pub fn get_user_transactions(env: Env, user: Address) -> Vec<Transaction> {
        get_user_transactions(&env, user)
    }

    /// Get all transactions for a user (alias for get_user_transactions)
    pub fn get_transactions_by_user(env: Env, user: Address) -> Vec<Transaction> {
        get_user_transactions(&env, user)
    }

    /// Get all transactions for a user filtered by `tx_type`.
    pub fn get_user_transactions_filtered(
        env: Env,
        user: Address,
        tx_type: Symbol,
    ) -> Vec<Transaction> {
        storage::get_user_transactions_filtered(&env, user, tx_type)
    }

    /// Check if a transaction exists
    pub fn transaction_exists(env: Env, id: Symbol) -> bool {
        transaction_exists(&env, id)
    }

    /// Delete a transaction (only owner or admin allowed)
    pub fn delete_transaction(env: Env, caller: Address, id: Symbol) -> bool {
        caller.require_auth();

        if !transaction_exists(&env, id.clone()) {
            panic_with_error!(&env, TransactionError::TransactionNotFound);
        }

        // Allow if caller is owner or admin
        let is_owner = is_transaction_owner(&env, id.clone(), caller.clone());
        if !is_owner {
            Self::require_admin(&env, &caller);
        }

        let success = storage::delete_transaction(&env, id.clone());
        if success {
            env.events()
                .publish((symbol_short!("tx"), symbol_short!("deleted")), id.clone());
        }
        success
    }

    pub fn update_transaction_status(
        env: Env,
        id: Symbol,
        caller: Address,
        status: TransactionStatus,
    ) -> bool {
        caller.require_auth();

        if !transaction_exists(&env, id.clone()) {
            panic_with_error!(&env, TransactionError::TransactionNotFound);
        }
<<<<<<< HEAD

        let success = storage::update_transaction_status(&env, id.clone(), caller, status.clone());

        if success {
            env.events().publish(
                (symbol_short!("tx"), symbol_short!("tx_status")),
=======
        
        let success = storage::update_transaction_status(&env, id.clone(), caller, status.clone());
        
        if success {
            env.events().publish(
                (symbol_short!("tx"), symbol_short!("stat_upd")),
>>>>>>> 067107d (fix(contracts): fix CI compilation errors across batch-transfer, spending-limits, multi-currency-wallet, and batch-rewards)
                (id.clone(), status),
            );
        }

        success
    }

    /// Check if a user is the owner of a transaction
    pub fn is_transaction_owner(env: Env, id: Symbol, user: Address) -> bool {
        is_transaction_owner(&env, id, user)
    }

    /// Get transaction memo
    pub fn get_transaction_memo(env: Env, id: Symbol) -> Option<String> {
        get_transaction_memo(&env, id)
    }

    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(env, TransactionError::NotInitialized));
        if caller != &admin {
            panic_with_error!(env, TransactionError::Unauthorized);
        }
    }
}

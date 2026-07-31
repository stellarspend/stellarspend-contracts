//! # Transaction Memo Contract
//!
//! A Soroban smart contract for managing transaction memos with
//! length validation to prevent oversized payloads and manage storage costs.
//!
//! ## Features
//!
//! - **Memo Storage**: Store and retrieve transaction memos
//! - **Length Validation**: Reject oversized memo text, references, and total payload
//! - **Boundary Checks**: Validates memo type, reference, and text lengths
//! - **Event Emission**: Emits events for memo operations

#![no_std]

mod validation;

use soroban_sdk::{contract, contractimpl, contracttype, panic_with_error, symbol_short, Address, Bytes, Env, String, Symbol};

use crate::validation::{validate_memo_text_length, validate_memo_reference_length, validate_total_memo_size};

/// Error codes for the transaction memo contract.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum MemoError {
    /// Contract not initialized
    NotInitialized = 1,
    /// Caller is not authorized
    Unauthorized = 2,
    /// Memo text is empty
    EmptyMemoText = 3,
    /// Memo text exceeds maximum length
    MemoTextTooLarge = 4,
    /// Memo reference exceeds maximum length
    MemoReferenceTooLarge = 5,
    /// Total memo payload exceeds maximum size
    MemoTooLarge = 6,
    /// Invalid memo type
    InvalidMemoType = 7,
}

impl From<MemoError> for soroban_sdk::Error {
    fn from(e: MemoError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

/// Represents a transaction memo.
#[derive(Clone, Debug)]
#[contracttype]
pub struct TransactionMemo {
    /// The transaction ID this memo belongs to
    pub transaction_id: Bytes,
    /// The memo type (e.g., "payment", "refund", "note")
    pub memo_type: Symbol,
    /// Optional reference string
    pub reference: String,
    /// The memo text content
    pub text: String,
    /// When the memo was created (ledger timestamp)
    pub created_at: u64,
}

/// Storage keys for the contract.
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    Memo(Bytes),
    TotalMemos,
}

/// Events emitted by the contract.
pub struct MemoEvents;

impl MemoEvents {
    pub fn memo_stored(env: &Env, transaction_id: &Bytes, memo_type: &Symbol, created_at: u64) {
        let topics = (symbol_short!("memo"), symbol_short!("stored"));
        env.events().publish(topics, (transaction_id.clone(), memo_type.clone(), created_at));
    }
}

#[contract]
pub struct TransactionMemoContract;

#[contractimpl]
impl TransactionMemoContract {
    /// Initializes the contract with an admin address.
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Contract already initialized");
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::TotalMemos, &0u64);
    }

    /// Stores a transaction memo with length validation.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `caller` - The admin address
    /// * `transaction_id` - Unique transaction identifier
    /// * `memo_type` - Type of memo
    /// * `reference` - Optional reference string
    /// * `text` - Memo text content
    pub fn set_memo(
        env: Env,
        caller: Address,
        transaction_id: Bytes,
        memo_type: Symbol,
        reference: String,
        text: String,
    ) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        // Validate memo type (must not be empty)
        if memo_type.len() == 0 {
            panic_with_error!(&env, MemoError::InvalidMemoType);
        }

        // Validate text length
        if let Err(_) = validate_memo_text_length(&text) {
            if text.len() == 0 {
                panic_with_error!(&env, MemoError::EmptyMemoText);
            } else {
                panic_with_error!(&env, MemoError::MemoTextTooLarge);
            }
        }

        // Validate reference length
        if let Err(_) = validate_memo_reference_length(&reference) {
            panic_with_error!(&env, MemoError::MemoReferenceTooLarge);
        }

        // Validate total memo size
        if let Err(_) = validate_total_memo_size(
            text.len() as u32,
            reference.len() as u32,
            memo_type.len() as u32,
        ) {
            panic_with_error!(&env, MemoError::MemoTooLarge);
        }

        let created_at = env.ledger().timestamp();

        let memo = TransactionMemo {
            transaction_id: transaction_id.clone(),
            memo_type: memo_type.clone(),
            reference: reference.clone(),
            text: text.clone(),
            created_at,
        };

        // Store the memo
        env.storage()
            .persistent()
            .set(&DataKey::Memo(transaction_id.clone()), &memo);

        // Update total count
        let total: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalMemos)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::TotalMemos, &(total + 1));

        // Emit event
        MemoEvents::memo_stored(&env, &transaction_id, &memo_type, created_at);
    }

    /// Retrieves a transaction memo by transaction ID.
    pub fn get_memo(env: Env, transaction_id: Bytes) -> Option<TransactionMemo> {
        env.storage()
            .persistent()
            .get(&DataKey::Memo(transaction_id))
    }

    /// Returns the admin address.
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Contract not initialized")
    }

    /// Updates the admin address.
    pub fn set_admin(env: Env, current_admin: Address, new_admin: Address) {
        current_admin.require_auth();
        Self::require_admin(&env, &current_admin);

        env.storage().instance().set(&DataKey::Admin, &new_admin);
    }

    /// Retrieves the memo text for a transaction by transaction ID.
    ///
    /// Returns `Some(String)` with the memo text if a memo exists for the
    /// given transaction ID, or `None` if no memo is found.
    pub fn get_transaction_memo(env: Env, transaction_id: Bytes) -> Option<String> {
        env.storage()
            .persistent()
            .get(&DataKey::Memo(transaction_id))
            .map(|memo: TransactionMemo| memo.text)
    }

    /// Returns the total number of memos stored.
    pub fn get_total_memos(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalMemos)
            .unwrap_or(0)
    }

    // Internal helper to verify admin
    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Contract not initialized");

        if *caller != admin {
            panic_with_error!(env, MemoError::Unauthorized);
        }
    }
}

#[cfg(test)]
mod test;

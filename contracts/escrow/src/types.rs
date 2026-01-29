//! Data types and events for the escrow contract.

use soroban_sdk::{contracttype, symbol_short, Address, Env, Vec};

/// Maximum number of escrows in a single batch operation.
pub const MAX_BATCH_SIZE: u32 = 100;

/// Escrow status enum.
#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub enum EscrowStatus {
    /// Funds locked, awaiting release or reversal
    Active,
    /// Funds released to recipient
    Released,
    /// Funds returned to depositor (reversed)
    Reversed,
}

/// An escrow record.
#[derive(Clone, Debug)]
#[contracttype]
pub struct Escrow {
    pub escrow_id: u64,
    pub depositor: Address,
    pub recipient: Address,
    pub token: Address,
    pub amount: i128,
    pub status: EscrowStatus,
    pub created_at: u64,
    pub deadline: u64,
}

/// Request to reverse an escrow.
#[derive(Clone, Debug)]
#[contracttype]
pub struct ReversalRequest {
    pub escrow_id: u64,
}

/// Result of a single escrow reversal.
#[derive(Clone, Debug)]
#[contracttype]
pub enum ReversalResult {
    /// Successful reversal: escrow_id, depositor, amount
    Success(u64, Address, i128),
    /// Failed reversal: escrow_id, error_code
    Failure(u64, u32),
}

/// Summary result of a batch reversal operation.
#[derive(Clone, Debug)]
#[contracttype]
pub struct BatchReversalResult {
    pub batch_id: u64,
    pub total_requests: u32,
    pub successful: u32,
    pub failed: u32,
    pub total_reversed: i128,
    pub results: Vec<ReversalResult>,
}

/// Storage keys for the escrow contract.
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    /// Admin address
    Admin,
    /// Token address used for escrows
    Token,
    /// Individual escrow by ID
    Escrow(u64),
    /// List of escrow IDs for a user (depositor)
    UserEscrows(Address),
    /// Counter for escrow IDs
    EscrowCounter,
    /// Total number of reversal batches processed
    TotalReversalBatches,
    /// Total number of escrows reversed
    TotalEscrowsReversed,
    /// Total amount reversed
    TotalAmountReversed,
}

/// Event emitters for escrow operations.
pub struct EscrowEvents;

impl EscrowEvents {
    /// Emitted when an escrow is created.
    pub fn escrow_created(
        env: &Env,
        escrow_id: u64,
        depositor: &Address,
        recipient: &Address,
        amount: i128,
    ) {
        let topics = (symbol_short!("escrow"), symbol_short!("created"));
        env.events()
            .publish(topics, (escrow_id, depositor.clone(), recipient.clone(), amount));
    }

    /// Emitted when a batch reversal starts.
    pub fn batch_reversal_started(env: &Env, batch_id: u64, request_count: u32) {
        let topics = (symbol_short!("escrow"), symbol_short!("rev_start"));
        env.events().publish(topics, (batch_id, request_count));
    }

    /// Emitted when a single escrow is successfully reversed.
    pub fn reversal_success(
        env: &Env,
        batch_id: u64,
        escrow_id: u64,
        depositor: &Address,
        amount: i128,
    ) {
        let topics = (symbol_short!("escrow"), symbol_short!("rev_ok"), batch_id);
        env.events()
            .publish(topics, (escrow_id, depositor.clone(), amount));
    }

    /// Emitted when a single escrow reversal fails.
    pub fn reversal_failure(env: &Env, batch_id: u64, escrow_id: u64, error_code: u32) {
        let topics = (symbol_short!("escrow"), symbol_short!("rev_fail"), batch_id);
        env.events().publish(topics, (escrow_id, error_code));
    }

    /// Emitted when a batch reversal completes.
    pub fn batch_reversal_completed(
        env: &Env,
        batch_id: u64,
        successful: u32,
        failed: u32,
        total_reversed: i128,
    ) {
        let topics = (symbol_short!("escrow"), symbol_short!("rev_done"), batch_id);
        env.events().publish(topics, (successful, failed, total_reversed));
    }

    /// Emitted when an escrow is released to recipient.
    pub fn escrow_released(env: &Env, escrow_id: u64, recipient: &Address, amount: i128) {
        let topics = (symbol_short!("escrow"), symbol_short!("released"));
        env.events().publish(topics, (escrow_id, recipient.clone(), amount));
    }
}

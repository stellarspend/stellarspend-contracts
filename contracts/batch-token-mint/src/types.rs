//! Data types and events for batch token minting operations.

use shared::batch_result::BatchItemResult;
use soroban_sdk::{contracttype, symbol_short, Address, Env, Vec};

/// Maximum number of mint operations in a single batch for optimization.
pub const MAX_BATCH_SIZE: u32 = 100;

/// Minimum mint amount (1 stroops)
pub const MIN_MINT_AMOUNT: i128 = 1;

/// Maximum mint amount (1 trillion XLM in stroops)
pub const MAX_MINT_AMOUNT: i128 = 1_000_000_000_000_000_000_000;

/// Represents a token minting request for a single user.
#[derive(Clone, Debug)]
#[contracttype]
pub struct TokenMintRequest {
    /// Recipient's address
    pub recipient: Address,
    /// Amount to mint (in stroops)
    pub amount: i128,
}

/// Represents a successfully minted token transaction.
#[derive(Clone, Debug)]
#[contracttype]
pub struct TokenMinted {
    /// Token contract address
    pub token_address: Address,
    /// Recipient address
    pub recipient: Address,
    /// Amount minted (in stroops)
    pub amount: i128,
    /// Ledger sequence when minted
    pub minted_at: u64,
}

/// Result of processing a single mint operation.
#[derive(Clone, Debug)]
#[contracttype]
pub enum MintResult {
    Success(TokenMinted),
    Failure(Address, u32), // recipient address, error code
}

/// Aggregated metrics for a batch of minting operations.
#[derive(Clone, Debug)]
#[contracttype]
pub struct BatchMintMetrics {
    /// Total number of mint requests
    pub total_requests: u32,
    /// Number of successful mints
    pub successful_mints: u32,
    /// Number of failed mints
    pub failed_mints: u32,
    /// Total amount minted
    pub total_amount_minted: i128,
    /// Average mint amount
    pub avg_mint_amount: i128,
    /// Batch processing timestamp
    pub processed_at: u64,
}

/// Result of batch token minting.
#[derive(Clone, Debug)]
#[contracttype]
pub struct BatchMintResult {
    /// Batch ID
    pub batch_id: u64,
    /// Token address being minted
    pub token_address: Address,
    /// Total number of requests
    pub total_requests: u32,
    /// Number of successful mints
    pub successful: u32,
    /// Number of failed mints
    pub failed: u32,
    /// Individual mint results
    pub results: Vec<MintResult>,
    /// Shared batch item results for callers that need a uniform schema
    pub shared_results: Vec<BatchItemResult>,
    /// Aggregated metrics
    pub metrics: BatchMintMetrics,
}

/// Storage keys for contract state.
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    /// Admin address (can authorize minting)
    Admin,
    /// Last created batch ID
    LastBatchId,
    /// Total tokens minted lifetime
    TotalMinted,
    /// Total batches processed lifetime
    TotalBatchesProcessed,
}

/// Error codes for token minting validation and execution.
pub mod ErrorCode {
    /// Invalid mint amount (too low or negative)
    pub const INVALID_AMOUNT: u32 = 0;
    /// Invalid recipient address
    pub const INVALID_RECIPIENT: u32 = 1;
    /// Caller is not authorized to mint
    pub const UNAUTHORIZED: u32 = 2;
    /// Token contract address is invalid
    pub const INVALID_TOKEN: u32 = 3;
    /// Batch is empty
    pub const EMPTY_BATCH: u32 = 4;
    /// Batch exceeds maximum size
    pub const BATCH_TOO_LARGE: u32 = 5;
    /// Contract not initialized
    pub const NOT_INITIALIZED: u32 = 6;
    /// Amount exceeds maximum allowed
    pub const AMOUNT_TOO_LARGE: u32 = 7;
}

/// Events emitted by the batch token mint contract.
pub struct MintEvents;

impl MintEvents {
    /// Event emitted when batch minting starts.
    pub fn batch_started(env: &Env, batch_id: u64, token: &Address, count: u32) {
        let topics = (symbol_short!("mint"), symbol_short!("start"));
        env.events()
            .publish(topics, (batch_id, token.clone(), count));
    }

    /// Event emitted when tokens are successfully minted.
    pub fn tokens_minted(env: &Env, batch_id: u64, token: &Address, minted: &TokenMinted) {
        let topics = (symbol_short!("mint"), symbol_short!("success"));
        env.events().publish(
            topics,
            (
                batch_id,
                token.clone(),
                minted.recipient.clone(),
                minted.amount,
            ),
        );
    }

    /// Event emitted when minting fails for a recipient.
    pub fn mint_failed(
        env: &Env,
        batch_id: u64,
        token: &Address,
        recipient: &Address,
        error_code: u32,
    ) {
        let topics = (symbol_short!("mint"), symbol_short!("failed"));
        env.events().publish(
            topics,
            (batch_id, token.clone(), recipient.clone(), error_code),
        );
    }

    /// Event emitted when batch minting completes.
    pub fn batch_completed(
        env: &Env,
        batch_id: u64,
        token: &Address,
        successful: u32,
        failed: u32,
        total_amount: i128,
    ) {
        let topics = (symbol_short!("mint"), symbol_short!("done"));
        env.events().publish(
            topics,
            (batch_id, token.clone(), successful, failed, total_amount),
        );
    }

    /// Event emitted for large mint operations (>= 1 billion stroops).
    pub fn large_mint(
        env: &Env,
        batch_id: u64,
        token: &Address,
        recipient: &Address,
        amount: i128,
    ) {
        let topics = (symbol_short!("mint"), symbol_short!("large"));
        env.events()
            .publish(topics, (batch_id, token.clone(), recipient.clone(), amount));
    }
}

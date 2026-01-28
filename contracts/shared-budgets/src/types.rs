// Types and events for shared budget batch allocations.

use soroban_sdk::{contracttype, symbol_short, Address, Env, Vec};

/// Maximum number of allocation entries in a single batch.
pub const MAX_BATCH_SIZE: u32 = 100;

/// A single allocation request from a shared budget to a recipient.
#[derive(Clone, Debug)]
#[contracttype]
pub struct AllocationRequest {
    /// Recipient address
    pub recipient: Address,
    /// Amount to allocate to the recipient
    pub amount: i128,
}

/// Result of processing a single allocation.
#[derive(Clone, Debug)]
#[contracttype]
pub enum AllocationResult {
    Success(Address, i128),      // recipient, amount
    Failure(Address, i128, u32), // recipient, requested amount, error_code
}

/// Aggregated result for a batch of allocations.
#[derive(Clone, Debug)]
#[contracttype]
pub struct AllocationBatchResult {
    /// Total allocation requests in the batch
    pub total_requests: u32,
    /// Number of successful allocations
    pub successful: u32,
    /// Number of failed allocations
    pub failed: u32,
    /// Total amount successfully allocated
    pub total_allocated: i128,
    /// Individual allocation results
    pub results: Vec<AllocationResult>,
}

/// Storage keys for contract state.
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    /// Admin address
    Admin,
    /// Total batches processed
    TotalBatches,
    /// Total allocation entries processed
    TotalAllocationsProcessed,
    /// Total amount allocated across all batches
    TotalAllocatedVolume,
}

/// Events emitted by the shared budgets contract.
pub struct SharedBudgetEvents;

impl SharedBudgetEvents {
    /// Event emitted when allocation batch processing starts.
    pub fn batch_started(env: &Env, batch_id: u64, request_count: u32) {
        let topics = (symbol_short!("alloc"), symbol_short!("started"));
        env.events().publish(topics, (batch_id, request_count));
    }

    /// Event emitted when an allocation succeeds for a recipient.
    pub fn allocation_success(env: &Env, batch_id: u64, recipient: &Address, amount: i128) {
        let topics = (symbol_short!("alloc"), symbol_short!("success"), batch_id);
        env.events().publish(topics, (recipient.clone(), amount));
    }

    /// Event emitted when an allocation fails for a recipient.
    pub fn allocation_failure(
        env: &Env,
        batch_id: u64,
        recipient: &Address,
        amount: i128,
        error_code: u32,
    ) {
        let topics = (symbol_short!("alloc"), symbol_short!("failed"), batch_id);
        env.events().publish(topics, (recipient.clone(), amount, error_code));
    }

    /// Event emitted when allocation batch processing completes.
    pub fn batch_completed(
        env: &Env,
        batch_id: u64,
        successful: u32,
        failed: u32,
        total_allocated: i128,
    ) {
        let topics = (symbol_short!("alloc"), symbol_short!("completed"), batch_id);
        env.events().publish(topics, (successful, failed, total_allocated));
    }
}

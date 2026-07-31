use shared::batch_result::BatchItemResult;
use soroban_sdk::{contracttype, symbol_short, Address, Env, Vec};
pub const MAX_BATCH_SIZE: u32 = 100;
#[derive(Clone, Debug)]
#[contracttype]
pub struct TransferRequest {
    pub recipient: Address,
    pub amount: i128,
}
#[derive(Clone, Debug)]
#[contracttype]
pub struct BurnRequest {
    pub owner: Address,
    pub amount: i128,
}
#[derive(Clone, Debug)]
#[contracttype]
pub enum TransferResult {
    Success(Address, i128),
    Failure(Address, i128, u32),
}
#[derive(Clone, Debug)]
#[contracttype]
pub enum BurnResult {
    Success(Address, i128),
    Failure(Address, i128, u32),
}
#[derive(Clone, Debug)]
#[contracttype]
pub struct BatchTransferResult {
    pub total_requests: u32,
    pub successful: u32,
    pub failed: u32,
    pub total_transferred: i128,
    pub results: Vec<TransferResult>,
    pub shared_results: Vec<BatchItemResult>,
}
#[derive(Clone, Debug)]
#[contracttype]
pub struct BatchBurnResult {
    pub total_requests: u32,
    pub successful: u32,
    pub failed: u32,
    pub total_burned: i128,
    pub results: Vec<BurnResult>,
    pub shared_results: Vec<BatchItemResult>,
}
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    TotalBatches,
    TotalTransfersProcessed,
    TotalVolumeTransferred,
    BatchRecipientCount(u64),
}
pub struct TransferEvents;
impl TransferEvents {
    pub fn batch_started(env: &Env, batch_id: u64, request_count: u32) {
        let topics = (symbol_short!("batch"), symbol_short!("started"));
        env.events().publish(topics, (batch_id, request_count));
    }
    pub fn transfer_success(env: &Env, batch_id: u64, recipient: &Address, amount: i128) {
        let topics = (
            symbol_short!("transfer"),
            symbol_short!("success"),
            batch_id,
        );
        env.events().publish(topics, (recipient.clone(), amount));
    }
    pub fn transfer_failure(
        env: &Env,
        batch_id: u64,
        recipient: &Address,
        requested_amount: i128,
        error_code: u32,
    ) {
        let topics = (
            symbol_short!("transfer"),
            symbol_short!("failure"),
            batch_id,
        );
        env.events()
            .publish(topics, (recipient.clone(), requested_amount, error_code));
    }
    pub fn batch_completed(
        env: &Env,
        batch_id: u64,
        successful: u32,
        failed: u32,
        total_transferred: i128,
    ) {
        let topics = (symbol_short!("batch"), symbol_short!("completed"), batch_id);
        env.events()
            .publish(topics, (successful, failed, total_transferred));
    }
    pub fn burn_success(env: &Env, batch_id: u64, owner: &Address, amount: i128) {
        let topics = (symbol_short!("burn"), symbol_short!("success"), batch_id);
        env.events().publish(topics, (owner.clone(), amount));
    }
    pub fn burn_failure(
        env: &Env,
        batch_id: u64,
        owner: &Address,
        requested_amount: i128,
        error_code: u32,
    ) {
        let topics = (symbol_short!("burn"), symbol_short!("failure"), batch_id);
        env.events()
            .publish(topics, (owner.clone(), requested_amount, error_code));
    }
    pub fn burn_batch_completed(
        env: &Env,
        batch_id: u64,
        successful: u32,
        failed: u32,
        total_burned: i128,
    ) {
        let topics = (symbol_short!("burn"), symbol_short!("completed"), batch_id);
        env.events()
            .publish(topics, (successful, failed, total_burned));
    }
}

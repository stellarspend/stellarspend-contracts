use shared::batch_result::BatchItemResult;
use soroban_sdk::{contracttype, symbol_short, Address, Env, Vec};

pub const MAX_BATCH_SIZE: u32 = 100;

#[derive(Clone, Debug)]
#[contracttype]
pub struct WalletCreateRequest {
    pub owner: Address,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct WalletRecoveryRequest {
    pub old_owner: Address,
    pub new_owner: Address,
}

#[derive(Clone, Debug)]
#[contracttype]
pub enum WalletCreateResult {
    Success(Address),
    Failure(Address, u32),
}

#[derive(Clone, Debug)]
#[contracttype]
pub enum WalletRecoveryResult {
    Success(Address, Address),
    Failure(Address, Address, u32),
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct BatchCreateResult {
    pub total_requests: u32,
    pub successful: u32,
    pub failed: u32,
    pub results: Vec<WalletCreateResult>,
    pub shared_results: Vec<BatchItemResult>,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct BatchRecoveryResult {
    pub total_requests: u32,
    pub successful: u32,
    pub failed: u32,
    pub results: Vec<WalletRecoveryResult>,
    pub shared_results: Vec<BatchItemResult>,
}

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    TotalBatches,
    TotalWalletsCreated,
    Wallets(Address), // Map of address to wallet id or something
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct Wallet {
    pub id: u64,
    pub owner: Address,
    pub created_at: u64,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct WalletCreatedEvent {
    pub owner: Address,
    pub wallet_id: u64,
    pub created_at: u64,
}

pub struct WalletEvents;

impl WalletEvents {
    pub fn batch_started(env: &Env, batch_id: u64, request_count: u32) {
        let topics = (symbol_short!("batch"), symbol_short!("started"));
        env.events().publish(topics, (batch_id, request_count));
    }

    pub fn wallet_created(env: &Env, batch_id: u64, event: &WalletCreatedEvent) {
        let topics = (symbol_short!("wallet"), symbol_short!("created"), batch_id);
        env.events().publish(topics, event.clone());
    }

    pub fn wallet_creation_failure(env: &Env, batch_id: u64, owner: &Address, error_code: u32) {
        let topics = (symbol_short!("wallet"), symbol_short!("failure"), batch_id);
        env.events().publish(topics, (owner.clone(), error_code));
    }

    pub fn batch_completed(env: &Env, batch_id: u64, successful: u32, failed: u32) {
        let topics = (symbol_short!("batch"), symbol_short!("completed"), batch_id);
        env.events().publish(topics, (successful, failed));
    }

    pub fn recovery_started(env: &Env, batch_id: u64, request_count: u32) {
        let topics = (symbol_short!("recovery"), symbol_short!("started"));
        env.events().publish(topics, (batch_id, request_count));
    }

    pub fn wallet_recovered(
        env: &Env,
        batch_id: u64,
        old_owner: &Address,
        new_owner: &Address,
        wallet_id: u64,
    ) {
        let topics = (
            symbol_short!("recovery"),
            symbol_short!("success"),
            batch_id,
        );
        env.events()
            .publish(topics, (old_owner.clone(), new_owner.clone(), wallet_id));
    }

    pub fn wallet_recovery_failure(
        env: &Env,
        batch_id: u64,
        old_owner: &Address,
        new_owner: &Address,
        error_code: u32,
    ) {
        let topics = (
            symbol_short!("recovery"),
            symbol_short!("failure"),
            batch_id,
        );
        env.events()
            .publish(topics, (old_owner.clone(), new_owner.clone(), error_code));
    }

    pub fn recovery_completed(env: &Env, batch_id: u64, successful: u32, failed: u32) {
        let topics = (
            symbol_short!("recovery"),
            symbol_short!("completed"),
            batch_id,
        );
        env.events().publish(topics, (successful, failed));
    }
}

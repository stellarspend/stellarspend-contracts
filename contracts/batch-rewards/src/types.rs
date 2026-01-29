use soroban_sdk::{contracttype, symbol_short, Address, Env, Vec};

pub const MAX_BATCH_SIZE: u32 = 100;

#[derive(Clone, Debug)]
#[contracttype]
pub struct RewardRequest {
    pub recipient: Address,
    pub amount: i128,
}

#[derive(Clone, Debug)]
#[contracttype]
pub enum RewardResult {
    Success(Address, i128),
    Failure(Address, i128, u32),
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct BatchRewardResult {
    pub total_requests: u32,
    pub successful: u32,
    pub failed: u32,
    pub total_distributed: i128,
    pub results: Vec<RewardResult>,
}

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    TotalBatches,
    TotalRewardsProcessed,
    TotalVolumeDistributed,
}

pub struct RewardEvents;

impl RewardEvents {
    pub fn batch_started(env: &Env, batch_id: u64, request_count: u32) {
        let topics = (symbol_short!("batch"), symbol_short!("started"));
        env.events().publish(topics, (batch_id, request_count));
    }

    pub fn reward_success(env: &Env, batch_id: u64, recipient: &Address, amount: i128) {
        let topics = (
            symbol_short!("reward"),
            symbol_short!("success"),
            batch_id,
        );
        env.events().publish(topics, (recipient, amount));
    }

    pub fn reward_failure(env: &Env, batch_id: u64, recipient: &Address, amount: i128, error_code: u32) {
        let topics = (
            symbol_short!("reward"),
            symbol_short!("failure"),
            batch_id,
        );
        env.events().publish(topics, (recipient, amount, error_code));
    }

    pub fn batch_completed(
        env: &Env,
        batch_id: u64,
        successful: u32,
        failed: u32,
        total_distributed: i128,
    ) {
        let topics = (symbol_short!("batch"), symbol_short!("completed"));
        env.events()
            .publish(topics, (batch_id, successful, failed, total_distributed));
    }
}

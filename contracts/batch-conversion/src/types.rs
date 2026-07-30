use soroban_sdk::{contracttype, symbol_short, Address, Env, Vec};

pub const MAX_BATCH_SIZE: u32 = 100;

#[derive(Clone, Debug)]
#[contracttype]
pub struct ConversionRequest {
    pub user: Address,
    pub from_asset: Address,
    pub to_asset: Address,
    pub amount_in: i128,      // How much user is converting
    pub min_amount_out: i128, // Minimum they expect to receive (slippage protection)
}

#[derive(Clone, Debug)]
#[contracttype]
pub enum ConversionResult {
    Success(Address, Address, Address, i128, i128),
    Failure(Address, Address, Address, i128, u32),
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct BatchConversionResult {
    pub total_requests: u32,
    pub successful: u32,
    pub failed: u32,
    pub total_converted: i128,
    pub results: Vec<ConversionResult>,
}

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    TotalBatches,
    TotalConversionsProcessed,
    TotalVolumeConverted,
    BatchOutput(u64),
}

pub struct ConversionEvents;

impl ConversionEvents {
    pub fn batch_started(env: &Env, batch_id: u64, request_count: u32) {
        let topics = (symbol_short!("batch"), symbol_short!("started"));
        env.events().publish(topics, (batch_id, request_count));
    }

    pub fn conversion_success(
        env: &Env,
        batch_id: u64,
        user: &Address,
        from_asset: &Address,
        to_asset: &Address,
        amount_in: i128,
        amount_out: i128,
    ) {
        let topics = (symbol_short!("convert"), symbol_short!("success"), batch_id);
        env.events().publish(
            topics,
            (
                user.clone(),
                from_asset.clone(),
                to_asset.clone(),
                amount_in,
                amount_out,
            ),
        );
    }

    pub fn conversion_failure(
        env: &Env,
        batch_id: u64,
        user: &Address,
        from_asset: &Address,
        to_asset: &Address,
        amount_in: i128,
        error_code: u32,
    ) {
        let topics = (symbol_short!("convert"), symbol_short!("failure"), batch_id);
        env.events().publish(
            topics,
            (
                user.clone(),
                from_asset.clone(),
                to_asset.clone(),
                amount_in,
                error_code,
            ),
        );
    }

    pub fn batch_completed(
        env: &Env,
        batch_id: u64,
        successful: u32,
        failed: u32,
        total_converted: i128,
    ) {
        let topics = (symbol_short!("batch"), symbol_short!("completed"), batch_id);
        env.events()
            .publish(topics, (successful, failed, total_converted));
    }
}

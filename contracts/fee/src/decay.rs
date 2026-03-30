use soroban_sdk::Env;

pub const DECAY_RATE: i128 = 1; // per second
pub const MIN_FEE: i128 = 10;
pub const MAX_FEE: i128 = 100;

pub fn calculate_fee_decay(
    _env: &Env,
    base_fee: i128,
    last_active: u64,
    current_time: u64,
) -> i128 {
    if current_time <= last_active {
        return base_fee;
    }

    let time_elapsed = (current_time - last_active) as i128;
    
    // decay = time_elapsed * decay_rate
    let decay = time_elapsed.checked_mul(DECAY_RATE).unwrap_or(base_fee);
    
    // Result: max(min_fee, base_fee - decay)
    let decayed_fee = base_fee.checked_sub(decay).unwrap_or(MIN_FEE);

    if decayed_fee < MIN_FEE {
        MIN_FEE
    } else {
        decayed_fee
    }
}

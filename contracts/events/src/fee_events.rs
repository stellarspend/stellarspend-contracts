use soroban_sdk::{symbol_short, Address, Env, Symbol, Vec};

/// ===============================
/// EVENT TYPES
/// ===============================
#[derive(Clone)]
pub enum FeeEventType {
    Collected,
    Withdrawn,
    Refunded,
}

impl FeeEventType {
    pub fn as_symbol(&self) -> Symbol {
        match self {
            FeeEventType::Collected => symbol_short!("FEE_COL"),
            FeeEventType::Withdrawn => symbol_short!("FEE_WDR"),
            FeeEventType::Refunded => symbol_short!("FEE_REF"),
        }
    }
}

/// ===============================
/// EVENT DATA
/// ===============================
#[derive(Clone)]
pub struct FeeEventData {
    pub user: Address,
    pub amount: i128,
    pub timestamp: u64,
}

impl FeeEventData {
    pub fn new(env: &Env, user: Address, amount: i128) -> Self {
        Self {
            user,
            amount,
            timestamp: env.ledger().timestamp(),
        }
    }
}

/// ===============================
/// CORE EMITTER
/// ===============================
pub fn emit_fee_event(env: &Env, event_type: FeeEventType, data: FeeEventData) {
    let topics: Vec<Symbol> =
        Vec::from_array(env, [symbol_short!("FEE"), event_type.as_symbol()]);

    env.events().publish(
        topics,
        (data.user, data.amount, data.timestamp),
    );
}

/// ===============================
/// HELPERS (PUBLIC API)
/// ===============================
pub fn log_fee_collected(env: &Env, user: Address, amount: i128) {
    emit_fee_event(env, FeeEventType::Collected, FeeEventData::new(env, user, amount));
}

pub fn log_fee_withdrawn(env: &Env, user: Address, amount: i128) {
    emit_fee_event(env, FeeEventType::Withdrawn, FeeEventData::new(env, user, amount));
}

pub fn log_fee_refunded(env: &Env, user: Address, amount: i128) {
    emit_fee_event(env, FeeEventType::Refunded, FeeEventData::new(env, user, amount));
}
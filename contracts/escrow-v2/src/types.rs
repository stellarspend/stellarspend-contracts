//! Data types and events for the multi-party escrow contract.

use soroban_sdk::{contracttype, symbol_short, Address, Env};

/// Basis points denominator (100% = 10_000 bps).
pub const BPS_DENOMINATOR: u32 = 10_000;

/// Lifecycle states for an escrow instance.
#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub enum EscrowState {
    /// Funds locked; awaiting release, dispute, or auto-release.
    Funded,
    /// Full amount released to seller (buyer confirmation or auto-release).
    Released,
    /// Dispute raised; awaiting arbitrator resolution.
    Disputed,
    /// Dispute resolved; funds split between buyer and seller.
    Resolved,
}

/// An escrow record.
#[derive(Clone, Debug)]
#[contracttype]
pub struct Escrow {
    pub escrow_id: u64,
    pub buyer: Address,
    pub seller: Address,
    pub arbitrator: Option<Address>,
    pub token: Address,
    pub amount: i128,
    pub state: EscrowState,
    /// Ledger timestamp when funds were deposited.
    pub funded_at: u64,
    /// Ledger timestamp after which `auto_release` is allowed (if still Funded).
    pub auto_release_at: u64,
    /// Party that raised the dispute, if any.
    pub disputed_by: Option<Address>,
    /// Buyer payout after resolve (0 until Resolved).
    pub buyer_payout: i128,
    /// Seller payout after resolve / release (0 until terminal).
    pub seller_payout: i128,
}

/// Storage keys.
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    /// Counter for escrow IDs.
    EscrowCounter,
    /// Individual escrow by ID.
    Escrow(u64),
}

/// Event emitters for escrow-v2 operations.
pub struct EscrowEvents;

impl EscrowEvents {
    pub fn funded(
        env: &Env,
        escrow_id: u64,
        buyer: &Address,
        seller: &Address,
        arbitrator: &Option<Address>,
        amount: i128,
        auto_release_at: u64,
    ) {
        let topics = (symbol_short!("escrow2"), symbol_short!("funded"));
        env.events().publish(
            topics,
            (
                escrow_id,
                buyer.clone(),
                seller.clone(),
                arbitrator.clone(),
                amount,
                auto_release_at,
            ),
        );
    }

    pub fn released(env: &Env, escrow_id: u64, seller: &Address, amount: i128) {
        let topics = (symbol_short!("escrow2"), symbol_short!("released"));
        env.events()
            .publish(topics, (escrow_id, seller.clone(), amount));
    }

    pub fn disputed(env: &Env, escrow_id: u64, raised_by: &Address) {
        let topics = (symbol_short!("escrow2"), symbol_short!("disputed"));
        env.events().publish(topics, (escrow_id, raised_by.clone()));
    }

    pub fn resolved(
        env: &Env,
        escrow_id: u64,
        buyer_payout: i128,
        seller_payout: i128,
        buyer_bps: u32,
        seller_bps: u32,
    ) {
        let topics = (symbol_short!("escrow2"), symbol_short!("resolved"));
        env.events().publish(
            topics,
            (
                escrow_id,
                buyer_payout,
                seller_payout,
                buyer_bps,
                seller_bps,
            ),
        );
    }

    pub fn auto_released(env: &Env, escrow_id: u64, seller: &Address, amount: i128) {
        let topics = (symbol_short!("escrow2"), symbol_short!("auto_rel"));
        env.events()
            .publish(topics, (escrow_id, seller.clone(), amount));
    }
}

//! # Multi-Party Escrow v2
//!
//! Escrow with buyer, seller, and optional arbitrator roles, supporting
//! release, dispute, arbitrator split resolution, and time-based auto-release.
//!
//! State machine:
//! ```text
//! Funded ──release──────► Released
//!   │
//!   ├──raise_dispute───► Disputed ──resolve_dispute──► Resolved
//!   │
//!   └──auto_release────► Released   (after window, no dispute)
//! ```
#![no_std]

mod storage;
mod types;
mod validation;

use soroban_sdk::{contract, contracterror, contractimpl, panic_with_error, token, Address, Env};

pub use crate::types::{DataKey, Escrow, EscrowEvents, EscrowState, BPS_DENOMINATOR};
use crate::validation::{
    split_amount, validate_auto_release, validate_fund, validate_raise_dispute, validate_release,
    validate_resolve_dispute, ValidationError,
};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum EscrowV2Error {
    EscrowNotFound = 1,
    InvalidAmount = 2,
    InvalidParties = 3,
    InvalidWindow = 4,
    InvalidState = 5,
    Unauthorized = 6,
    NoArbitrator = 7,
    WindowClosed = 8,
    WindowNotExpired = 9,
    InvalidSplit = 10,
}

impl From<ValidationError> for EscrowV2Error {
    fn from(e: ValidationError) -> Self {
        match e {
            ValidationError::EscrowNotFound => EscrowV2Error::EscrowNotFound,
            ValidationError::InvalidAmount => EscrowV2Error::InvalidAmount,
            ValidationError::InvalidParties => EscrowV2Error::InvalidParties,
            ValidationError::InvalidWindow => EscrowV2Error::InvalidWindow,
            ValidationError::InvalidState => EscrowV2Error::InvalidState,
            ValidationError::Unauthorized => EscrowV2Error::Unauthorized,
            ValidationError::NoArbitrator => EscrowV2Error::NoArbitrator,
            ValidationError::WindowClosed => EscrowV2Error::WindowClosed,
            ValidationError::WindowNotExpired => EscrowV2Error::WindowNotExpired,
            ValidationError::InvalidSplit => EscrowV2Error::InvalidSplit,
        }
    }
}

#[contract]
pub struct EscrowV2Contract;

#[contractimpl]
impl EscrowV2Contract {
    /// Buyer deposits funds and creates a funded escrow.
    ///
    /// # Arguments
    /// * `buyer` — depositor (must authorize)
    /// * `seller` — intended recipient
    /// * `arbitrator` — optional third party for dispute resolution
    /// * `token` — single asset for this escrow instance
    /// * `amount` — positive deposit amount
    /// * `auto_release_secs` — seconds until auto-release is allowed
    ///
    /// # Returns
    /// Newly assigned escrow id.
    pub fn fund_escrow(
        env: Env,
        buyer: Address,
        seller: Address,
        arbitrator: Option<Address>,
        token: Address,
        amount: i128,
        auto_release_secs: u64,
    ) -> u64 {
        buyer.require_auth();

        if let Err(e) = validate_fund(amount, &buyer, &seller, &arbitrator, auto_release_secs) {
            panic_with_error!(&env, EscrowV2Error::from(e));
        }

        let now = env.ledger().timestamp();
        let auto_release_at = now
            .checked_add(auto_release_secs)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowV2Error::InvalidWindow));

        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&buyer, &env.current_contract_address(), &amount);

        let escrow_id = storage::next_escrow_id(&env);
        let escrow = Escrow {
            escrow_id,
            buyer: buyer.clone(),
            seller: seller.clone(),
            arbitrator: arbitrator.clone(),
            token,
            amount,
            state: EscrowState::Funded,
            funded_at: now,
            auto_release_at,
            disputed_by: None,
            buyer_payout: 0,
            seller_payout: 0,
        };
        storage::set_escrow(&env, &escrow);

        EscrowEvents::funded(
            &env,
            escrow_id,
            &buyer,
            &seller,
            &arbitrator,
            amount,
            auto_release_at,
        );

        escrow_id
    }

    /// Buyer confirms delivery and releases the full amount to the seller.
    pub fn release(env: Env, caller: Address, escrow_id: u64) {
        caller.require_auth();

        let mut escrow = storage::get_escrow(&env, escrow_id)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowV2Error::EscrowNotFound));

        if let Err(e) = validate_release(&escrow, &caller) {
            panic_with_error!(&env, EscrowV2Error::from(e));
        }

        let token_client = token::Client::new(&env, &escrow.token);
        token_client.transfer(
            &env.current_contract_address(),
            &escrow.seller,
            &escrow.amount,
        );

        escrow.state = EscrowState::Released;
        escrow.seller_payout = escrow.amount;
        escrow.buyer_payout = 0;
        storage::set_escrow(&env, &escrow);

        EscrowEvents::released(&env, escrow_id, &escrow.seller, escrow.amount);
    }

    /// Buyer or seller raises a dispute before the auto-release window closes.
    /// Requires an arbitrator to have been set at funding time.
    pub fn raise_dispute(env: Env, caller: Address, escrow_id: u64) {
        caller.require_auth();

        let mut escrow = storage::get_escrow(&env, escrow_id)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowV2Error::EscrowNotFound));

        let now = env.ledger().timestamp();
        if let Err(e) = validate_raise_dispute(&escrow, &caller, now) {
            panic_with_error!(&env, EscrowV2Error::from(e));
        }

        escrow.state = EscrowState::Disputed;
        escrow.disputed_by = Some(caller.clone());
        storage::set_escrow(&env, &escrow);

        EscrowEvents::disputed(&env, escrow_id, &caller);
    }

    /// Arbitrator splits escrowed funds between buyer and seller.
    ///
    /// `buyer_bps + seller_bps` must equal `10_000`. The arbitrator cannot
    /// claim funds for themselves — only buyer/seller receive payouts.
    pub fn resolve_dispute(
        env: Env,
        caller: Address,
        escrow_id: u64,
        buyer_bps: u32,
        seller_bps: u32,
    ) {
        caller.require_auth();

        let mut escrow = storage::get_escrow(&env, escrow_id)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowV2Error::EscrowNotFound));

        if let Err(e) = validate_resolve_dispute(&escrow, &caller, buyer_bps, seller_bps) {
            panic_with_error!(&env, EscrowV2Error::from(e));
        }

        let (buyer_payout, seller_payout) = split_amount(escrow.amount, buyer_bps);
        let token_client = token::Client::new(&env, &escrow.token);
        let contract = env.current_contract_address();

        if buyer_payout > 0 {
            token_client.transfer(&contract, &escrow.buyer, &buyer_payout);
        }
        if seller_payout > 0 {
            token_client.transfer(&contract, &escrow.seller, &seller_payout);
        }

        escrow.state = EscrowState::Resolved;
        escrow.buyer_payout = buyer_payout;
        escrow.seller_payout = seller_payout;
        storage::set_escrow(&env, &escrow);

        EscrowEvents::resolved(
            &env,
            escrow_id,
            buyer_payout,
            seller_payout,
            buyer_bps,
            seller_bps,
        );
    }

    /// Anyone may release funds to the seller after the auto-release window
    /// expires, provided no dispute was raised.
    pub fn auto_release(env: Env, escrow_id: u64) {
        let mut escrow = storage::get_escrow(&env, escrow_id)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowV2Error::EscrowNotFound));

        let now = env.ledger().timestamp();
        if let Err(e) = validate_auto_release(&escrow, now) {
            panic_with_error!(&env, EscrowV2Error::from(e));
        }

        let token_client = token::Client::new(&env, &escrow.token);
        token_client.transfer(
            &env.current_contract_address(),
            &escrow.seller,
            &escrow.amount,
        );

        escrow.state = EscrowState::Released;
        escrow.seller_payout = escrow.amount;
        escrow.buyer_payout = 0;
        storage::set_escrow(&env, &escrow);

        EscrowEvents::auto_released(&env, escrow_id, &escrow.seller, escrow.amount);
    }

    /// Returns an escrow by id, if it exists.
    pub fn get_escrow(env: Env, escrow_id: u64) -> Option<Escrow> {
        storage::get_escrow(&env, escrow_id)
    }

    /// Returns the total number of escrows created.
    pub fn get_escrow_counter(env: Env) -> u64 {
        storage::escrow_counter(&env)
    }
}

#[cfg(test)]
mod test;

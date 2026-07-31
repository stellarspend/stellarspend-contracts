//! Validation helpers for multi-party escrow state transitions.

use crate::types::{Escrow, EscrowState, BPS_DENOMINATOR};
use soroban_sdk::Address;

/// Validation / domain errors (mapped to contract errors in `lib.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationError {
    EscrowNotFound,
    InvalidAmount,
    InvalidParties,
    InvalidWindow,
    InvalidState,
    Unauthorized,
    NoArbitrator,
    WindowClosed,
    WindowNotExpired,
    InvalidSplit,
}

/// Validates funding inputs.
pub fn validate_fund(
    amount: i128,
    buyer: &Address,
    seller: &Address,
    arbitrator: &Option<Address>,
    auto_release_secs: u64,
) -> Result<(), ValidationError> {
    if amount <= 0 {
        return Err(ValidationError::InvalidAmount);
    }
    if auto_release_secs == 0 {
        return Err(ValidationError::InvalidWindow);
    }
    if buyer == seller {
        return Err(ValidationError::InvalidParties);
    }
    if let Some(arb) = arbitrator {
        if arb == buyer || arb == seller {
            return Err(ValidationError::InvalidParties);
        }
    }
    Ok(())
}

/// Buyer may release only while Funded.
pub fn validate_release(escrow: &Escrow, caller: &Address) -> Result<(), ValidationError> {
    if escrow.state != EscrowState::Funded {
        return Err(ValidationError::InvalidState);
    }
    if caller != &escrow.buyer {
        return Err(ValidationError::Unauthorized);
    }
    Ok(())
}

/// Buyer or seller may raise a dispute while Funded and before the window closes.
/// An arbitrator must have been set at funding time.
pub fn validate_raise_dispute(
    escrow: &Escrow,
    caller: &Address,
    now: u64,
) -> Result<(), ValidationError> {
    if escrow.state != EscrowState::Funded {
        return Err(ValidationError::InvalidState);
    }
    if escrow.arbitrator.is_none() {
        return Err(ValidationError::NoArbitrator);
    }
    if now >= escrow.auto_release_at {
        return Err(ValidationError::WindowClosed);
    }
    if caller != &escrow.buyer && caller != &escrow.seller {
        return Err(ValidationError::Unauthorized);
    }
    Ok(())
}

/// Only the designated arbitrator may resolve a Disputed escrow.
/// Split ratios must sum to exactly 10_000 bps.
pub fn validate_resolve_dispute(
    escrow: &Escrow,
    caller: &Address,
    buyer_bps: u32,
    seller_bps: u32,
) -> Result<(), ValidationError> {
    if escrow.state != EscrowState::Disputed {
        return Err(ValidationError::InvalidState);
    }
    let Some(arb) = &escrow.arbitrator else {
        return Err(ValidationError::NoArbitrator);
    };
    if caller != arb {
        return Err(ValidationError::Unauthorized);
    }
    if buyer_bps
        .checked_add(seller_bps)
        .map(|sum| sum != BPS_DENOMINATOR)
        .unwrap_or(true)
    {
        return Err(ValidationError::InvalidSplit);
    }
    Ok(())
}

/// Anyone may auto-release after the window expires while still Funded.
pub fn validate_auto_release(escrow: &Escrow, now: u64) -> Result<(), ValidationError> {
    if escrow.state != EscrowState::Funded {
        return Err(ValidationError::InvalidState);
    }
    if now < escrow.auto_release_at {
        return Err(ValidationError::WindowNotExpired);
    }
    Ok(())
}

/// Splits `amount` by basis points. Seller receives the remainder to avoid dust.
pub fn split_amount(amount: i128, buyer_bps: u32) -> (i128, i128) {
    let buyer_payout =
        amount.checked_mul(buyer_bps as i128).expect("overflow") / BPS_DENOMINATOR as i128;
    let seller_payout = amount - buyer_payout;
    (buyer_payout, seller_payout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::EscrowState;
    use soroban_sdk::{testutils::Address as _, Env};

    fn sample_escrow(env: &Env, state: EscrowState) -> Escrow {
        Escrow {
            escrow_id: 1,
            buyer: Address::generate(env),
            seller: Address::generate(env),
            arbitrator: Some(Address::generate(env)),
            token: Address::generate(env),
            amount: 10_000,
            state,
            funded_at: 100,
            auto_release_at: 200,
            disputed_by: None,
            buyer_payout: 0,
            seller_payout: 0,
        }
    }

    #[test]
    fn fund_rejects_zero_amount() {
        let env = Env::default();
        let buyer = Address::generate(&env);
        let seller = Address::generate(&env);
        assert_eq!(
            validate_fund(0, &buyer, &seller, &None, 60),
            Err(ValidationError::InvalidAmount)
        );
    }

    #[test]
    fn fund_rejects_same_buyer_seller() {
        let env = Env::default();
        let buyer = Address::generate(&env);
        assert_eq!(
            validate_fund(100, &buyer, &buyer, &None, 60),
            Err(ValidationError::InvalidParties)
        );
    }

    #[test]
    fn fund_rejects_arbitrator_as_party() {
        let env = Env::default();
        let buyer = Address::generate(&env);
        let seller = Address::generate(&env);
        assert_eq!(
            validate_fund(100, &buyer, &seller, &Some(buyer.clone()), 60),
            Err(ValidationError::InvalidParties)
        );
    }

    #[test]
    fn release_requires_buyer_and_funded() {
        let env = Env::default();
        let escrow = sample_escrow(&env, EscrowState::Funded);
        assert!(validate_release(&escrow, &escrow.buyer).is_ok());
        assert_eq!(
            validate_release(&escrow, &escrow.seller),
            Err(ValidationError::Unauthorized)
        );

        let disputed = sample_escrow(&env, EscrowState::Disputed);
        assert_eq!(
            validate_release(&disputed, &disputed.buyer),
            Err(ValidationError::InvalidState)
        );
    }

    #[test]
    fn dispute_requires_window_and_arbitrator() {
        let env = Env::default();
        let mut escrow = sample_escrow(&env, EscrowState::Funded);
        assert!(validate_raise_dispute(&escrow, &escrow.buyer, 150).is_ok());
        assert_eq!(
            validate_raise_dispute(&escrow, &escrow.buyer, 200),
            Err(ValidationError::WindowClosed)
        );

        escrow.arbitrator = None;
        assert_eq!(
            validate_raise_dispute(&escrow, &escrow.buyer, 150),
            Err(ValidationError::NoArbitrator)
        );
    }

    #[test]
    fn resolve_requires_valid_split() {
        let env = Env::default();
        let escrow = sample_escrow(&env, EscrowState::Disputed);
        let arb = escrow.arbitrator.clone().unwrap();
        assert!(validate_resolve_dispute(&escrow, &arb, 3000, 7000).is_ok());
        assert_eq!(
            validate_resolve_dispute(&escrow, &arb, 3000, 6000),
            Err(ValidationError::InvalidSplit)
        );
        assert_eq!(
            validate_resolve_dispute(&escrow, &escrow.buyer, 5000, 5000),
            Err(ValidationError::Unauthorized)
        );
    }

    #[test]
    fn auto_release_requires_expiry() {
        let env = Env::default();
        let escrow = sample_escrow(&env, EscrowState::Funded);
        assert_eq!(
            validate_auto_release(&escrow, 199),
            Err(ValidationError::WindowNotExpired)
        );
        assert!(validate_auto_release(&escrow, 200).is_ok());
    }

    #[test]
    fn split_uses_remainder_for_seller() {
        assert_eq!(split_amount(100, 3333), (33, 67));
        assert_eq!(split_amount(10_000, 0), (0, 10_000));
        assert_eq!(split_amount(10_000, 10_000), (10_000, 0));
        assert_eq!(split_amount(10_000, 5000), (5_000, 5_000));
    }
}

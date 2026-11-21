#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, Address, Env,
};

#[cfg(test)]
mod test;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Typed errors for the delegation contract.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    /// The address provided is invalid (e.g. self-delegation).
    InvalidAddress = 1,
    /// Amount is zero or negative.
    InvalidAmount = 2,
    /// Caller is not authorised to perform this operation.
    Unauthorized = 3,
    /// The requested amount would exceed the remaining allowance.
    AmountTooLarge = 4,
    // [SEC-DEL-01] Explicit overflow variant instead of silent i128::MAX clamp.
    Overflow = 5,
}

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// Persisted delegation record for an (owner, delegate) pair.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct Delegation {
    /// Maximum cumulative amount the delegate is allowed to spend.
    pub limit: i128,
    /// Cumulative amount already consumed by the delegate.
    pub spent: i128,
}

/// Storage keys for the delegation contract.
#[derive(Clone)]
#[contracttype]
pub enum DelegationDataKey {
    /// Per-(owner, delegate) allowance record.
    Allowance(Address, Address),
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

/// Entry point for the delegation contract.
#[contract]
pub struct DelegationContract;

#[contractimpl]
impl DelegationContract {
    /// Authorises `delegate` to spend up to `limit` on behalf of `owner`.
    ///
    /// # Security
    /// - [SEC-DEL-02] Self-delegation is explicitly blocked: an address granting
    ///   itself a delegation would create a trivial privilege-escalation path.
    /// - [SEC-DEL-03] `limit` must be strictly positive; zero or negative limits
    ///   are rejected before any storage write occurs.
    /// - Pre-existing delegations have their `limit` updated but `spent` is
    ///   preserved, preventing a re-grant from resetting accumulated usage.
    pub fn set_delegation(env: Env, owner: Address, delegate: Address, limit: i128) {
        if let Err(e) = Self::grant(env.clone(), owner, delegate, limit) {
            panic_with_error!(&env, e);
        }
    }

    /// Authorises `delegate` to spend up to `limit` on behalf of `owner`,
    /// returning a typed error instead of panicking.
    ///
    /// # Errors
    /// - [`Error::InvalidAddress`] if `owner == delegate` ([SEC-DEL-02]).
    /// - [`Error::InvalidAmount`] if `limit <= 0` ([SEC-DEL-03]).
    pub fn grant(env: Env, owner: Address, delegate: Address, limit: i128) -> Result<(), Error> {
        owner.require_auth();

        // [SEC-DEL-02] Self-delegation guard.
        if owner == delegate {
            return Err(Error::InvalidAddress);
        }
        // [SEC-DEL-03] Positive-limit guard.
        if limit <= 0 {
            return Err(Error::InvalidAmount);
        }

        let key = DelegationDataKey::Allowance(owner.clone(), delegate.clone());
        // Preserve spent so a limit reset cannot be abused to replay already-
        // consumed allowance.
        let mut delegation: Delegation = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Delegation { limit: 0, spent: 0 });

        delegation.limit = limit;
        env.storage().persistent().set(&key, &delegation);

        env.events().publish(
            (
                soroban_sdk::symbol_short!("delegate"),
                soroban_sdk::symbol_short!("set"),
                owner.clone(),
                delegate.clone(),
            ),
            limit,
        );

        Ok(())
    }

    /// Revokes all delegation rights granted by `owner` to `delegate`.
    ///
    /// # Parameters
    /// - `env`: The Soroban environment.
    /// - `owner`: The address that granted the delegation.
    /// - `delegate`: The address whose delegation is revoked.
    ///
    /// # Errors
    /// Fails authorization if `owner` has not authorized the invocation.
    /// If no delegation exists for this pair, the call succeeds without changes.
    ///
    /// # Security
    /// - [SEC-DEL-04] Removes the key entirely rather than zeroing fields so
    ///   `consume_allowance` cannot race a zero-limit entry.
    pub fn revoke_delegation(env: Env, owner: Address, delegate: Address) {
        owner.require_auth();

        let key = DelegationDataKey::Allowance(owner.clone(), delegate.clone());
        if env.storage().persistent().has(&key) {
            env.storage().persistent().remove(&key);

            env.events().publish(
                (
                    soroban_sdk::symbol_short!("delegate"),
                    soroban_sdk::symbol_short!("revoked"),
                    owner.clone(),
                    delegate.clone(),
                ),
                (),
            );
        }
    }

    /// Records that `delegate` has consumed `amount` of their allowance.
    ///
    /// # Security
    /// - [SEC-DEL-05] `delegate.require_auth()` is the first operation so that
    ///   unauthorized callers cannot probe delegation state.
    /// - [SEC-DEL-01] `checked_add` replaces the previous `unwrap_or(i128::MAX)`
    ///   clamp; an overflow now surfaces as a typed error rather than silently
    ///   capping `new_spent` and potentially bypassing the limit comparison.
    /// - Missing delegation entry returns `Unauthorized` (not `NotFound`) to
    ///   avoid leaking information about whether a delegation exists.
    pub fn consume_allowance(
        env: Env,
        owner: Address,
        delegate: Address,
        amount: i128,
    ) -> Result<(), Error> {
        // [SEC-DEL-05] Authenticate first — no state reads before this point.
        delegate.require_auth();

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let key = DelegationDataKey::Allowance(owner.clone(), delegate.clone());

        if let Some(mut delegation) = env.storage().persistent().get::<_, Delegation>(&key) {
            // [SEC-DEL-01] Checked addition — surfaced as Overflow, not clamped.
            let new_spent = delegation
                .spent
                .checked_add(amount)
                .ok_or(Error::Overflow)?;

            if new_spent > delegation.limit {
                return Err(Error::AmountTooLarge);
            }

            delegation.spent = new_spent;
            env.storage().persistent().set(&key, &delegation);

            env.events().publish(
                (
                    soroban_sdk::symbol_short!("delegate"),
                    soroban_sdk::symbol_short!("consumed"),
                    owner.clone(),
                    delegate.clone(),
                ),
                amount,
            );

            Ok(())
        } else {
            // [SEC-DEL-05] Return Unauthorized rather than a distinct "not found"
            // error to avoid leaking delegation existence to unauthenticated callers.
            Err(Error::Unauthorized)
        }
    }

    /// Spends `amount` from the allowance that `owner` granted to `delegate`.
    ///
    /// This is the primary entry point for a delegate wishing to make a payment
    /// on behalf of an owner. It authenticates the delegate, validates the
    /// amount, and atomically deducts it from the stored allowance record.
    ///
    /// # Parameters
    ///
    /// - `env`      – The Soroban execution environment.
    /// - `owner`    – The [`Address`] of the account whose allowance is being
    ///   drawn down. Must have previously called [`set_delegation`] to grant an
    ///   allowance to `delegate`.
    /// - `delegate` – The [`Address`] of the account performing the spend.
    ///   `delegate.require_auth()` is called before any state is read, so the
    ///   transaction must carry a valid signature from this address.
    /// - `amount`   – The number of stroops (or the contract's native unit) to
    ///   consume. Must be strictly positive (`> 0`).
    ///
    /// # Errors
    ///
    /// | Variant | Condition |
    /// |---------|-----------|
    /// | [`Error::InvalidAmount`] | `amount` is zero or negative. |
    /// | [`Error::Unauthorized`]  | No active delegation from `owner` to `delegate` exists, or the delegation has been revoked. Returned instead of a "not found" variant to avoid leaking delegation state to unauthenticated callers. |
    /// | [`Error::AmountTooLarge`] | `amount` would cause cumulative `spent` to exceed `limit`. |
    /// | [`Error::Overflow`]      | Adding `amount` to the current `spent` counter would overflow `i128`. |
    ///
    /// # Security
    ///
    /// - Authentication (`delegate.require_auth()`) is the very first operation;
    ///   no storage reads occur before the caller is verified.
    /// - Arithmetic uses [`i128::checked_add`] so overflow surfaces as
    ///   [`Error::Overflow`] rather than silently wrapping or clamping.
    pub fn spend(env: Env, owner: Address, delegate: Address, amount: i128) -> Result<(), Error> {
        // Authenticate before touching any state.
        delegate.require_auth();

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let key = DelegationDataKey::Allowance(owner.clone(), delegate.clone());

        if let Some(mut delegation) = env.storage().persistent().get::<_, Delegation>(&key) {
            let new_spent = delegation
                .spent
                .checked_add(amount)
                .ok_or(Error::Overflow)?;

            if new_spent > delegation.limit {
                return Err(Error::AmountTooLarge);
            }

            delegation.spent = new_spent;
            env.storage().persistent().set(&key, &delegation);

            env.events().publish(
                (
                    soroban_sdk::symbol_short!("delegate"),
                    soroban_sdk::symbol_short!("spend"),
                    owner,
                    delegate,
                ),
                amount,
            );

            Ok(())
        } else {
            Err(Error::Unauthorized)
        }
    }

    /// Returns the current delegation state, or `None` if no delegation exists.
    pub fn get_delegation(env: Env, owner: Address, delegate: Address) -> Option<Delegation> {
        let key = DelegationDataKey::Allowance(owner, delegate);
        env.storage().persistent().get(&key)
    }

    /// Returns `true` when `delegate` has sufficient remaining allowance to cover
    /// `amount` on behalf of `owner`, without mutating any state.
    ///
    /// This is the read-only entry point called cross-contract by `batch-rewards`
    /// before distributing rewards, so it never requires auth itself.
    ///
    /// # Security
    /// - Read-only: no storage writes occur.
    /// - Returns `false` (not an error) when no delegation record exists, so
    ///   callers can treat the result as a simple boolean gate.
    pub fn check_allowance(env: Env, owner: Address, delegate: Address, amount: i128) -> bool {
        if amount <= 0 {
            return false;
        }
        let key = DelegationDataKey::Allowance(owner, delegate);
        match env.storage().persistent().get::<_, Delegation>(&key) {
            Some(d) => {
                let remaining = d.limit.saturating_sub(d.spent);
                remaining >= amount
            }
            None => false,
        }
    }
}

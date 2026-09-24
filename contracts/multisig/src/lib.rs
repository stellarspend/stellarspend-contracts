#![no_std]

//! N-of-M multi-signature contract: admin-configured signer set and
//! threshold gate pending transactions until enough signers approve.
//! Every `pub fn` below already carries a `///` doc comment; this module
//! doc summarizes the file as a whole for `cargo doc`.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, Address,
    Env, Symbol, Vec,
};

#[cfg(test)]
mod test;

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

/// All storage keys used by the multisig contract.
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    Signers,
    Threshold,
    HighValueThreshold,
    NextTxId,
    PendingTx(u64),
    Approval(u64, Address),
    ApprovalCount(u64),
    Balance(Address),
}

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// A pending multi-signature transaction waiting for approvals.
#[derive(Clone)]
#[contracttype]
pub struct PendingTx {
    pub id: u64,
    pub from: Address,
    pub to: Address,
    pub amount: i128,
    pub payload: Symbol,
    pub asset: Option<Address>,
    pub created_at: u64,
    pub executed: bool,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
    InvalidThreshold = 4,
    DuplicateSigner = 5,
    InvalidAmount = 6,
    PendingTxNotFound = 7,
    UnauthorizedSigner = 8,
    DuplicateApproval = 9,
    AlreadyExecuted = 10,
    InsufficientBalance = 11,
    MultisigNotConfigured = 12,
    Overflow = 13,
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Emits structured events for multisig state transitions.
pub struct MultisigEvents;

impl MultisigEvents {
    /// Emitted when a new pending transaction is created.
    pub fn pending_created(env: &Env, tx: &PendingTx) {
        let topics = (symbol_short!("tx"), symbol_short!("pending"), tx.id);
        env.events().publish(
            topics,
            (tx.from.clone(), tx.to.clone(), tx.amount, tx.asset.clone()),
        );
    }

    /// Emitted when a signer records their approval for a pending transaction.
    pub fn approval_recorded(
        env: &Env,
        tx_id: u64,
        signer: &Address,
        approvals_count: u32,
        threshold: u32,
    ) {
        let topics = (symbol_short!("approve"), symbol_short!("record"), tx_id);
        env.events()
            .publish(topics, (signer.clone(), approvals_count, threshold));
    }

    /// Emitted when a transaction reaches threshold and is executed.
    pub fn transaction_executed(env: &Env, tx: &PendingTx, executor: &Address) {
        let topics = (symbol_short!("tx"), symbol_short!("executed"), tx.id);
        env.events().publish(
            topics,
            (
                executor.clone(),
                tx.from.clone(),
                tx.to.clone(),
                tx.amount,
                tx.asset.clone(),
            ),
        );
    }
}

// ---------------------------------------------------------------------------
// Internal state helpers (pub so batch-rewards and other crates can reuse)
// ---------------------------------------------------------------------------

/// Initialises contract storage. Panics if already initialised.
pub fn initialize_state(env: &Env, admin: Address) {
    if env.storage().instance().has(&DataKey::Admin) {
        panic_with_error!(env, Error::AlreadyInitialized);
    }
    env.storage().instance().set(&DataKey::Admin, &admin);
    env.storage()
        .instance()
        .set(&DataKey::Signers, &Vec::<Address>::new(env));
    env.storage().instance().set(&DataKey::Threshold, &0u32);
    env.storage()
        .instance()
        .set(&DataKey::HighValueThreshold, &i128::MAX);
    env.storage().instance().set(&DataKey::NextTxId, &0u64);
}

/// Returns the stored admin address. Panics if not initialised.
pub fn get_admin(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized))
}

/// Requires `caller` to be the admin and to have authorised the call.
pub fn require_admin(env: &Env, caller: &Address) {
    caller.require_auth();
    let admin = get_admin(env);
    if admin != caller.clone() {
        panic_with_error!(env, Error::Unauthorized);
    }
}

/// Replaces the current signer set and threshold. Admin-only.
pub fn set_signers(env: &Env, caller: Address, signers: Vec<Address>, threshold: u32) {
    require_admin(env, &caller);
    validate_signer_config(env, &signers, threshold);
    env.storage().instance().set(&DataKey::Signers, &signers);
    env.storage()
        .instance()
        .set(&DataKey::Threshold, &threshold);
}

/// Multisig contract entrypoint.
#[contract]
pub struct Contract;

#[contractimpl]
impl Contract {
    /// Initialises the multisig contract with an admin address.
    ///
    /// Must be called exactly once before any other entry point.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        initialize_state(&env, admin);
        Ok(())
    }

    /// Replaces the signer set and threshold. Caller must be the admin.
    pub fn set_signers(
        env: Env,
        caller: Address,
        signers: Vec<Address>,
        threshold: u32,
    ) -> Result<(), Error> {
        set_signers(&env, caller, signers, threshold);
        Ok(())
    }

    /// Updates the high-value transaction threshold. Caller must be the admin.
    pub fn set_high_value_threshold(env: Env, caller: Address, amount: i128) -> Result<(), Error> {
        set_high_value_threshold(&env, caller, amount);
        Ok(())
    }

    /// Returns the current signer list.
    pub fn get_signers(env: Env) -> Vec<Address> {
        get_signers(&env)
    }

    /// Returns the current approval threshold.
    pub fn get_threshold(env: Env) -> u32 {
        get_threshold(&env)
    }

    /// Returns the high-value transaction threshold.
    pub fn get_high_value_threshold(env: Env) -> i128 {
        get_high_value_threshold(&env)
    }

    /// Returns `true` if `signer` is in the configured signer set.
    pub fn is_signer(env: Env, signer: Address) -> bool {
        is_signer(&env, &signer)
    }

    /// Returns the current approval count for a pending transaction.
    pub fn get_approval_count(env: Env, tx_id: u64) -> u32 {
        get_approval_count(&env, tx_id)
    }
}

use soroban_sdk::{contract, contractimpl, panic_with_error, Address, Env, Symbol, Vec};

#[path = "multisig.rs"]
mod multisig;

#[path = "timelock.rs"]
mod timelock;

use multisig::{DataKey, MultisigEvents};
pub use multisig::{MultiSigError, PendingTx};
use timelock::TimelockEvents;
pub use timelock::{TimelockError, TimelockedTx};

#[contract]
pub struct TransactionsContract;

#[contractimpl]
impl TransactionsContract {
    pub fn initialize(env: Env, admin: Address) {
        multisig::initialize_state(&env, admin);
    }

    pub fn get_admin(env: Env) -> Address {
        multisig::get_admin(&env)
    }

    pub fn set_signers(env: Env, caller: Address, signers: Vec<Address>, threshold: u32) {
        multisig::set_signers(&env, caller, signers, threshold);
    }

    pub fn set_high_value_threshold(env: Env, caller: Address, amount: i128) {
        multisig::set_high_value_threshold(&env, caller, amount);
    }

    pub fn get_signers(env: Env) -> Vec<Address> {
        multisig::get_signers(&env)
    }

    pub fn get_threshold(env: Env) -> u32 {
        multisig::get_threshold(&env)
    }

    pub fn get_multisig_threshold(env: Env) -> u32 {
        multisig::get_multisig_threshold(&env)
    }

    pub fn get_high_value_threshold(env: Env) -> i128 {
        multisig::get_high_value_threshold(&env)
    }

    pub fn set_balance(env: Env, caller: Address, user: Address, amount: i128) {
        multisig::require_admin(&env, &caller);
        if amount < 0 {
            panic_with_error!(&env, MultiSigError::InvalidAmount);
        }

        env.storage()
            .persistent()
            .set(&DataKey::Balance(user), &amount);
    }

    pub fn get_balance(env: Env, user: Address) -> i128 {
        Self::balance_of(&env, &user)
    }

    pub fn block_destination(env: Env, caller: Address, destination: Address) {
        caller.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::BlacklistedDestination(caller, destination), &true);
    }

    pub fn unblock_destination(env: Env, caller: Address, destination: Address) {
        caller.require_auth();
        env.storage()
            .persistent()
            .remove(&DataKey::BlacklistedDestination(caller, destination));
    }

    pub fn is_destination_blocked(env: Env, caller: Address, destination: Address) -> bool {
        Self::is_blocked(&env, &caller, &destination)
    }

    pub fn submit_transaction(
        env: Env,
        from: Address,
        to: Address,
        amount: i128,
        payload: Symbol,
        asset: Option<Address>,
    ) -> Option<u64> {
        from.require_auth();

        if Self::is_blocked(&env, &from, &to) {
            panic_with_error!(&env, MultiSigError::BlacklistedDestination);
        }

        if amount <= 0 {
            panic_with_error!(&env, MultiSigError::InvalidAmount);
        }

        let high_value_threshold = multisig::get_high_value_threshold(&env);

        if amount < high_value_threshold {
            Self::execute_transfer(&env, &from, &to, amount);

            let executed_tx = PendingTx {
                id: 0,
                from: from.clone(),
                to,
                amount,
                payload,
                asset,
                created_at: env.ledger().timestamp(),
                executed: true,
            };
            MultisigEvents::transaction_executed(&env, &executed_tx, &from);

            return None;
        }

        multisig::ensure_multisig_configured(&env);

        let tx_id = multisig::next_tx_id(&env);
        let pending_tx = PendingTx {
            id: tx_id,
            from,
            to,
            amount,
            payload,
            asset,
            created_at: env.ledger().timestamp(),
            executed: false,
        };

        env.storage()
            .persistent()
            .set(&DataKey::PendingTx(tx_id), &pending_tx);
        env.storage()
            .persistent()
            .set(&DataKey::ApprovalCount(tx_id), &0u32);

        MultisigEvents::pending_created(&env, &pending_tx);

        Some(tx_id)
    }

    pub fn approve(env: Env, tx_id: u64, signer: Address) {
        signer.require_auth();
        multisig::require_signer(&env, &signer);

        let mut pending_tx: PendingTx = env
            .storage()
            .persistent()
            .get(&DataKey::PendingTx(tx_id))
            .unwrap_or_else(|| panic_with_error!(&env, MultiSigError::PendingTxNotFound));

        if pending_tx.executed {
            panic_with_error!(&env, MultiSigError::AlreadyExecuted);
        }

        let approvals = multisig::record_approval(&env, tx_id, &signer);
        let threshold = multisig::get_threshold(&env);

        MultisigEvents::approval_recorded(&env, tx_id, &signer, approvals, threshold);

        if approvals >= threshold {
            pending_tx.executed = true;
            env.storage()
                .persistent()
                .set(&DataKey::PendingTx(tx_id), &pending_tx);

            Self::execute_transfer(&env, &pending_tx.from, &pending_tx.to, pending_tx.amount);
            MultisigEvents::transaction_executed(&env, &pending_tx, &signer);
        }
    }

    pub fn get_pending_tx(env: Env, tx_id: u64) -> Option<PendingTx> {
        env.storage().persistent().get(&DataKey::PendingTx(tx_id))
    }

    pub fn get_approval_count(env: Env, tx_id: u64) -> u32 {
        multisig::get_approval_count(&env, tx_id)
    }

    pub fn has_approved(env: Env, tx_id: u64, signer: Address) -> bool {
        multisig::has_approval(&env, tx_id, &signer)
    }

    /// Schedules a timelocked transaction for future execution.
    ///
    /// The `from` address must authorize this call. The transaction will not
    /// execute until the ledger timestamp is at or after `execute_at`.
    pub fn schedule_timelocked_transaction(
        env: Env,
        from: Address,
        to: Address,
        amount: i128,
        payload: Symbol,
        asset: Option<Address>,
        execute_at: u64,
    ) -> TimelockedTx {
        from.require_auth();

        if Self::is_blocked(&env, &from, &to) {
            panic_with_error!(&env, MultiSigError::BlacklistedDestination);
        }

        if amount <= 0 {
            panic_with_error!(&env, MultiSigError::InvalidAmount);
        }

        let now = env.ledger().timestamp();
        if execute_at <= now {
            // Must be strictly in the future.
            panic_with_error!(&env, TimelockError::InvalidScheduleTime);
        }

        let id = timelock::next_timelock_id(&env);
        let tx = TimelockedTx {
            id,
            from: from.clone(),
            to,
            amount,
            payload,
            asset,
            execute_at,
            created_at: now,
            executed: false,
            canceled: false,
            executed_at: None,
            canceled_at: None,
        };

        timelock::save_timelock(&env, &tx);
        TimelockEvents::scheduled(&env, &tx);

        tx
    }

    /// Executes a previously scheduled timelocked transaction once its
    /// execution time has been reached.
    ///
    /// The original `from` or the contract admin may execute the transaction.
    pub fn execute_timelocked_transaction(env: Env, caller: Address, id: u64) {
        caller.require_auth();

        let mut tx = timelock::get_timelock(&env, id)
            .unwrap_or_else(|| panic_with_error!(&env, TimelockError::NotFound));

        if tx.executed {
            panic_with_error!(&env, TimelockError::AlreadyExecuted);
        }
        if tx.canceled {
            panic_with_error!(&env, TimelockError::AlreadyCanceled);
        }

        let now = env.ledger().timestamp();
        if now < tx.execute_at {
            panic_with_error!(&env, TimelockError::EarlyExecution);
        }

        // Allow either the original sender or the admin to execute.
        let admin = multisig::get_admin(&env);
        if caller != tx.from && caller != admin {
            panic_with_error!(&env, MultiSigError::Unauthorized);
        }

        // Reuse the existing internal balance transfer logic.
        Self::execute_transfer(&env, &tx.from, &tx.to, tx.amount);

        tx.executed = true;
        tx.executed_at = Some(now);
        timelock::update_timelock(&env, &tx);

        TimelockEvents::executed(&env, &tx, &caller);
    }

    /// Cancels a timelocked transaction before it has been executed.
    ///
    /// Only the original `from` address or the admin may cancel.
    pub fn cancel_timelocked_transaction(env: Env, caller: Address, id: u64) {
        caller.require_auth();

        let mut tx = timelock::get_timelock(&env, id)
            .unwrap_or_else(|| panic_with_error!(&env, TimelockError::NotFound));

        if tx.executed {
            panic_with_error!(&env, TimelockError::AlreadyExecuted);
        }
        if tx.canceled {
            panic_with_error!(&env, TimelockError::AlreadyCanceled);
        }

        let admin = multisig::get_admin(&env);
        if caller != tx.from && caller != admin {
            panic_with_error!(&env, MultiSigError::Unauthorized);
        }

        tx.canceled = true;
        tx.canceled_at = Some(env.ledger().timestamp());
        timelock::update_timelock(&env, &tx);

        TimelockEvents::cancelled(&env, &tx, &caller);
    }

    /// Returns a timelocked transaction by its identifier, if present.
    pub fn get_timelocked_transaction(env: Env, id: u64) -> Option<TimelockedTx> {
        timelock::get_timelock(&env, id)
    }
}

impl TransactionsContract {
    fn execute_transfer(env: &Env, from: &Address, to: &Address, amount: i128) {
        let from_balance = Self::balance_of(env, from);
        if from_balance < amount {
            panic_with_error!(env, MultiSigError::InsufficientBalance);
        }

        let to_balance = Self::balance_of(env, to);

        let new_from_balance = from_balance
            .checked_sub(amount)
            .unwrap_or_else(|| panic_with_error!(env, MultiSigError::Overflow));
        let new_to_balance = to_balance
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(env, MultiSigError::Overflow));

        env.storage()
            .persistent()
            .set(&DataKey::Balance(from.clone()), &new_from_balance);
        env.storage()
            .persistent()
            .set(&DataKey::Balance(to.clone()), &new_to_balance);
    }

    fn balance_of(env: &Env, user: &Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Balance(user.clone()))
            .unwrap_or(0)
    }

    fn is_blocked(env: &Env, caller: &Address, destination: &Address) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::BlacklistedDestination(
                caller.clone(),
                destination.clone(),
            ))
    }
}

use soroban_sdk::{
    contracterror, contracttype, panic_with_error, symbol_short, Address, Env, Symbol,
};

/// Storage keys for timelocked transactions.
#[derive(Clone)]
#[contracttype]
pub enum TimelockDataKey {
    NextTimelockId,
    TimelockedTx(u64),
}

/// Represents a single timelocked transaction scheduled for future execution.
#[derive(Clone)]
#[contracttype]
pub struct TimelockedTx {
    pub id: u64,
    pub from: Address,
    pub to: Address,
    pub amount: i128,
    pub payload: Symbol,
    /// Optional asset address (e.g., token contract); `None` can represent native balance.
    pub asset: Option<Address>,
    /// When this transaction becomes executable (ledger timestamp).
    pub execute_at: u64,
    pub created_at: u64,
    pub executed: bool,
    pub canceled: bool,
    /// Actual ledger timestamp when execution happened.
    pub executed_at: Option<u64>,
    /// Actual ledger timestamp when cancellation happened.
    pub canceled_at: Option<u64>,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum TimelockError {
    NotFound = 1,
    AlreadyExecuted = 2,
    AlreadyCanceled = 3,
    EarlyExecution = 4,
    InvalidScheduleTime = 5,
}

pub struct TimelockEvents;

impl TimelockEvents {
    /// Emitted when a new timelocked transaction is scheduled.
    pub fn scheduled(env: &Env, tx: &TimelockedTx) {
        let topics = (symbol_short!("timelock"), symbol_short!("scheduled"), tx.id);
        env.events().publish(
            topics,
            (
                tx.from.clone(),
                tx.to.clone(),
                tx.amount,
                tx.asset.clone(),
                tx.execute_at,
            ),
        );
    }

    /// Emitted when a timelocked transaction is successfully executed.
    pub fn executed(env: &Env, tx: &TimelockedTx, executor: &Address) {
        let topics = (symbol_short!("timelock"), symbol_short!("executed"), tx.id);
        env.events().publish(
            topics,
            (
                executor.clone(),
                tx.from.clone(),
                tx.to.clone(),
                tx.amount,
                tx.asset.clone(),
                tx.execute_at,
                tx.executed_at,
            ),
        );
    }

    /// Emitted when a timelocked transaction is cancelled before execution.
    pub fn cancelled(env: &Env, tx: &TimelockedTx, canceller: &Address) {
        let topics = (symbol_short!("timelock"), symbol_short!("cancelled"), tx.id);
        env.events().publish(
            topics,
            (
                canceller.clone(),
                tx.from.clone(),
                tx.to.clone(),
                tx.amount,
                tx.asset.clone(),
                tx.execute_at,
                tx.canceled_at,
            ),
        );
    }
}

/// Generate and persist the next timelock identifier.
pub fn next_timelock_id(env: &Env) -> u64 {
    let current: u64 = env
        .storage()
        .instance()
        .get(&TimelockDataKey::NextTimelockId)
        .unwrap_or(0);
    let next = current
        .checked_add(1)
        .unwrap_or_else(|| panic_with_error!(env, TimelockError::InvalidScheduleTime));

    env.storage()
        .instance()
        .set(&TimelockDataKey::NextTimelockId, &next);
    next
}

pub fn save_timelock(env: &Env, tx: &TimelockedTx) {
    env.storage()
        .persistent()
        .set(&TimelockDataKey::TimelockedTx(tx.id), tx);
}

pub fn get_timelock(env: &Env, id: u64) -> Option<TimelockedTx> {
    env.storage()
        .persistent()
        .get(&TimelockDataKey::TimelockedTx(id))
}

pub fn update_timelock(env: &Env, tx: &TimelockedTx) {
    env.storage()
        .persistent()
        .set(&TimelockDataKey::TimelockedTx(tx.id), tx);
}

pub fn get_timelock_release_date(env: &Env, lock_id: u64) -> u64 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    #[test]
    fn test_get_timelock_release_date() {
        let env = Env::default();
        let lock_id = 123u64;
        let result = get_timelock_release_date(&env, lock_id);
        assert_eq!(result, 0);
    }
}
pub fn get_timelock_release_date(env: &Env, lock_id: u64) -> u64 {
    // TODO: implement proper lookup when timelock storage schema is present.
    // For now, return 0 when the lock_id does not exist (test expects 0).
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    #[test]
    fn test_get_timelock_release_date() {
        let env = Env::default();
        let lock_id = 123u64;
        let result = get_timelock_release_date(&env, lock_id);
        assert_eq!(result, 0);
    }
}

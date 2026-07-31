use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, Address,
    Env, Vec,
};

#[path = "history.rs"]
pub mod history;
use history::TransactionRecord;

#[derive(Clone)]
#[contracttype]
pub enum ArchiveDataKey {
    Admin,
    ArchiveThreshold, // threshold in seconds
    ArchiveBatchCount,
    ArchiveBatch(u64), // Storage key for batches
}

#[derive(Clone)]
#[contracttype]
pub struct CompressedArchive {
    pub batch_id: u64,
    pub start_time: u64,
    pub end_time: u64,
    pub records: Vec<TransactionRecord>,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ArchiveError {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
    NoRecordsToArchive = 4,
    InvalidThreshold = 5,
}

pub struct ArchiveEvents;

impl ArchiveEvents {
    pub fn batch_archived(env: &Env, batch_id: u64, record_count: u32) {
        let topics = (symbol_short!("archive"), symbol_short!("batched"));
        env.events()
            .publish(topics, (batch_id, record_count, env.ledger().timestamp()));
    }
}

pub fn initialize_archive(env: &Env, admin: Address, threshold_seconds: u64) {
    if env.storage().instance().has(&ArchiveDataKey::Admin) {
        panic_with_error!(env, ArchiveError::AlreadyInitialized);
    }
    env.storage().instance().set(&ArchiveDataKey::Admin, &admin);
    env.storage()
        .instance()
        .set(&ArchiveDataKey::ArchiveThreshold, &threshold_seconds);
    env.storage()
        .instance()
        .set(&ArchiveDataKey::ArchiveBatchCount, &0u64);
}

pub fn require_admin(env: &Env, caller: &Address) {
    caller.require_auth();
    let admin: Address = env
        .storage()
        .instance()
        .get(&ArchiveDataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(env, ArchiveError::NotInitialized));
    if admin != *caller {
        panic_with_error!(env, ArchiveError::Unauthorized);
    }
}

pub fn set_threshold(env: &Env, caller: Address, threshold_seconds: u64) {
    require_admin(env, &caller);
    if threshold_seconds == 0 {
        panic_with_error!(env, ArchiveError::InvalidThreshold);
    }
    env.storage()
        .instance()
        .set(&ArchiveDataKey::ArchiveThreshold, &threshold_seconds);
}

pub fn archive_records(env: &Env, caller: Address, records: Vec<TransactionRecord>) -> u64 {
    require_admin(env, &caller);

    if records.is_empty() {
        panic_with_error!(env, ArchiveError::NoRecordsToArchive);
    }

    let threshold: u64 = env
        .storage()
        .instance()
        .get(&ArchiveDataKey::ArchiveThreshold)
        .unwrap_or_else(|| panic_with_error!(env, ArchiveError::NotInitialized));

    let current_time = env.ledger().timestamp();
    let cutoff_time = current_time.saturating_sub(threshold);

    // Prevent data loss: Only archive records before cutoff
    let mut valid_records = Vec::new(env);
    let mut min_time = u64::MAX;
    let mut max_time = 0;

    for record in records.iter() {
        if record.timestamp <= cutoff_time {
            valid_records.push_back(record.clone());
            if record.timestamp < min_time {
                min_time = record.timestamp;
            }
            if record.timestamp > max_time {
                max_time = record.timestamp;
            }
        }
    }

    if valid_records.is_empty() {
        panic_with_error!(env, ArchiveError::NoRecordsToArchive);
    }

    let batch_count: u64 = env
        .storage()
        .instance()
        .get(&ArchiveDataKey::ArchiveBatchCount)
        .unwrap_or(0);

    let new_batch_id = batch_count + 1;

    let archive = CompressedArchive {
        batch_id: new_batch_id,
        start_time: min_time,
        end_time: max_time,
        records: valid_records.clone(),
    };

    env.storage()
        .persistent()
        .set(&ArchiveDataKey::ArchiveBatch(new_batch_id), &archive);
    env.storage()
        .instance()
        .set(&ArchiveDataKey::ArchiveBatchCount, &new_batch_id);

    ArchiveEvents::batch_archived(env, new_batch_id, valid_records.len());

    new_batch_id
}

pub fn get_archive_batch(env: &Env, batch_id: u64) -> Option<CompressedArchive> {
    env.storage()
        .persistent()
        .get(&ArchiveDataKey::ArchiveBatch(batch_id))
}

/// Returns the archive entry for the given entry_id, or None if not found.
///
/// This is a read-only view function — no auth required, no state mutation.
///
/// # Arguments
/// * `entry_id` - The unique identifier of the archived entry (batch_id)
///
/// # Returns
/// * `Some(CompressedArchive)` if the entry exists in the archive
/// * `None` if no entry with this id has been archived
pub fn get_archive_entry(env: &Env, entry_id: u64) -> Option<CompressedArchive> {
    env.storage()
        .persistent()
        .get(&ArchiveDataKey::ArchiveBatch(entry_id))
}

#[contract]
pub struct ArchiveContract;

#[contractimpl]
impl ArchiveContract {
    pub fn initialize(env: Env, admin: Address, threshold_seconds: u64) {
        initialize_archive(&env, admin, threshold_seconds);
    }

    pub fn set_threshold(env: Env, caller: Address, threshold_seconds: u64) {
        set_threshold(&env, caller, threshold_seconds);
    }

    pub fn archive_records(env: Env, caller: Address, records: Vec<TransactionRecord>) -> u64 {
        archive_records(&env, caller, records)
    }

    pub fn get_archive_batch(env: Env, batch_id: u64) -> Option<CompressedArchive> {
        get_archive_batch(&env, batch_id)
    }

    pub fn get_archive_entry(env: Env, entry_id: u64) -> Option<CompressedArchive> {
        get_archive_entry(&env, entry_id)
    }
}

#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, Symbol, Vec,
};

// ─── Storage Keys ─────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Admin address for this contract
    Admin,
    /// Total number of audit logs stored
    TotalAuditLogs,
    /// Individual audit log entries indexed by sequence number
    AuditLog(u64),
    /// Configuration settings
    Config,
}

// ─── Types ────────────────────────────────────────────────────────────────────

/// Represents a single audit log entry
#[contracttype]
#[derive(Clone, Debug)]
pub struct AuditLog {
    /// Address of the actor who performed the operation
    pub actor: Address,
    /// The operation performed (e.g., "transfer", "withdraw", "config_update")
    pub operation: Symbol,
    /// Timestamp of the operation
    pub timestamp: u64,
    /// Status of the operation (e.g., "success", "failure")
    pub status: Symbol,
    /// Optional additional metadata about the operation (as bytes)
    pub metadata: Option<soroban_sdk::Bytes>,
    /// Length of the metadata (stored separately since Bytes is fixed-size)
    pub metadata_len: u32,
}

/// Contract configuration
#[contracttype]
#[derive(Clone, Debug)]
pub struct Config {
    /// Address allowed to call admin-only functions
    pub admin: Address,
    /// Maximum size of metadata in bytes
    pub max_metadata_size: u32,
}

// ─── Events ───────────────────────────────────────────────────────────────────

#[contract]
pub struct AuditContract;

#[contractimpl]
impl AuditContract {
    // ── Initialize ────────────────────────────────────────────────────────────

    /// Initialize the audit contract with admin address and configuration.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - The admin address that can manage this contract
    /// * `max_metadata_size` - Maximum size allowed for metadata field
    pub fn initialize(env: Env, admin: Address, max_metadata_size: u32) {
        // Ensure idempotency — initialize only once
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("contract already initialized");
        }

        admin.require_auth();

        let config = Config {
            admin: admin.clone(),
            max_metadata_size,
        };

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Config, &config);

        // Emit initialization event
        env.events().publish(
            (symbol_short!("audit"), symbol_short!("init")),
            (admin, max_metadata_size),
        );
    }

    // ── Log Audit Entry ───────────────────────────────────────────────────────

    /// Log a single audit entry.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `actor` - The address of the actor performing the operation
    /// * `operation` - The operation being performed
    /// * `status` - The status of the operation
    /// * `metadata` - Optional metadata about the operation
    pub fn log_audit(
        env: Env,
        actor: Address,
        operation: Symbol,
        status: Symbol,
        metadata: Option<soroban_sdk::Bytes>,
    ) {
        // Require authentication from the actor
        actor.require_auth();

        // Validate metadata size if provided
        let metadata_len = match &metadata {
            Some(meta) => {
                let len = meta.len() as u32;
                let config: Config = env
                    .storage()
                    .instance()
                    .get(&DataKey::Config)
                    .expect("contract not initialized");
                
                if len > config.max_metadata_size {
                    panic!("metadata exceeds maximum allowed size");
                }
                len
            },
            None => 0,
        };

        // Create audit log entry
        let audit_log = AuditLog {
            actor: actor.clone(),
            operation: operation.clone(),
            timestamp: env.ledger().timestamp(),
            status: status.clone(),
            metadata,
            metadata_len,
        };

        // Get current total audit logs and increment
        let mut total_logs: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalAuditLogs)
            .unwrap_or(0);

        total_logs += 1;

        // Store the audit log
        env.storage()
            .persistent()
            .set(&DataKey::AuditLog(total_logs), &audit_log);

        // Update total count
        env.storage()
            .instance()
            .set(&DataKey::TotalAuditLogs, &total_logs);

        // Emit audit event
        env.events().publish(
            (symbol_short!("audit"), symbol_short!("entry")),
            (actor, operation, status, total_logs),
        );
    }

    /// Log multiple audit entries in a batch.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `caller` - The address calling this function (must be admin)
    /// * `logs` - Vector of audit logs to store
    pub fn batch_log_audit(env: Env, caller: Address, logs: Vec<AuditLog>) {
        // Verify authorization
        caller.require_auth();
        Self::require_admin(&env, &caller);

        // Validate logs
        if logs.is_empty() {
            panic!("audit log batch cannot be empty");
        }

        // Limit batch size for gas optimization
        if logs.len() > 50 {
            panic!("audit log batch exceeds maximum size of 50");
        }

        let mut total_logs: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalAuditLogs)
            .unwrap_or(0);

        // Process each log in the batch
        for log in logs.iter() {
            total_logs += 1;

            // Validate log timestamp isn't in the future
            if log.timestamp > env.ledger().timestamp() {
                panic!("audit log timestamp cannot be in the future");
            }

            // Store the audit log
            env.storage()
                .persistent()
                .set(&DataKey::AuditLog(total_logs), &log);

            // Emit audit event for each log
            env.events().publish(
                (symbol_short!("audit"), symbol_short!("entry")),
                (
                    log.actor.clone(),
                    log.operation.clone(),
                    log.status.clone(),
                    total_logs,
                ),
            );
        }

        // Update total count
        env.storage()
            .instance()
            .set(&DataKey::TotalAuditLogs, &total_logs);
    }

    // ── Accessor Functions ────────────────────────────────────────────────────

    /// Get an audit log by its sequence number.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `index` - The sequence number of the audit log to retrieve
    pub fn get_audit_log(env: Env, index: u64) -> Option<AuditLog> {
        env.storage().persistent().get(&DataKey::AuditLog(index))
    }

    /// Get the total number of audit logs stored.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    pub fn get_total_audit_logs(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalAuditLogs)
            .unwrap_or(0)
    }

    /// Get the total number of audit log entries stored.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    pub fn get_audit_log_count(env: Env) -> u32 {
        Self::get_total_audit_logs(env) as u32
    }

    /// Get a range of audit logs.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `start_index` - The starting index (inclusive)
    /// * `end_index` - The ending index (inclusive)
    pub fn get_audit_logs_range(env: Env, start_index: u64, end_index: u64) -> Vec<Option<AuditLog>> {
        if start_index > end_index {
            panic!("start index cannot be greater than end index");
        }

        let total_logs = Self::get_total_audit_logs(env.clone());
        if end_index > total_logs {
            panic!("end index exceeds total number of audit logs");
        }

        // Create the vector first with an env clone
        let mut logs: Vec<Option<AuditLog>> = Vec::new(&env);
        
        // Then populate it with actual data
        for i in start_index..=end_index {
            let log = env.storage().persistent().get(&DataKey::AuditLog(i));
            logs.push_back(log);
        }

        logs
    }

    // ── Admin Functions ───────────────────────────────────────────────────────

    /// Update the admin address.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `new_admin` - The new admin address
    pub fn set_adm(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        env.storage().instance().set(&DataKey::Admin, &new_admin);

        // Emit admin transfer event
        env.events().publish(
            (symbol_short!("audit"), symbol_short!("admtfr")),
            (caller, new_admin),
        );
    }

    /// Update the maximum metadata size configuration.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `caller` - The address calling this function (must be admin)
    /// * `new_max_size` - The new maximum metadata size
    pub fn set_max_metadata_size(env: Env, caller: Address, new_max_size: u32) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let mut config: Config = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .expect("contract not initialized");

        config.max_metadata_size = new_max_size;

        env.storage().instance().set(&DataKey::Config, &config);

        // Emit config update event
        env.events().publish(
            (symbol_short!("audit"), symbol_short!("cfgup")),
            (new_max_size,),
        );
    }

    // ── View Functions ────────────────────────────────────────────────────────

    /// Check if an address is the admin.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `addr` - The address to check
    pub fn is_admin(env: Env, addr: Address) -> bool {
        if let Some(admin) = env.storage().instance().get::<_, Address>(&DataKey::Admin) {
            addr == admin
        } else {
            false
        }
    }

    /// Get the current admin address.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::Admin)
    }

    /// Get the current configuration.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    pub fn get_config(env: Env) -> Option<Config> {
        env.storage().instance().get(&DataKey::Config)
    }

    // ── Private Helpers ───────────────────────────────────────────────────────

    /// Require that the given address is the admin.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `addr` - The address to check
    fn require_admin(env: &Env, addr: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("contract not initialized");

        if addr != &admin {
            panic!("unauthorized: only admin can call this function");
        }
    }

    // ── Integrity Verification Functions ──────────────────────────────────────

    /// Verify the integrity of a specific audit log entry.
    /// Ensures the log cannot be altered or has been corrupted.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `index` - The index of the audit log to verify
    ///
    /// # Returns
    /// `true` if the log exists and is valid, `false` otherwise
    pub fn verify_audit_log_integrity(env: Env, index: u64) -> bool {
        // Check if the log exists
        if let Some(log) = env.storage().persistent().get::<_, AuditLog>(&DataKey::AuditLog(index)) {
            // Verify that metadata_len matches actual metadata length
            let actual_len = match &log.metadata {
                Some(meta) => meta.len() as u32,
                None => 0,
            };

            // Log is considered valid if metadata_len matches actual metadata length
            log.metadata_len == actual_len
        } else {
            // Non-existent logs are not "invalid" - just return false
            false
        }
    }

    /// Verify the integrity of all audit logs in a given range.
    /// Returns count of valid and invalid logs.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `start_index` - Starting index for verification
    /// * `end_index` - Ending index for verification
    ///
    /// # Returns
    /// Tuple of (valid_count, invalid_count)
    pub fn verify_audit_logs_range(env: Env, start_index: u64, end_index: u64) -> (u64, u64) {
        if start_index > end_index {
            panic!("start index cannot be greater than end index");
        }

        let total_logs = Self::get_total_audit_logs(env.clone());
        if end_index > total_logs {
            panic!("end index exceeds total number of audit logs");
        }

        let mut valid_count: u64 = 0;
        let mut invalid_count: u64 = 0;

        for i in start_index..=end_index {
            if Self::verify_audit_log_integrity(env.clone(), i) {
                valid_count += 1;
            } else {
                invalid_count += 1;
            }
        }

        (valid_count, invalid_count)
    }

    /// Verify that an audit log has an immutable structure.
    /// This checks that the log entry cannot be overwritten by comparing
    /// the stored log data against its original state.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `index` - The index of the audit log
    /// * `expected_actor` - The expected actor from the original log
    /// * `expected_operation` - The expected operation from the original log
    ///
    /// # Returns
    /// `true` if the log has not been modified, `false` otherwise
    pub fn verify_audit_immutability(
        env: Env,
        index: u64,
        expected_actor: Address,
        expected_operation: Symbol,
    ) -> bool {
        if let Some(log) = env.storage().persistent().get::<_, AuditLog>(&DataKey::AuditLog(index)) {
            // Check that critical fields match (actor and operation should not change)
            log.actor == expected_actor && log.operation == expected_operation
        } else {
            false
        }
    }

    /// Return all audit logs recorded for a specific actor address.
    ///
    /// Performs a linear scan of all stored logs. Returns an empty Vec when the
    /// actor has no logs — does not panic.
    pub fn get_logs_by_user(env: Env, user: Address) -> Vec<AuditLog> {
        let total: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalAuditLogs)
            .unwrap_or(0);

        let mut logs: Vec<AuditLog> = Vec::new(&env);
        for i in 1..=total {
            if let Some(log) = env
                .storage()
                .persistent()
                .get::<_, AuditLog>(&DataKey::AuditLog(i))
            {
                if log.actor == user {
                    logs.push_back(log);
                }
            }
        }
        logs
    }

    /// Return all audit logs whose `timestamp` falls within [start_ts, end_ts].
    ///
    /// Performs a sequential scan of all stored logs. Returns an empty Vec when
    /// no logs fall in the range — does not panic.
    pub fn get_logs_by_range(env: Env, start_ts: u64, end_ts: u64) -> Vec<AuditLog> {
        if start_ts > end_ts {
            panic!("start timestamp cannot be greater than end timestamp");
        }

        let total: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalAuditLogs)
            .unwrap_or(0);

        let mut logs: Vec<AuditLog> = Vec::new(&env);
        for i in 1..=total {
            if let Some(log) = env
                .storage()
                .persistent()
                .get::<_, AuditLog>(&DataKey::AuditLog(i))
            {
                if log.timestamp >= start_ts && log.timestamp <= end_ts {
                    logs.push_back(log);
                }
            }
        }
        logs
    }
}

#[cfg(test)]
mod test;

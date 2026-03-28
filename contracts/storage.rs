use soroban_sdk::{contracttype, Address, Env, Vec};
use crate::DataKey;

#[derive(Clone, Debug)]
#[contracttype]
pub struct FeeConfigLog {
    pub previous_rate: u32,
    pub new_rate: u32,
    pub timestamp: u64,
    pub admin: Address,
}

pub fn write_audit_log(env: &Env, previous_rate: u32, new_rate: u32, admin: Address) {
    let key = DataKey::AuditLog;
    let mut logs: Vec<FeeConfigLog> = env.storage().persistent().get(&key).unwrap_or_else(|| Vec::new(env));
    
    let log_entry = FeeConfigLog {
        previous_rate,
        new_rate,
        timestamp: env.ledger().timestamp(),
        admin,
    };
    
    logs.push_back(log_entry);
    
    // Cap logs to the last 100 entries to prevent storage bloat
    if logs.len() > 100 {
        logs.remove(0);
    }
    
    env.storage().persistent().set(&key, &logs);
}

pub fn get_audit_log(env: &Env) -> Vec<FeeConfigLog> {
    env.storage().persistent().get(&DataKey::AuditLog).unwrap_or_else(|| Vec::new(env))
}

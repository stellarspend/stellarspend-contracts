//! Starting scaffold for the on-chain savings challenge contract (issue
//! #907 — a "susu"/"chama" style group savings mechanism). Not yet wired
//! into the workspace (no Cargo.toml yet); function signatures only.

use soroban_sdk::{contracttype, Address, Env, Vec};

#[derive(Clone)]
#[contracttype]
pub struct ChallengeConfig {
    pub participants: Vec<Address>,
    pub contribution_amount: i128,
    pub periods: u32,
    pub interval_secs: u64,
    pub penalty_bps: u32,
    pub enrollment_deadline: u64,
}

/// Organizer sets up a new challenge. Full implementation (storage,
/// validation) to follow.
pub fn create_challenge(_env: &Env, config: ChallengeConfig) -> ChallengeConfig {
    config
}

/// Participant opts in before the enrollment deadline.
pub fn enroll(_env: &Env, _participant: &Address, deadline: u64, now: u64) -> Result<(), &'static str> {
    if now > deadline {
        return Err("enrollment deadline has passed");
    }
    Ok(())
}

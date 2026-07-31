#![no_std]
// `Events::publish` is deprecated in soroban-sdk 23.x (pinned by this crate) in
// favour of `#[contractevent]`. We keep the established `publish` style used
// across this repo for these informational upgrade events.
#![allow(deprecated)]

//! Upgradeable contract with governance-gated, multisig-authorized, timelocked upgrades.
//!
//! Upgrades follow a propose -> approve -> execute lifecycle:
//!   1. A configured signer calls [`schedule_upgrade`], which records a pending
//!      upgrade, auto-approves on behalf of the proposer and sets the earliest
//!      execution time to `now + timelock_delay`.
//!   2. Other signers call [`approve_upgrade`] until the approval count reaches
//!      the configured threshold (admin multisig verification).
//!   3. Before execution, the governance contract is checked for an approved
//!      upgrade proposal. If no governance address is configured, the upgrade
//!      proceeds with only multisig + timelock (backwards compatible).
//!   4. Once the threshold is met *and* the timelock has elapsed *and* the
//!      governance proposal (if configured) is approved, any signer can call
//!      [`execute_upgrade`] to swap the contract Wasm.
//!
//! This protects upgrade permissions in three ways: unauthorized callers are
//! rejected, a threshold of approvals is enforced, a timelock delay is enforced,
//! and optionally a governance vote must pass before the upgrade can take effect.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, Address,
    BytesN, Env, Vec,
};

// Governance contract client for cross-contract invocation.
// Uses a raw soroban-sdk contractclient trait instead of a Rust dependency
// on the governance crate to avoid SDK version conflicts (upgrade contract
// uses 23.0.1 while the workspace defaults to 22.0.0).
mod governance_interface {
    use soroban_sdk::{contractclient, Address, BytesN, Env};

    #[contractclient(name = "GovernanceClient")]
    pub trait GovernanceInterface {
        /// Check whether an upgrade proposal has been approved (passed quorum
        /// + threshold, within deadline, not yet executed).
        fn is_upgrade_proposal_approved(env: Env, proposal_id: u32) -> bool;

        /// Consume (execute) an approved upgrade proposal. Returns the Wasm
        /// hash and new version that were authorized. Reverts if not approved.
        fn consume_upgrade_proposal(env: Env, caller: Address, proposal_id: u32) -> (BytesN<32>, u32);
    }
}

/// Default timelock delay applied at construction: 48 hours (in seconds).
const DEFAULT_TIMELOCK_DELAY: u64 = 48 * 60 * 60;

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Version,
    /// Addresses authorized to propose, approve and execute upgrades.
    Signers,
    /// Number of signer approvals required before an upgrade can execute.
    Threshold,
    /// Minimum number of seconds between scheduling and executing an upgrade.
    TimelockDelay,
    /// The single in-flight upgrade proposal, if any.
    Pending,
    /// Governance contract address. When set, upgrades additionally require
    /// a passed governance proposal (gating via `governance::is_upgrade_proposal_approved`).
    /// When absent, only multisig + timelock are enforced (backwards compatible).
    Governance,
    /// The governance proposal ID that authorized the current pending upgrade.
    /// Set during `schedule_upgrade` when a governance proposal ID is provided.
    GovernanceProposalId,
}

/// A pending, not-yet-executed upgrade proposal.
#[contracttype]
#[derive(Clone)]
pub struct PendingUpgrade {
    pub wasm_hash: BytesN<32>,
    pub new_version: u32,
    pub proposer: Address,
    pub scheduled_at: u64,
    /// Earliest ledger timestamp at which the upgrade may execute.
    pub execute_at: u64,
    /// Signers that have approved this proposal (includes the proposer).
    pub approvals: Vec<Address>,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum UpgradeError {
    /// Caller is not a configured signer (or admin where required).
    NotAuthorized = 1,
    /// New version must be strictly greater than the current version.
    InvalidVersion = 2,
    /// Threshold must be in `1..=signers.len()`.
    InvalidThreshold = 3,
    /// Signer set contains a duplicate address.
    DuplicateSigner = 4,
    /// No upgrade is currently pending.
    NoPendingUpgrade = 5,
    /// An upgrade is already pending; cancel it before scheduling another.
    PendingUpgradeExists = 6,
    /// Signer has already approved the pending upgrade.
    AlreadyApproved = 7,
    /// Approval count has not yet reached the threshold.
    ThresholdNotMet = 8,
    /// The timelock delay has not elapsed yet.
    TimelockNotElapsed = 9,
    /// Critical state (Admin) is missing; refuse to upgrade.
    AdminMissing = 10,
    /// Signer set must contain at least one address.
    EmptySigners = 11,
}

#[contract]
pub struct UpgradeableContract;

#[contractimpl]
impl UpgradeableContract {
    pub fn __constructor(e: Env, admin: Address) {
        e.storage().instance().set(&DataKey::Admin, &admin);
        e.storage().instance().set(&DataKey::Version, &1u32);

        // Bootstrap the multisig with the admin as the sole signer and a
        // threshold of one. The admin can widen the signer set / threshold via
        // `set_upgrade_signers` before relying on multisig protection.
        let mut signers = Vec::new(&e);
        signers.push_back(admin);
        e.storage().instance().set(&DataKey::Signers, &signers);
        e.storage().instance().set(&DataKey::Threshold, &1u32);
        e.storage()
            .instance()
            .set(&DataKey::TimelockDelay, &DEFAULT_TIMELOCK_DELAY);
    }

    pub fn version(e: Env) -> u32 {
        e.storage().instance().get(&DataKey::Version).unwrap_or(0)
    }

    // ---------------------------------------------------------------------
    // Configuration (admin only)
    // ---------------------------------------------------------------------

    /// Replace the set of upgrade signers and the approval threshold.
    ///
    /// Only the admin may call this. `threshold` must be in `1..=signers.len()`
    /// and the signer set must not contain duplicates.
    pub fn set_upgrade_signers(e: Env, signers: Vec<Address>, threshold: u32) {
        Self::require_admin(&e);
        Self::validate_signer_config(&e, &signers, threshold);

        e.storage().instance().set(&DataKey::Signers, &signers);
        e.storage().instance().set(&DataKey::Threshold, &threshold);

        e.events()
            .publish((symbol_short!("upg_cfg"),), (signers.len(), threshold));
    }

    /// Update the timelock delay (in seconds) applied to future upgrades.
    pub fn set_timelock_delay(e: Env, delay: u64) {
        Self::require_admin(&e);
        e.storage().instance().set(&DataKey::TimelockDelay, &delay);

        e.events().publish((symbol_short!("upg_tl"),), delay);
    }

    // ---------------------------------------------------------------------
    // Views
    // ---------------------------------------------------------------------

    pub fn get_signers(e: Env) -> Vec<Address> {
        e.storage()
            .instance()
            .get(&DataKey::Signers)
            .unwrap_or_else(|| Vec::new(&e))
    }

    pub fn get_threshold(e: Env) -> u32 {
        e.storage().instance().get(&DataKey::Threshold).unwrap_or(0)
    }

    pub fn get_timelock_delay(e: Env) -> u64 {
        e.storage()
            .instance()
            .get(&DataKey::TimelockDelay)
            .unwrap_or(0)
    }

    pub fn get_pending_upgrade(e: Env) -> Option<PendingUpgrade> {
        e.storage().instance().get(&DataKey::Pending)
    }

    // ---------------------------------------------------------------------
    // Governance configuration
    // ---------------------------------------------------------------------

    /// Set the governance contract address. When set, every upgrade execution
    /// will additionally require a passed governance upgrade proposal.
    /// Only the admin may call this.
    pub fn set_governance(e: Env, gov_address: Address) {
        Self::require_admin(&e);
        e.storage()
            .instance()
            .set(&DataKey::Governance, &gov_address);
        e.events()
            .publish((symbol_short!("gov_set"),), gov_address);
    }

    /// Returns the configured governance contract address, or `None` if
    /// governance gating is not enabled.
    pub fn get_governance(e: Env) -> Option<Address> {
        e.storage().instance().get(&DataKey::Governance)
    }

    /// Returns the governance proposal ID associated with the pending upgrade,
    /// or `None` if the pending upgrade was scheduled without governance.
    pub fn get_governance_proposal_id(e: Env) -> Option<u32> {
        e.storage().instance().get(&DataKey::GovernanceProposalId)
    }

    /// Number of approvals recorded on the pending upgrade (0 if none pending).
    pub fn upgrade_approval_count(e: Env) -> u32 {
        match Self::get_pending_upgrade(e) {
            Some(p) => p.approvals.len(),
            None => 0,
        }
    }

    /// Returns true when a pending upgrade has met its approval threshold and
    /// its timelock has elapsed, i.e. [`execute_upgrade`] would be allowed.
    pub fn is_upgrade_ready(e: Env) -> bool {
        let threshold = Self::get_threshold(e.clone());
        match Self::get_pending_upgrade(e.clone()) {
            Some(p) => p.approvals.len() >= threshold && e.ledger().timestamp() >= p.execute_at,
            None => false,
        }
    }

    /// Schedule an upgrade with an associated governance proposal ID.
    /// Behaves identically to [`schedule_upgrade`] but additionally stores
    /// the governance proposal ID so that `execute_upgrade` can verify it
    /// against the governance contract.
    pub fn schedule_upgrade_with_governance(
        e: Env,
        signer: Address,
        new_wasm_hash: BytesN<32>,
        new_version: u32,
        governance_proposal_id: u32,
    ) {
        signer.require_auth();
        Self::require_signer(&e, &signer);

        if e.storage().instance().has(&DataKey::Pending) {
            panic_with_error!(&e, UpgradeError::PendingUpgradeExists);
        }

        let current_version = Self::version(e.clone());
        if new_version <= current_version {
            panic_with_error!(&e, UpgradeError::InvalidVersion);
        }
        if !e.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&e, UpgradeError::AdminMissing);
        }

        let now = e.ledger().timestamp();
        let delay = Self::get_timelock_delay(e.clone());
        let execute_at = now.saturating_add(delay);

        let mut approvals = Vec::new(&e);
        approvals.push_back(signer.clone());

        let pending = PendingUpgrade {
            wasm_hash: new_wasm_hash.clone(),
            new_version,
            proposer: signer.clone(),
            scheduled_at: now,
            execute_at,
            approvals,
        };
        e.storage().instance().set(&DataKey::Pending, &pending);
        e.storage()
            .instance()
            .set(&DataKey::GovernanceProposalId, &governance_proposal_id);

        e.events().publish(
            (symbol_short!("upg_sched"), signer),
            (new_wasm_hash, new_version, execute_at, governance_proposal_id),
        );
    }

    // ---------------------------------------------------------------------
    // Upgrade lifecycle (multisig + timelock)
    // ---------------------------------------------------------------------

    /// Propose an upgrade. Only a configured signer may schedule one, and only
    /// one upgrade may be pending at a time. The proposer's approval is recorded
    /// automatically.
    pub fn schedule_upgrade(e: Env, signer: Address, new_wasm_hash: BytesN<32>, new_version: u32) {
        signer.require_auth();
        Self::require_signer(&e, &signer);

        if e.storage().instance().has(&DataKey::Pending) {
            panic_with_error!(&e, UpgradeError::PendingUpgradeExists);
        }

        // Prevent downgrades / no-op version reuse.
        let current_version = Self::version(e.clone());
        if new_version <= current_version {
            panic_with_error!(&e, UpgradeError::InvalidVersion);
        }

        // Critical state validation: refuse to schedule if Admin is missing.
        if !e.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&e, UpgradeError::AdminMissing);
        }

        let now = e.ledger().timestamp();
        let delay = Self::get_timelock_delay(e.clone());
        let execute_at = now.saturating_add(delay);

        let mut approvals = Vec::new(&e);
        approvals.push_back(signer.clone());

        let pending = PendingUpgrade {
            wasm_hash: new_wasm_hash.clone(),
            new_version,
            proposer: signer.clone(),
            scheduled_at: now,
            execute_at,
            approvals,
        };
        e.storage().instance().set(&DataKey::Pending, &pending);

        e.events().publish(
            (symbol_short!("upg_sched"), signer),
            (new_wasm_hash, new_version, execute_at),
        );
    }

    /// Approve the pending upgrade. Only a configured signer may approve, and
    /// each signer may approve at most once.
    pub fn approve_upgrade(e: Env, signer: Address) {
        signer.require_auth();
        Self::require_signer(&e, &signer);

        let mut pending: PendingUpgrade = e
            .storage()
            .instance()
            .get(&DataKey::Pending)
            .unwrap_or_else(|| panic_with_error!(&e, UpgradeError::NoPendingUpgrade));

        for approver in pending.approvals.iter() {
            if approver == signer {
                panic_with_error!(&e, UpgradeError::AlreadyApproved);
            }
        }

        pending.approvals.push_back(signer.clone());
        let count = pending.approvals.len();
        e.storage().instance().set(&DataKey::Pending, &pending);

        e.events().publish(
            (symbol_short!("upg_apprv"), signer),
            (count, Self::get_threshold(e.clone())),
        );
    }

    /// Cancel the pending upgrade. Any signer (or the admin) may cancel.
    pub fn cancel_upgrade(e: Env, signer: Address) {
        signer.require_auth();
        if !Self::is_signer(&e, &signer) && signer != Self::get_admin(&e) {
            panic_with_error!(&e, UpgradeError::NotAuthorized);
        }

        if !e.storage().instance().has(&DataKey::Pending) {
            panic_with_error!(&e, UpgradeError::NoPendingUpgrade);
        }
        e.storage().instance().remove(&DataKey::Pending);

        e.events().publish((symbol_short!("upg_cncl"), signer), ());
    }

    /// Execute the pending upgrade. Requires that a configured signer authorizes
    /// the call, the approval threshold is met, the timelock has elapsed, and
    /// (if governance is configured) a passed governance upgrade proposal exists.
    ///
    /// When governance is enabled, the governance proposal is consumed
    /// (marked executed) as part of this call to provide replay protection.
    pub fn execute_upgrade(e: Env, executor: Address) {
        executor.require_auth();
        Self::require_signer(&e, &executor);

        let pending: PendingUpgrade = e
            .storage()
            .instance()
            .get(&DataKey::Pending)
            .unwrap_or_else(|| panic_with_error!(&e, UpgradeError::NoPendingUpgrade));

        // Admin multisig verification: enough distinct signers approved.
        let threshold = Self::get_threshold(e.clone());
        if pending.approvals.len() < threshold {
            panic_with_error!(&e, UpgradeError::ThresholdNotMet);
        }

        // Timelock enforcement.
        if e.ledger().timestamp() < pending.execute_at {
            panic_with_error!(&e, UpgradeError::TimelockNotElapsed);
        }

        // Governance gating: if a governance contract is configured, verify
        // that an approved upgrade proposal exists.
        if let Some(gov_addr) = Self::get_governance(e.clone()) {
            let prop_id: u32 = e
                .storage()
                .instance()
                .get(&DataKey::GovernanceProposalId)
                .unwrap_or_else(|| panic_with_error!(&e, UpgradeError::NoPendingUpgrade));

            let gov_client = governance_interface::GovernanceClient::new(&e, &gov_addr);

            // Read-only pre-check: is the proposal approved WITHOUT consuming it.
            // This allows us to fail early before we modify state.
            if !gov_client.is_upgrade_proposal_approved(&prop_id) {
                panic_with_error!(&e, UpgradeError::ThresholdNotMet);
            }

            // Consume the governance proposal — this marks it executed
            // (replay protection). If the proposal is missing, expired, or
            // insufficiently approved this call will panic.
            let (_authorized_hash, authorized_version) =
                gov_client.consume_upgrade_proposal(&executor, &prop_id);

            // Verify the governance proposal authorized this exact version.
            if authorized_version != pending.new_version {
                panic_with_error!(&e, UpgradeError::InvalidVersion);
            }
        }

        // Re-validate version and critical state at execution time.
        let current_version = Self::version(e.clone());
        if pending.new_version <= current_version {
            panic_with_error!(&e, UpgradeError::InvalidVersion);
        }
        if !e.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&e, UpgradeError::AdminMissing);
        }

        e.storage()
            .instance()
            .set(&DataKey::Version, &pending.new_version);

        e.deployer()
            .update_current_contract_wasm(pending.wasm_hash.clone());

        e.storage().instance().remove(&DataKey::Pending);
        e.storage().instance().remove(&DataKey::GovernanceProposalId);

        e.events().publish(
            (
                symbol_short!("upgrade"),
                current_version,
                pending.new_version,
            ),
            pending.wasm_hash,
        );
    }

    // ---------------------------------------------------------------------
    // Internal helpers
    // ---------------------------------------------------------------------

    fn get_admin(e: &Env) -> Address {
        e.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, UpgradeError::AdminMissing))
    }

    fn require_admin(e: &Env) {
        let admin = Self::get_admin(e);
        admin.require_auth();
    }

    fn is_signer(e: &Env, who: &Address) -> bool {
        let signers: Vec<Address> = e
            .storage()
            .instance()
            .get(&DataKey::Signers)
            .unwrap_or_else(|| Vec::new(e));
        signers.iter().any(|s| &s == who)
    }

    fn require_signer(e: &Env, who: &Address) {
        if !Self::is_signer(e, who) {
            panic_with_error!(e, UpgradeError::NotAuthorized);
        }
    }

    fn validate_signer_config(e: &Env, signers: &Vec<Address>, threshold: u32) {
        let count = signers.len();
        if count == 0 {
            panic_with_error!(e, UpgradeError::EmptySigners);
        }
        if threshold == 0 || threshold > count {
            panic_with_error!(e, UpgradeError::InvalidThreshold);
        }
        for i in 0..count {
            let a = signers.get(i).unwrap();
            for j in (i + 1)..count {
                let b = signers.get(j).unwrap();
                if a == b {
                    panic_with_error!(e, UpgradeError::DuplicateSigner);
                }
            }
        }
    }
}

mod test;

#[cfg(test)]
mod upgrade_tests;

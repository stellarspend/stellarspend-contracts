#![no_std]

mod policy;

use policy::{find_tier_requirement, validate_tier_config, SpendingTier, TierConfig};
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, Address,
    Env, Symbol, Vec,
};

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    Signers,
    Threshold,
    NextProposalId,
    SpendingTiers,
    TierFallbackThreshold,
    Proposal(u64),
    ProposalApproval(u64, Address),
    ProposalApprovalCount(u64),
    ProposalStatus(u64),
    TotalPenalties,
    TotalFees,
    TotalRewards,
    ProposalExpiry(u64),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum ProposalStatus {
    Pending,
    Approved,
    Executed,
    Cancelled,
    Expired,
}

#[derive(Clone)]
#[contracttype]
pub struct Proposal {
    pub id: u64,
    pub recipient: Address,
    pub amount: i128,
    pub reason: Symbol,
    pub proposer: Address,
    pub created_at: u64,
    pub approval_count: u32,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum TreasuryError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidAmount = 4,
    InvalidSignerConfig = 5,
    DuplicateSigner = 6,
    ProposalNotFound = 7,
    ProposalNotPending = 8,
    DuplicateApproval = 9,
    InsufficientApprovals = 10,
    AlreadyExecuted = 11,
    ProposalExpired = 12,
    Overflow = 13,
    InsufficientTreasuryBalance = 14,
    InvalidTierConfig = 15,
    SignerNotFound = 16,
}

pub struct TreasuryEvents;

impl TreasuryEvents {
    pub fn penalty_received(env: &Env, amount: i128) {
        let topics = (symbol_short!("treasury"), symbol_short!("penalty"));
        env.events()
            .publish(topics, (amount, env.ledger().timestamp()));
    }
    pub fn fee_received(env: &Env, amount: i128) {
        let topics = (symbol_short!("treasury"), symbol_short!("fee"));
        env.events()
            .publish(topics, (amount, env.ledger().timestamp()));
    }
    pub fn reward_received(env: &Env, amount: i128) {
        let topics = (symbol_short!("treasury"), symbol_short!("reward"));
        env.events()
            .publish(topics, (amount, env.ledger().timestamp()));
    }
    pub fn signer_added(env: &Env, signer: &Address) {
        let topics = (symbol_short!("treasury"), symbol_short!("sgn_add"));
        env.events().publish(topics, signer.clone());
    }
    pub fn signer_removed(env: &Env, signer: &Address) {
        let topics = (symbol_short!("treasury"), symbol_short!("sgn_rm"));
        env.events().publish(topics, signer.clone());
    }
    pub fn threshold_changed(env: &Env, old: u32, new: u32) {
        let topics = (symbol_short!("treasury"), symbol_short!("thr_chg"));
        env.events().publish(topics, (old, new));
    }
    pub fn tiers_updated(env: &Env) {
        let topics = (symbol_short!("treasury"), symbol_short!("tiers_upd"));
        env.events().publish(topics, env.ledger().timestamp());
    }
    pub fn proposal_created(env: &Env, proposal: &Proposal) {
        let topics = (
            symbol_short!("treasury"),
            symbol_short!("proposal"),
            proposal.id,
        );
        env.events().publish(
            topics,
            (
                proposal.recipient.clone(),
                proposal.amount,
                proposal.reason.clone(),
                proposal.proposer.clone(),
            ),
        );
    }
    pub fn proposal_approved(env: &Env, pid: u64, approver: &Address, count: u32, required: u32) {
        let topics = (symbol_short!("treasury"), symbol_short!("approve"), pid);
        env.events()
            .publish(topics, (approver.clone(), count, required));
    }
    pub fn proposal_executed(
        env: &Env,
        pid: u64,
        executor: &Address,
        recipient: &Address,
        amount: i128,
    ) {
        let topics = (symbol_short!("treasury"), symbol_short!("executed"), pid);
        env.events()
            .publish(topics, (executor.clone(), recipient.clone(), amount));
    }
    pub fn proposal_cancelled(env: &Env, pid: u64, canceller: &Address) {
        let topics = (symbol_short!("treasury"), symbol_short!("cancel"), pid);
        env.events().publish(topics, canceller.clone());
    }
}

#[contract]
pub struct TreasuryContract;

#[contractimpl]
impl TreasuryContract {
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&env, TreasuryError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::Signers, &Vec::<Address>::new(&env));
        env.storage().instance().set(&DataKey::Threshold, &0u32);
        env.storage()
            .instance()
            .set(&DataKey::NextProposalId, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::TotalPenalties, &0i128);
        env.storage().instance().set(&DataKey::TotalFees, &0i128);
        env.storage().instance().set(&DataKey::TotalRewards, &0i128);
        env.storage()
            .instance()
            .set(&DataKey::SpendingTiers, &Vec::<SpendingTier>::new(&env));
        env.storage()
            .instance()
            .set(&DataKey::TierFallbackThreshold, &1u32);
    }

    pub fn credit_penalty(env: Env, amount: i128) {
        if amount <= 0 {
            panic_with_error!(&env, TreasuryError::InvalidAmount);
        }
        let mut total: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalPenalties)
            .unwrap_or(0);
        total = total
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(&env, TreasuryError::Overflow));
        env.storage()
            .instance()
            .set(&DataKey::TotalPenalties, &total);
        TreasuryEvents::penalty_received(&env, amount);
    }

    pub fn credit_fee(env: Env, amount: i128) {
        if amount <= 0 {
            panic_with_error!(&env, TreasuryError::InvalidAmount);
        }
        let mut total: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalFees)
            .unwrap_or(0);
        total = total
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(&env, TreasuryError::Overflow));
        env.storage().instance().set(&DataKey::TotalFees, &total);
        TreasuryEvents::fee_received(&env, amount);
    }

    pub fn credit_reward(env: Env, amount: i128) {
        if amount <= 0 {
            panic_with_error!(&env, TreasuryError::InvalidAmount);
        }
        let mut total: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalRewards)
            .unwrap_or(0);
        total = total
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(&env, TreasuryError::Overflow));
        env.storage().instance().set(&DataKey::TotalRewards, &total);
        TreasuryEvents::reward_received(&env, amount);
    }

    pub fn set_signers(env: Env, caller: Address, signers: Vec<Address>, threshold: u32) {
        Self::require_admin(&env, &caller);
        if signers.len() == 0 || threshold == 0 || threshold > signers.len() {
            panic_with_error!(&env, TreasuryError::InvalidSignerConfig);
        }
        for i in 0..signers.len() {
            let a = signers.get(i).unwrap();
            for j in (i + 1)..signers.len() {
                let b = signers.get(j).unwrap();
                if a == b {
                    panic_with_error!(&env, TreasuryError::DuplicateSigner);
                }
            }
        }
        let old = Self::get_threshold(env.clone());
        env.storage().instance().set(&DataKey::Signers, &signers);
        env.storage()
            .instance()
            .set(&DataKey::Threshold, &threshold);
        TreasuryEvents::threshold_changed(&env, old, threshold);
    }

    pub fn add_signer(env: Env, caller: Address, signer: Address) {
        Self::require_admin(&env, &caller);
        let mut signers: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::Signers)
            .unwrap_or_else(|| Vec::new(&env));
        for s in signers.iter() {
            if s == signer {
                panic_with_error!(&env, TreasuryError::DuplicateSigner);
            }
        }
        signers.push_back(signer.clone());
        env.storage().instance().set(&DataKey::Signers, &signers);
        TreasuryEvents::signer_added(&env, &signer);
    }

    pub fn remove_signer(env: Env, caller: Address, signer: Address) {
        Self::require_admin(&env, &caller);
        let signers: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::Signers)
            .unwrap_or_else(|| Vec::new(&env));
        let mut found = false;
        let mut new_signers: Vec<Address> = Vec::new(&env);
        for s in signers.iter() {
            if s == signer {
                found = true;
            } else {
                new_signers.push_back(s);
            }
        }
        if !found {
            panic_with_error!(&env, TreasuryError::SignerNotFound);
        }
        let threshold: u32 = env
            .storage()
            .instance()
            .get(&DataKey::Threshold)
            .unwrap_or(0);
        let new_len = new_signers.len();
        if threshold > new_len {
            let adjusted = if new_len == 0 { 1 } else { new_len };
            env.storage().instance().set(&DataKey::Threshold, &adjusted);
        }
        env.storage()
            .instance()
            .set(&DataKey::Signers, &new_signers);
        TreasuryEvents::signer_removed(&env, &signer);
    }

    pub fn set_threshold(env: Env, caller: Address, threshold: u32) {
        Self::require_admin(&env, &caller);
        let signers: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::Signers)
            .unwrap_or_else(|| Vec::new(&env));
        if threshold == 0 || threshold > signers.len() {
            panic_with_error!(&env, TreasuryError::InvalidSignerConfig);
        }
        let old: u32 = env
            .storage()
            .instance()
            .get(&DataKey::Threshold)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::Threshold, &threshold);
        TreasuryEvents::threshold_changed(&env, old, threshold);
    }

    pub fn set_spending_tiers(
        env: Env,
        caller: Address,
        tiers: Vec<SpendingTier>,
        fallback_threshold: u32,
    ) {
        Self::require_admin(&env, &caller);
        let signers: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::Signers)
            .unwrap_or_else(|| Vec::new(&env));
        let total_signers = if signers.len() == 0 { 1 } else { signers.len() };
        let config = TierConfig {
            tiers: tiers.clone(),
            fallback_threshold,
        };
        validate_tier_config(&config, total_signers);
        env.storage()
            .instance()
            .set(&DataKey::SpendingTiers, &tiers);
        env.storage()
            .instance()
            .set(&DataKey::TierFallbackThreshold, &fallback_threshold);
        TreasuryEvents::tiers_updated(&env);
    }

    pub fn propose_disbursement(
        env: Env,
        caller: Address,
        recipient: Address,
        amount: i128,
        reason: Symbol,
    ) -> u64 {
        caller.require_auth();
        if amount <= 0 {
            panic_with_error!(&env, TreasuryError::InvalidAmount);
        }
        Self::require_initialized(&env);
        if !Self::is_signer(env.clone(), caller.clone()) {
            panic_with_error!(&env, TreasuryError::Unauthorized);
        }
        let reserve = Self::get_total_reserve(env.clone());
        if amount > reserve {
            panic_with_error!(&env, TreasuryError::InsufficientTreasuryBalance);
        }
        let pid = Self::next_proposal_id(&env);
        let now = env.ledger().timestamp();
        let proposal = Proposal {
            id: pid,
            recipient,
            amount,
            reason,
            proposer: caller.clone(),
            created_at: now,
            approval_count: 0,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Proposal(pid), &proposal);
        env.storage()
            .persistent()
            .set(&DataKey::ProposalStatus(pid), &ProposalStatus::Pending);
        let default_expiry: u64 = 7 * 24 * 60 * 60;
        env.storage()
            .persistent()
            .set(&DataKey::ProposalExpiry(pid), &(now + default_expiry));
        env.storage()
            .persistent()
            .set(&DataKey::ProposalApprovalCount(pid), &0u32);
        TreasuryEvents::proposal_created(&env, &proposal);
        pid
    }

    pub fn approve_disbursement(env: Env, caller: Address, proposal_id: u64) {
        Self::require_initialized(&env);
        Self::require_signer(&env, &caller);
        Self::require_proposal_pending(&env, proposal_id);
        Self::check_proposal_expiry(&env, proposal_id);
        if env
            .storage()
            .persistent()
            .has(&DataKey::ProposalApproval(proposal_id, caller.clone()))
        {
            panic_with_error!(&env, TreasuryError::DuplicateApproval);
        }
        env.storage().persistent().set(
            &DataKey::ProposalApproval(proposal_id, caller.clone()),
            &true,
        );
        let mut proposal: Proposal = env
            .storage()
            .persistent()
            .get(&DataKey::Proposal(proposal_id))
            .unwrap();
        proposal.approval_count = proposal
            .approval_count
            .checked_add(1)
            .unwrap_or_else(|| panic_with_error!(&env, TreasuryError::Overflow));
        let required = self::get_required_signers(&env, proposal.amount);
        env.storage().persistent().set(
            &DataKey::ProposalApprovalCount(proposal_id),
            &proposal.approval_count,
        );
        env.storage()
            .persistent()
            .set(&DataKey::Proposal(proposal_id), &proposal);
        TreasuryEvents::proposal_approved(
            &env,
            proposal_id,
            &caller,
            proposal.approval_count,
            required,
        );
        if proposal.approval_count >= required {
            env.storage().persistent().set(
                &DataKey::ProposalStatus(proposal_id),
                &ProposalStatus::Approved,
            );
        }
    }

    pub fn execute_disbursement(env: Env, caller: Address, proposal_id: u64) {
        caller.require_auth();
        Self::require_initialized(&env);
        let status: ProposalStatus = env
            .storage()
            .persistent()
            .get(&DataKey::ProposalStatus(proposal_id))
            .unwrap_or_else(|| panic_with_error!(&env, TreasuryError::ProposalNotFound));
        if status == ProposalStatus::Executed {
            panic_with_error!(&env, TreasuryError::AlreadyExecuted);
        }
        if status == ProposalStatus::Cancelled {
            panic_with_error!(&env, TreasuryError::ProposalNotPending);
        }
        if status == ProposalStatus::Expired {
            panic_with_error!(&env, TreasuryError::ProposalExpired);
        }
        Self::check_proposal_expiry(&env, proposal_id);
        let proposal: Proposal = env
            .storage()
            .persistent()
            .get(&DataKey::Proposal(proposal_id))
            .unwrap_or_else(|| panic_with_error!(&env, TreasuryError::ProposalNotFound));
        let required = self::get_required_signers(&env, proposal.amount);
        if proposal.approval_count < required {
            panic_with_error!(&env, TreasuryError::InsufficientApprovals);
        }
        let reserve = Self::get_total_reserve(env.clone());
        if proposal.amount > reserve {
            panic_with_error!(&env, TreasuryError::InsufficientTreasuryBalance);
        }
        env.storage().persistent().set(
            &DataKey::ProposalStatus(proposal_id),
            &ProposalStatus::Executed,
        );
        TreasuryEvents::proposal_executed(
            &env,
            proposal_id,
            &caller,
            &proposal.recipient,
            proposal.amount,
        );
    }

    pub fn cancel_proposal(env: Env, caller: Address, proposal_id: u64) {
        Self::require_admin(&env, &caller);
        let status: ProposalStatus = env
            .storage()
            .persistent()
            .get(&DataKey::ProposalStatus(proposal_id))
            .unwrap_or_else(|| panic_with_error!(&env, TreasuryError::ProposalNotFound));
        if status == ProposalStatus::Executed {
            panic_with_error!(&env, TreasuryError::AlreadyExecuted);
        }
        if status == ProposalStatus::Cancelled {
            panic_with_error!(&env, TreasuryError::ProposalNotPending);
        }
        env.storage().persistent().set(
            &DataKey::ProposalStatus(proposal_id),
            &ProposalStatus::Cancelled,
        );
        TreasuryEvents::proposal_cancelled(&env, proposal_id, &caller);
    }

    pub fn get_proposal(env: Env, proposal_id: u64) -> Option<Proposal> {
        env.storage()
            .persistent()
            .get(&DataKey::Proposal(proposal_id))
    }

    pub fn get_proposal_status(env: Env, proposal_id: u64) -> Option<ProposalStatus> {
        env.storage()
            .persistent()
            .get(&DataKey::ProposalStatus(proposal_id))
    }

    pub fn get_proposal_approval_count(env: Env, proposal_id: u64) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::ProposalApprovalCount(proposal_id))
            .unwrap_or(0)
    }

    pub fn has_approved(env: Env, proposal_id: u64, signer: Address) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::ProposalApproval(proposal_id, signer))
    }

    pub fn get_total_penalties(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalPenalties)
            .unwrap_or(0)
    }

    pub fn get_total_fees(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalFees)
            .unwrap_or(0)
    }

    pub fn get_total_rewards(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalRewards)
            .unwrap_or(0)
    }

    pub fn get_total_reserve(env: Env) -> i128 {
        let p: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalPenalties)
            .unwrap_or(0);
        let f: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalFees)
            .unwrap_or(0);
        let r: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalRewards)
            .unwrap_or(0);
        p.checked_add(f).unwrap_or(0).checked_add(r).unwrap_or(0)
    }

    pub fn get_signers(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::Signers)
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn get_threshold(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::Threshold)
            .unwrap_or(0)
    }

    pub fn get_required_signers_for_amount(env: Env, amount: i128) -> u32 {
        self::get_required_signers(&env, amount)
    }

    pub fn is_signer(env: Env, address: Address) -> bool {
        let signers: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::Signers)
            .unwrap_or_else(|| Vec::new(&env));
        for s in signers.iter() {
            if s == address {
                return true;
            }
        }
        false
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, TreasuryError::NotInitialized))
    }

    pub fn get_spending_tiers(env: Env) -> Vec<SpendingTier> {
        env.storage()
            .instance()
            .get(&DataKey::SpendingTiers)
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn get_fallback_threshold(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::TierFallbackThreshold)
            .unwrap_or(1)
    }

    fn require_admin(env: &Env, caller: &Address) {
        caller.require_auth();
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(env, TreasuryError::NotInitialized));
        if *caller != admin {
            panic_with_error!(env, TreasuryError::Unauthorized);
        }
    }

    fn require_initialized(env: &Env) {
        if !env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(env, TreasuryError::NotInitialized);
        }
    }

    fn require_signer(env: &Env, address: &Address) {
        let signers: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::Signers)
            .unwrap_or_else(|| Vec::new(env));
        for s in signers.iter() {
            if s == address.clone() {
                return;
            }
        }
        panic_with_error!(env, TreasuryError::Unauthorized);
    }

    fn require_proposal_pending(env: &Env, proposal_id: u64) {
        let status: ProposalStatus = env
            .storage()
            .persistent()
            .get(&DataKey::ProposalStatus(proposal_id))
            .unwrap_or_else(|| panic_with_error!(env, TreasuryError::ProposalNotFound));
        if status != ProposalStatus::Pending {
            panic_with_error!(env, TreasuryError::ProposalNotPending);
        }
    }

    fn check_proposal_expiry(env: &Env, proposal_id: u64) {
        let expiry: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::ProposalExpiry(proposal_id))
            .unwrap_or(u64::MAX);
        if env.ledger().timestamp() > expiry {
            env.storage().persistent().set(
                &DataKey::ProposalStatus(proposal_id),
                &ProposalStatus::Expired,
            );
            panic_with_error!(env, TreasuryError::ProposalExpired);
        }
    }

    fn next_proposal_id(env: &Env) -> u64 {
        let current: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextProposalId)
            .unwrap_or(0);
        let next = current
            .checked_add(1)
            .unwrap_or_else(|| panic_with_error!(env, TreasuryError::Overflow));
        env.storage()
            .instance()
            .set(&DataKey::NextProposalId, &next);
        next
    }
}

fn get_required_signers(env: &Env, amount: i128) -> u32 {
    let tiers: Vec<SpendingTier> = env
        .storage()
        .instance()
        .get(&DataKey::SpendingTiers)
        .unwrap_or_else(|| Vec::new(env));
    let fallback: u32 = env
        .storage()
        .instance()
        .get(&DataKey::TierFallbackThreshold)
        .unwrap_or(1);
    let total_signers: u32 = {
        let signers: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::Signers)
            .unwrap_or_else(|| Vec::new(env));
        signers.len()
    };
    let threshold: u32 = env
        .storage()
        .instance()
        .get(&DataKey::Threshold)
        .unwrap_or(1);
    let required = if tiers.len() > 0 {
        let config = TierConfig {
            tiers,
            fallback_threshold: fallback,
        };
        let tier_req = find_tier_requirement(&config, amount);
        if tier_req > 0 {
            tier_req
        } else {
            if threshold > 0 {
                threshold
            } else {
                1
            }
        }
    } else {
        if threshold > 0 {
            threshold
        } else {
            1
        }
    };
    if total_signers > 0 && required > total_signers {
        total_signers
    } else if required == 0 {
        1
    } else {
        required
    }
}

#[cfg(test)]
mod test;

#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, Address,
    BytesN, Env, String,
};

#[derive(Clone)]
#[contracttype]
pub enum GovernanceDataKey {
    Admin,
    RequiredApprovals,
    ProposalCount,
    Proposal(u32),
    UpgradeProposal(u32),
    UserVote(u32, Address),
    ConfigValue(String),
}

#[derive(Clone)]
#[contracttype]
pub struct Proposal {
    pub id: u32,
    pub proposer: Address,
    pub config_key: String,
    pub config_value: String,
    pub approvals: u32,
    pub executed: bool,
    pub deadline: u64,
}

/// A proposal specifically for authorizing a contract upgrade.
/// Carries the new Wasm hash and version so the governance vote gates
/// the exact upgrade parameters.
#[derive(Clone)]
#[contracttype]
pub struct UpgradeProposal {
    pub id: u32,
    pub proposer: Address,
    pub wasm_hash: BytesN<32>,
    pub new_version: u32,
    pub approvals: u32,
    pub executed: bool,
    pub deadline: u64,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum GovernanceError {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
    ProposalNotFound = 4,
    AlreadyVoted = 5,
    ProposalExpired = 6,
    AlreadyExecuted = 7,
    NotEnoughApprovals = 8,
    Overflow = 9,
    InvalidInput = 10,
}

pub struct GovernanceEvents;

impl GovernanceEvents {
    pub fn admin_updated(env: &Env, previous_admin: &Address, new_admin: &Address) {
        let topics = (symbol_short!("gov"), symbol_short!("admin"));
        env.events().publish(
            topics,
            (
                previous_admin.clone(),
                new_admin.clone(),
                env.ledger().timestamp(),
            ),
        );
    }

    pub fn proposal_created(
        env: &Env,
        id: u32,
        proposer: &Address,
        config_key: &String,
        config_value: &String,
    ) {
        let topics = (symbol_short!("gov"), symbol_short!("created"));
        env.events().publish(
            topics,
            (
                id,
                proposer.clone(),
                config_key.clone(),
                config_value.clone(),
                env.ledger().timestamp(),
            ),
        );
    }

    pub fn upgrade_proposal_created(
        env: &Env,
        id: u32,
        proposer: &Address,
        wasm_hash: &BytesN<32>,
        new_version: u32,
    ) {
        let topics = (symbol_short!("gov"), symbol_short!("upg_created"));
        env.events().publish(
            topics,
            (
                id,
                proposer.clone(),
                wasm_hash.clone(),
                new_version,
                env.ledger().timestamp(),
            ),
        );
    }

    pub fn voted(env: &Env, id: u32, voter: &Address) {
        let topics = (symbol_short!("gov"), symbol_short!("voted"));
        env.events()
            .publish(topics, (id, voter.clone(), env.ledger().timestamp()));
    }

    pub fn proposal_executed(env: &Env, id: u32, config_key: &String, config_value: &String) {
        let topics = (symbol_short!("gov"), symbol_short!("executed"));
        env.events().publish(
            topics,
            (
                id,
                config_key.clone(),
                config_value.clone(),
                env.ledger().timestamp(),
            ),
        );
    }

    pub fn upgrade_executed(env: &Env, id: u32, wasm_hash: &BytesN<32>, new_version: u32) {
        let topics = (symbol_short!("gov"), symbol_short!("upg_executed"));
        env.events().publish(
            topics,
            (id, wasm_hash.clone(), new_version, env.ledger().timestamp()),
        );
    }
}

pub fn initialize_governance(env: &Env, admin: Address, required_approvals: u32) {
    if env.storage().instance().has(&GovernanceDataKey::Admin) {
        panic_with_error!(env, GovernanceError::AlreadyInitialized);
    }
    env.storage()
        .instance()
        .set(&GovernanceDataKey::Admin, &admin);
    env.storage()
        .instance()
        .set(&GovernanceDataKey::RequiredApprovals, &required_approvals);
    env.storage()
        .instance()
        .set(&GovernanceDataKey::ProposalCount, &0u32);
}

pub fn require_admin(env: &Env, caller: &Address) {
    caller.require_auth();
    let admin: Address = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(env, GovernanceError::NotInitialized));
    if admin != *caller {
        panic_with_error!(env, GovernanceError::Unauthorized);
    }
}

pub fn update_admin(env: &Env, current_admin: Address, new_admin: Address) {
    require_admin(env, &current_admin);
    env.storage()
        .instance()
        .set(&GovernanceDataKey::Admin, &new_admin);
    GovernanceEvents::admin_updated(env, &current_admin, &new_admin);
}

const MAX_CONFIG_STRING_LENGTH: u32 = 256;

pub fn create_proposal(
    env: &Env,
    proposer: Address,
    config_key: String,
    config_value: String,
    duration_seconds: u64,
) -> u32 {
    proposer.require_auth();

    if config_key.len() > MAX_CONFIG_STRING_LENGTH || config_key.len() == 0 {
        panic_with_error!(env, GovernanceError::InvalidInput);
    }
    if config_value.len() > MAX_CONFIG_STRING_LENGTH {
        panic_with_error!(env, GovernanceError::InvalidInput);
    }
    if duration_seconds == 0 {
        panic_with_error!(env, GovernanceError::InvalidInput);
    }

    let count: u32 = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::ProposalCount)
        .unwrap_or_else(|| panic_with_error!(env, GovernanceError::NotInitialized));

    let new_id = count
        .checked_add(1)
        .unwrap_or_else(|| panic_with_error!(env, GovernanceError::Overflow));
    let current_time = env.ledger().timestamp();
    let deadline = current_time
        .checked_add(duration_seconds)
        .unwrap_or_else(|| panic_with_error!(env, GovernanceError::Overflow));

    let proposal = Proposal {
        id: new_id,
        proposer: proposer.clone(),
        config_key: config_key.clone(),
        config_value: config_value.clone(),
        approvals: 0,
        executed: false,
        deadline,
    };

    env.storage()
        .persistent()
        .set(&GovernanceDataKey::Proposal(new_id), &proposal);
    env.storage()
        .instance()
        .set(&GovernanceDataKey::ProposalCount, &new_id);

    GovernanceEvents::proposal_created(env, new_id, &proposer, &config_key, &config_value);

    new_id
}

/// Create an upgrade proposal specifically for gating a contract upgrade.
/// The proposal stores the target Wasm hash and new version number.
pub fn create_upgrade_proposal(
    env: &Env,
    proposer: Address,
    wasm_hash: BytesN<32>,
    new_version: u32,
    duration_seconds: u64,
) -> u32 {
    proposer.require_auth();

    if new_version == 0 {
        panic_with_error!(env, GovernanceError::InvalidInput);
    }
    if duration_seconds == 0 {
        panic_with_error!(env, GovernanceError::InvalidInput);
    }

    let count: u32 = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::ProposalCount)
        .unwrap_or_else(|| panic_with_error!(env, GovernanceError::NotInitialized));

    let new_id = count
        .checked_add(1)
        .unwrap_or_else(|| panic_with_error!(env, GovernanceError::Overflow));
    let current_time = env.ledger().timestamp();
    let deadline = current_time
        .checked_add(duration_seconds)
        .unwrap_or_else(|| panic_with_error!(env, GovernanceError::Overflow));

    let upgrade_proposal = UpgradeProposal {
        id: new_id,
        proposer: proposer.clone(),
        wasm_hash: wasm_hash.clone(),
        new_version,
        approvals: 0,
        executed: false,
        deadline,
    };

    env.storage().persistent().set(
        &GovernanceDataKey::UpgradeProposal(new_id),
        &upgrade_proposal,
    );
    env.storage()
        .instance()
        .set(&GovernanceDataKey::ProposalCount, &new_id);

    GovernanceEvents::upgrade_proposal_created(env, new_id, &proposer, &wasm_hash, new_version);

    new_id
}

/// Vote on any proposal type (config or upgrade).
/// Uses a shared UserVote key scoped by proposal ID to prevent double voting.
pub fn vote_proposal(env: &Env, voter: Address, proposal_id: u32) {
    voter.require_auth();

    // Check if it's an upgrade proposal first
    if let Some(mut up) = env
        .storage()
        .persistent()
        .get::<_, UpgradeProposal>(&GovernanceDataKey::UpgradeProposal(proposal_id))
    {
        if up.executed {
            panic_with_error!(env, GovernanceError::AlreadyExecuted);
        }
        if env.ledger().timestamp() > up.deadline {
            panic_with_error!(env, GovernanceError::ProposalExpired);
        }

        let vote_key = GovernanceDataKey::UserVote(proposal_id, voter.clone());
        let has_voted: bool = env.storage().persistent().get(&vote_key).unwrap_or(false);
        if has_voted {
            panic_with_error!(env, GovernanceError::AlreadyVoted);
        }

        env.storage().persistent().set(&vote_key, &true);
        up.approvals = up
            .approvals
            .checked_add(1)
            .unwrap_or_else(|| panic_with_error!(env, GovernanceError::Overflow));
        env.storage()
            .persistent()
            .set(&GovernanceDataKey::UpgradeProposal(proposal_id), &up);

        GovernanceEvents::voted(env, proposal_id, &voter);
        return;
    }

    // Otherwise treat as a config proposal
    let mut proposal: Proposal = env
        .storage()
        .persistent()
        .get(&GovernanceDataKey::Proposal(proposal_id))
        .unwrap_or_else(|| panic_with_error!(env, GovernanceError::ProposalNotFound));

    if proposal.executed {
        panic_with_error!(env, GovernanceError::AlreadyExecuted);
    }
    if env.ledger().timestamp() > proposal.deadline {
        panic_with_error!(env, GovernanceError::ProposalExpired);
    }

    let vote_key = GovernanceDataKey::UserVote(proposal_id, voter.clone());
    let has_voted: bool = env.storage().persistent().get(&vote_key).unwrap_or(false);
    if has_voted {
        panic_with_error!(env, GovernanceError::AlreadyVoted);
    }

    env.storage().persistent().set(&vote_key, &true);
    proposal.approvals = proposal
        .approvals
        .checked_add(1)
        .unwrap_or_else(|| panic_with_error!(env, GovernanceError::Overflow));
    env.storage()
        .persistent()
        .set(&GovernanceDataKey::Proposal(proposal_id), &proposal);

    GovernanceEvents::voted(env, proposal_id, &voter);
}

/// Execute a config proposal (applies the config change).
pub fn execute_proposal(env: &Env, caller: Address, proposal_id: u32) {
    caller.require_auth();

    let mut proposal: Proposal = env
        .storage()
        .persistent()
        .get(&GovernanceDataKey::Proposal(proposal_id))
        .unwrap_or_else(|| panic_with_error!(env, GovernanceError::ProposalNotFound));

    if proposal.executed {
        panic_with_error!(env, GovernanceError::AlreadyExecuted);
    }
    if env.ledger().timestamp() > proposal.deadline {
        panic_with_error!(env, GovernanceError::ProposalExpired);
    }

    let required_approvals: u32 = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::RequiredApprovals)
        .unwrap_or_else(|| panic_with_error!(env, GovernanceError::NotInitialized));

    if proposal.approvals < required_approvals {
        panic_with_error!(env, GovernanceError::NotEnoughApprovals);
    }

    env.storage().persistent().set(
        &GovernanceDataKey::ConfigValue(proposal.config_key.clone()),
        &proposal.config_value,
    );

    proposal.executed = true;
    env.storage()
        .persistent()
        .set(&GovernanceDataKey::Proposal(proposal_id), &proposal);

    GovernanceEvents::proposal_executed(
        env,
        proposal_id,
        &proposal.config_key,
        &proposal.config_value,
    );
}

/// Mark an upgrade proposal as executed (called by the upgrade contract after
/// a successful upgrade). Verifies the proposal has enough approvals and is
/// within its deadline, then marks it executed.
///
/// Returns the Wasm hash and version that were authorized so the caller can
/// perform the actual upgrade.
pub fn consume_upgrade_proposal(env: &Env, caller: Address, proposal_id: u32) -> (BytesN<32>, u32) {
    caller.require_auth();

    let mut up: UpgradeProposal = env
        .storage()
        .persistent()
        .get(&GovernanceDataKey::UpgradeProposal(proposal_id))
        .unwrap_or_else(|| panic_with_error!(env, GovernanceError::ProposalNotFound));

    if up.executed {
        panic_with_error!(env, GovernanceError::AlreadyExecuted);
    }
    if env.ledger().timestamp() > up.deadline {
        panic_with_error!(env, GovernanceError::ProposalExpired);
    }

    let required_approvals: u32 = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::RequiredApprovals)
        .unwrap_or_else(|| panic_with_error!(env, GovernanceError::NotInitialized));

    if up.approvals < required_approvals {
        panic_with_error!(env, GovernanceError::NotEnoughApprovals);
    }

    up.executed = true;
    let result = (up.wasm_hash.clone(), up.new_version);
    env.storage()
        .persistent()
        .set(&GovernanceDataKey::UpgradeProposal(proposal_id), &up);

    GovernanceEvents::upgrade_executed(env, proposal_id, &up.wasm_hash, up.new_version);

    result
}

/// Check whether an upgrade proposal exists, has been passed (enough approvals
/// and within deadline), and has NOT yet been executed. This is a read-only
/// view used by the upgrade contract to validate preconditions without
/// consuming the proposal.
pub fn is_upgrade_proposal_approved(env: &Env, proposal_id: u32) -> bool {
    let up: UpgradeProposal = match env
        .storage()
        .persistent()
        .get(&GovernanceDataKey::UpgradeProposal(proposal_id))
    {
        Some(p) => p,
        None => return false,
    };

    if up.executed {
        return false;
    }
    if env.ledger().timestamp() > up.deadline {
        return false;
    }

    let required_approvals: u32 = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::RequiredApprovals)
        .unwrap_or(0);

    up.approvals >= required_approvals
}

#[contract]
pub struct GovernanceContract;

#[contractimpl]
impl GovernanceContract {
    pub fn initialize(env: Env, admin: Address, required_approvals: u32) {
        initialize_governance(&env, admin, required_approvals);
    }

    pub fn update_admin(env: Env, current_admin: Address, new_admin: Address) {
        update_admin(&env, current_admin, new_admin);
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&GovernanceDataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, GovernanceError::NotInitialized))
    }

    pub fn create_proposal(
        env: Env,
        proposer: Address,
        config_key: String,
        config_value: String,
        duration_seconds: u64,
    ) -> u32 {
        create_proposal(&env, proposer, config_key, config_value, duration_seconds)
    }

    pub fn create_upgrade_proposal(
        env: Env,
        proposer: Address,
        wasm_hash: BytesN<32>,
        new_version: u32,
        duration_seconds: u64,
    ) -> u32 {
        create_upgrade_proposal(&env, proposer, wasm_hash, new_version, duration_seconds)
    }

    pub fn vote_proposal(env: Env, voter: Address, proposal_id: u32) {
        vote_proposal(&env, voter, proposal_id);
    }

    pub fn execute_proposal(env: Env, caller: Address, proposal_id: u32) {
        execute_proposal(&env, caller, proposal_id);
    }

    pub fn consume_upgrade_proposal(
        env: Env,
        caller: Address,
        proposal_id: u32,
    ) -> (BytesN<32>, u32) {
        consume_upgrade_proposal(&env, caller, proposal_id)
    }

    pub fn is_upgrade_proposal_approved(env: Env, proposal_id: u32) -> bool {
        is_upgrade_proposal_approved(&env, proposal_id)
    }

    pub fn get_proposal(env: Env, proposal_id: u32) -> Option<Proposal> {
        env.storage()
            .persistent()
            .get(&GovernanceDataKey::Proposal(proposal_id))
    }

    pub fn get_upgrade_proposal(env: Env, proposal_id: u32) -> Option<UpgradeProposal> {
        env.storage()
            .persistent()
            .get(&GovernanceDataKey::UpgradeProposal(proposal_id))
    }

    pub fn get_config(env: Env, config_key: String) -> Option<String> {
        env.storage()
            .persistent()
            .get(&GovernanceDataKey::ConfigValue(config_key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger as _},
        Address, BytesN, Env, String,
    };

    fn setup() -> (Env, Address, GovernanceContractClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|li| {
            li.timestamp = 1000;
        });

        let contract_id = env.register(GovernanceContract, ());
        let client = GovernanceContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin, &2);
        (env, admin, client)
    }

    fn dummy_hash(env: &Env) -> BytesN<32> {
        BytesN::from_array(env, &[7u8; 32])
    }

    #[test]
    fn test_upgrade_proposal_lifecycle() {
        let (env, _admin, client) = setup();

        let proposer = Address::generate(&env);
        let voter1 = Address::generate(&env);
        let voter2 = Address::generate(&env);
        let hash = dummy_hash(&env);

        let prop_id = client.create_upgrade_proposal(&proposer, &hash, &2, &86400);
        assert_eq!(prop_id, 1);

        // Not yet approved (0 votes, need 2)
        assert!(!client.is_upgrade_proposal_approved(&prop_id));

        client.vote_proposal(&voter1, &prop_id);
        // Still only 1 approval
        assert!(!client.is_upgrade_proposal_approved(&prop_id));

        client.vote_proposal(&voter2, &prop_id);
        // Now approved
        assert!(client.is_upgrade_proposal_approved(&prop_id));

        // Consume the proposal
        let (consumed_hash, version) = client.consume_upgrade_proposal(&voter1, &prop_id);
        assert_eq!(consumed_hash, hash);
        assert_eq!(version, 2);

        // After consumption, the proposal is no longer approved (replay protection)
        assert!(!client.is_upgrade_proposal_approved(&prop_id));
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #8)")]
    fn test_upgrade_proposal_not_enough_approvals() {
        let (env, _admin, client) = setup();

        let proposer = Address::generate(&env);
        let hash = dummy_hash(&env);

        let prop_id = client.create_upgrade_proposal(&proposer, &hash, &2, &86400);
        // Only 1 vote but need 2
        client.vote_proposal(&proposer, &prop_id);
        client.consume_upgrade_proposal(&proposer, &prop_id);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #6)")]
    fn test_upgrade_proposal_expired() {
        let (env, _admin, client) = setup();

        let proposer = Address::generate(&env);
        let voter1 = Address::generate(&env);
        let voter2 = Address::generate(&env);
        let hash = dummy_hash(&env);

        let prop_id = client.create_upgrade_proposal(&proposer, &hash, &2, &3600);
        client.vote_proposal(&voter1, &prop_id);
        client.vote_proposal(&voter2, &prop_id);

        // Advance past deadline
        env.ledger().with_mut(|li| {
            li.timestamp += 3601;
        });

        client.consume_upgrade_proposal(&voter1, &prop_id);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #7)")]
    fn test_upgrade_proposal_double_execution_rejected() {
        let (env, _admin, client) = setup();

        let proposer = Address::generate(&env);
        let voter1 = Address::generate(&env);
        let voter2 = Address::generate(&env);
        let hash = dummy_hash(&env);

        let prop_id = client.create_upgrade_proposal(&proposer, &hash, &2, &86400);
        client.vote_proposal(&voter1, &prop_id);
        client.vote_proposal(&voter2, &prop_id);

        // First consume succeeds
        client.consume_upgrade_proposal(&voter1, &prop_id);

        // Second consume must fail
        client.consume_upgrade_proposal(&voter2, &prop_id);
    }

    #[test]
    fn test_config_proposal_still_works() {
        let (env, _admin, client) = setup();

        let proposer = Address::generate(&env);
        let voter1 = Address::generate(&env);
        let voter2 = Address::generate(&env);
        let key = String::from_str(&env, "max_fee");
        let val = String::from_str(&env, "100");

        let prop_id = client.create_proposal(&proposer, &key, &val, &86400);
        assert_eq!(prop_id, 1);

        client.vote_proposal(&voter1, &prop_id);
        client.vote_proposal(&voter2, &prop_id);
        client.execute_proposal(&voter1, &prop_id);

        let proposal = client.get_proposal(&prop_id).unwrap();
        assert!(proposal.executed);
        assert_eq!(client.get_config(&key).unwrap(), val);
    }
}

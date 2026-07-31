use super::policy::SpendingTier;
use super::{ProposalStatus, TreasuryContract, TreasuryContractClient};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env, Vec};

fn setup() -> (Env, Address, TreasuryContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(TreasuryContract, ());
    let client = TreasuryContractClient::new(&env, &contract_id);
    client.initialize(&admin);
    (env, admin, client)
}

fn setup_with_signers() -> (Env, Address, TreasuryContractClient<'static>, Vec<Address>) {
    let (env, admin, client) = setup();
    let signer1 = Address::generate(&env);
    let signer2 = Address::generate(&env);
    let signer3 = Address::generate(&env);
    let mut signers: Vec<Address> = Vec::new(&env);
    signers.push_back(signer1.clone());
    signers.push_back(signer2.clone());
    signers.push_back(signer3.clone());
    client.set_signers(&admin, &signers, &2);
    (env, admin, client, signers)
}

#[test]
fn initializes_with_zero_balances() {
    let (_env, _admin, client) = setup();
    assert_eq!(client.get_total_penalties(), 0);
    assert_eq!(client.get_total_fees(), 0);
    assert_eq!(client.get_total_rewards(), 0);
    assert_eq!(client.get_total_reserve(), 0);
}

#[test]
fn credit_penalty_updates_total() {
    let (_env, _admin, client) = setup();
    client.credit_penalty(&100i128);
    assert_eq!(client.get_total_penalties(), 100);
}

#[test]
fn credit_fee_updates_total() {
    let (_env, _admin, client) = setup();
    client.credit_fee(&250i128);
    assert_eq!(client.get_total_fees(), 250);
}

#[test]
fn credit_reward_updates_total() {
    let (_env, _admin, client) = setup();
    client.credit_reward(&500i128);
    assert_eq!(client.get_total_rewards(), 500);
}

#[test]
fn reserve_is_sum_of_all_totals() {
    let (_env, _admin, client) = setup();
    client.credit_penalty(&100i128);
    client.credit_fee(&200i128);
    client.credit_reward(&300i128);
    assert_eq!(client.get_total_reserve(), 600);
}

#[test]
fn set_and_query_signers() {
    let (env, admin, client) = setup();
    let signer1 = Address::generate(&env);
    let signer2 = Address::generate(&env);
    let mut signers: Vec<Address> = Vec::new(&env);
    signers.push_back(signer1.clone());
    signers.push_back(signer2.clone());
    client.set_signers(&admin, &signers, &2);
    let stored = client.get_signers();
    assert_eq!(stored.len(), 2);
    assert!(client.is_signer(&signer1));
    assert!(client.is_signer(&signer2));
    assert_eq!(client.get_threshold(), 2);
}

#[test]
fn add_remove_signer() {
    let (env, admin, client) = setup();
    let signer1 = Address::generate(&env);
    let signer2 = Address::generate(&env);
    let mut signers: Vec<Address> = Vec::new(&env);
    signers.push_back(signer1.clone());
    client.set_signers(&admin, &signers, &1);
    client.add_signer(&admin, &signer2);
    assert_eq!(client.get_signers().len(), 2);
    client.remove_signer(&admin, &signer1);
    assert_eq!(client.get_signers().len(), 1);
    assert!(!client.is_signer(&signer1));
}

#[test]
fn set_threshold_updates_requirement() {
    let (env, admin, client) = setup();
    let signer1 = Address::generate(&env);
    let signer2 = Address::generate(&env);
    let signer3 = Address::generate(&env);
    let mut signers: Vec<Address> = Vec::new(&env);
    signers.push_back(signer1);
    signers.push_back(signer2);
    signers.push_back(signer3);
    client.set_signers(&admin, &signers, &2);
    client.set_threshold(&admin, &3);
    assert_eq!(client.get_threshold(), 3);
}

#[test]
fn propose_disbursement_creates_pending_proposal() {
    let (env, admin, client, signers) = setup_with_signers();
    client.credit_fee(&10000i128);
    let pid = client.propose_disbursement(
        &signers.get(0).unwrap(),
        &signers.get(1).unwrap(),
        &500i128,
        &symbol_short!("test"),
    );
    let proposal = client.get_proposal(&pid).unwrap();
    assert_eq!(proposal.amount, 500);
    assert_eq!(
        client.get_proposal_status(&pid).unwrap(),
        ProposalStatus::Pending
    );
}

#[test]
fn single_sig_disbursement_below_lowest_tier_succeeds() {
    let (env, admin, client, signers) = setup_with_signers();
    client.credit_fee(&10000i128);
    let tier = SpendingTier {
        label: symbol_short!("low"),
        min_amount: 0i128,
        max_amount: 1000i128,
        required_signers: 1,
    };
    let mut tiers: Vec<SpendingTier> = Vec::new(&env);
    tiers.push_back(tier);
    client.set_spending_tiers(&admin, &tiers, &3);
    let pid = client.propose_disbursement(
        &signers.get(0).unwrap(),
        &signers.get(1).unwrap(),
        &500i128,
        &symbol_short!("test"),
    );
    client.approve_disbursement(&signers.get(0).unwrap(), &pid);
    let status = client.get_proposal_status(&pid).unwrap();
    assert_eq!(status, ProposalStatus::Approved);
}

#[test]
fn tier2_disbursement_needs_two_signatures() {
    let (env, admin, client, signers) = setup_with_signers();
    client.credit_fee(&10000i128);
    let low = SpendingTier {
        label: symbol_short!("low"),
        min_amount: 0i128,
        max_amount: 1000i128,
        required_signers: 1,
    };
    let high = SpendingTier {
        label: symbol_short!("high"),
        min_amount: 1000i128,
        max_amount: i128::MAX,
        required_signers: 2,
    };
    let mut tiers: Vec<SpendingTier> = Vec::new(&env);
    tiers.push_back(low);
    tiers.push_back(high);
    client.set_spending_tiers(&admin, &tiers, &3);
    let pid = client.propose_disbursement(
        &signers.get(0).unwrap(),
        &signers.get(1).unwrap(),
        &5000i128,
        &symbol_short!("test"),
    );
    client.approve_disbursement(&signers.get(0).unwrap(), &pid);
    let count = client.get_proposal_approval_count(&pid);
    assert_eq!(count, 1);
    let s = client.get_proposal_status(&pid).unwrap();
    assert_eq!(s, ProposalStatus::Pending);
    client.approve_disbursement(&signers.get(1).unwrap(), &pid);
    let count2 = client.get_proposal_approval_count(&pid);
    assert_eq!(count2, 2);
    let s2 = client.get_proposal_status(&pid).unwrap();
    assert_eq!(s2, ProposalStatus::Approved);
}

#[test]
fn disbursement_fails_with_insufficient_approvals() {
    let (env, admin, client, signers) = setup_with_signers();
    client.credit_fee(&10000i128);
    let tier = SpendingTier {
        label: symbol_short!("high"),
        min_amount: 0i128,
        max_amount: i128::MAX,
        required_signers: 3,
    };
    let mut tiers: Vec<SpendingTier> = Vec::new(&env);
    tiers.push_back(tier);
    client.set_spending_tiers(&admin, &tiers, &3);
    let pid = client.propose_disbursement(
        &signers.get(0).unwrap(),
        &signers.get(1).unwrap(),
        &500i128,
        &symbol_short!("test"),
    );
    client.approve_disbursement(&signers.get(0).unwrap(), &pid);
    let s = client.get_proposal_status(&pid).unwrap();
    assert_eq!(s, ProposalStatus::Pending);
}

#[test]
fn execute_disbursement_after_approval() {
    let (env, admin, client, signers) = setup_with_signers();
    client.credit_fee(&10000i128);
    let tier = SpendingTier {
        label: symbol_short!("low"),
        min_amount: 0i128,
        max_amount: i128::MAX,
        required_signers: 1,
    };
    let mut tiers: Vec<SpendingTier> = Vec::new(&env);
    tiers.push_back(tier);
    client.set_spending_tiers(&admin, &tiers, &3);
    let pid = client.propose_disbursement(
        &signers.get(0).unwrap(),
        &signers.get(1).unwrap(),
        &500i128,
        &symbol_short!("test"),
    );
    client.approve_disbursement(&signers.get(0).unwrap(), &pid);
    client.execute_disbursement(&signers.get(0).unwrap(), &pid);
    let s = client.get_proposal_status(&pid).unwrap();
    assert_eq!(s, ProposalStatus::Executed);
}

#[test]
fn execute_disbursement_rejected_when_insufficient_approvals() {
    let (env, admin, client, signers) = setup_with_signers();
    client.credit_fee(&10000i128);
    let tier = SpendingTier {
        label: symbol_short!("high"),
        min_amount: 0i128,
        max_amount: i128::MAX,
        required_signers: 3,
    };
    let mut tiers: Vec<SpendingTier> = Vec::new(&env);
    tiers.push_back(tier);
    client.set_spending_tiers(&admin, &tiers, &3);
    let pid = client.propose_disbursement(
        &signers.get(0).unwrap(),
        &signers.get(1).unwrap(),
        &500i128,
        &symbol_short!("test"),
    );
    client.approve_disbursement(&signers.get(0).unwrap(), &pid);
    let s = client.get_proposal_status(&pid).unwrap();
    assert_eq!(s, ProposalStatus::Pending);
}

#[test]
fn duplicate_approval_rejected() {
    let (env, admin, client, signers) = setup_with_signers();
    client.credit_fee(&10000i128);
    let tier = SpendingTier {
        label: symbol_short!("low"),
        min_amount: 0i128,
        max_amount: i128::MAX,
        required_signers: 1,
    };
    let mut tiers: Vec<SpendingTier> = Vec::new(&env);
    tiers.push_back(tier);
    client.set_spending_tiers(&admin, &tiers, &3);
    let pid = client.propose_disbursement(
        &signers.get(0).unwrap(),
        &signers.get(1).unwrap(),
        &500i128,
        &symbol_short!("test"),
    );
    client.approve_disbursement(&signers.get(0).unwrap(), &pid);
}

#[test]
#[should_panic(expected = "HostError")]
fn non_signer_cannot_propose() {
    let (env, _admin, client, signers) = setup_with_signers();
    client.credit_fee(&10000i128);
    let non_signer = Address::generate(&env);
    client.propose_disbursement(
        &non_signer,
        &signers.get(1).unwrap(),
        &500i128,
        &symbol_short!("test"),
    );
}

#[test]
#[should_panic(expected = "HostError")]
fn non_signer_cannot_approve() {
    let (env, _admin, client, signers) = setup_with_signers();
    client.credit_fee(&10000i128);
    let non_signer = Address::generate(&env);
    let pid = client.propose_disbursement(
        &signers.get(0).unwrap(),
        &signers.get(1).unwrap(),
        &500i128,
        &symbol_short!("test"),
    );
    client.approve_disbursement(&non_signer, &pid);
}

#[test]
fn cancel_proposal() {
    let (env, admin, client, signers) = setup_with_signers();
    client.credit_fee(&10000i128);
    let pid = client.propose_disbursement(
        &signers.get(0).unwrap(),
        &signers.get(1).unwrap(),
        &500i128,
        &symbol_short!("test"),
    );
    client.cancel_proposal(&admin, &pid);
    let s = client.get_proposal_status(&pid).unwrap();
    assert_eq!(s, ProposalStatus::Cancelled);
    assert!(client.get_proposal_approval_count(&pid) == 0);
}

#[test]
#[should_panic(expected = "HostError")]
fn cannot_execute_cancelled_proposal() {
    let (env, admin, client, signers) = setup_with_signers();
    client.credit_fee(&10000i128);
    let pid = client.propose_disbursement(
        &signers.get(0).unwrap(),
        &signers.get(1).unwrap(),
        &500i128,
        &symbol_short!("test"),
    );
    client.cancel_proposal(&admin, &pid);
    client.execute_disbursement(&signers.get(0).unwrap(), &pid);
}

#[test]
#[should_panic(expected = "HostError")]
fn proposal_rejected_when_insufficient_balance() {
    let (env, _admin, client, signers) = setup_with_signers();
    client.propose_disbursement(
        &signers.get(0).unwrap(),
        &signers.get(1).unwrap(),
        &99999i128,
        &symbol_short!("test"),
    );
}

#[test]
fn has_approved_returns_correctly() {
    let (env, admin, client, signers) = setup_with_signers();
    client.credit_fee(&10000i128);
    let tier = SpendingTier {
        label: symbol_short!("low"),
        min_amount: 0i128,
        max_amount: i128::MAX,
        required_signers: 2,
    };
    let mut tiers: Vec<SpendingTier> = Vec::new(&env);
    tiers.push_back(tier);
    client.set_spending_tiers(&admin, &tiers, &3);
    let pid = client.propose_disbursement(
        &signers.get(0).unwrap(),
        &signers.get(1).unwrap(),
        &500i128,
        &symbol_short!("test"),
    );
    assert!(!client.has_approved(&pid, &signers.get(0).unwrap()));
    client.approve_disbursement(&signers.get(0).unwrap(), &pid);
    assert!(client.has_approved(&pid, &signers.get(0).unwrap()));
    assert!(!client.has_approved(&pid, &signers.get(1).unwrap()));
}

#[test]
fn get_required_signers_for_amount_returns_tier() {
    let (env, admin, client, signers) = setup_with_signers();
    let low = SpendingTier {
        label: symbol_short!("low"),
        min_amount: 0i128,
        max_amount: 1000i128,
        required_signers: 1,
    };
    let medium = SpendingTier {
        label: symbol_short!("med"),
        min_amount: 1000i128,
        max_amount: 10000i128,
        required_signers: 2,
    };
    let mut tiers: Vec<SpendingTier> = Vec::new(&env);
    tiers.push_back(low);
    tiers.push_back(medium);
    client.set_spending_tiers(&admin, &tiers, &3);
    assert_eq!(client.get_required_signers_for_amount(&500i128), 1);
    assert_eq!(client.get_required_signers_for_amount(&5000i128), 2);
}

#[test]
fn acceptance_criteria_tiered_disbursement() {
    // Configure tiers (1 sig under 100, 2-of-3 from 100–10k)
    let (env, admin, client, signers) = setup_with_signers();
    client.credit_fee(&20000i128);
    let low = SpendingTier {
        label: symbol_short!("low"),
        min_amount: 0i128,
        max_amount: 100i128,
        required_signers: 1,
    };
    let high = SpendingTier {
        label: symbol_short!("high"),
        min_amount: 100i128,
        max_amount: 10000i128,
        required_signers: 2,
    };
    let mut tiers: Vec<SpendingTier> = Vec::new(&env);
    tiers.push_back(low);
    tiers.push_back(high);
    client.set_spending_tiers(&admin, &tiers, &3);

    // Attempt a 50-unit disbursement with 1 signature (succeeds)
    let pid1 = client.propose_disbursement(
        &signers.get(0).unwrap(),
        &signers.get(1).unwrap(),
        &50i128,
        &symbol_short!("small"),
    );
    client.approve_disbursement(&signers.get(0).unwrap(), &pid1);
    let status1 = client.get_proposal_status(&pid1).unwrap();
    assert_eq!(status1, ProposalStatus::Approved);

    // Attempt a 500-unit disbursement with 1 signature (fails - still pending)
    let pid2 = client.propose_disbursement(
        &signers.get(0).unwrap(),
        &signers.get(1).unwrap(),
        &500i128,
        &symbol_short!("medium"),
    );
    client.approve_disbursement(&signers.get(0).unwrap(), &pid2);
    let status2 = client.get_proposal_status(&pid2).unwrap();
    assert_eq!(status2, ProposalStatus::Pending);
    assert_eq!(client.get_proposal_approval_count(&pid2), 1);

    // Add a second approval (succeeds)
    client.approve_disbursement(&signers.get(1).unwrap(), &pid2);
    let status3 = client.get_proposal_status(&pid2).unwrap();
    assert_eq!(status3, ProposalStatus::Approved);
    assert_eq!(client.get_proposal_approval_count(&pid2), 2);
}

use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _},
    Address, Env, Symbol, TryFromVal, Vec,
};

#[path = "../contracts/transactions.rs"]
mod transactions;

use transactions::{TransactionsContract, TransactionsContractClient};

fn setup_test_contract() -> (Env, Address, TransactionsContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(TransactionsContract, ());
    let client = TransactionsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    (env, admin, client)
}

fn configure_multisig(
    env: &Env,
    client: &TransactionsContractClient<'static>,
    admin: &Address,
    threshold: u32,
) -> (Address, Address, Address) {
    let signer_1 = Address::generate(env);
    let signer_2 = Address::generate(env);
    let signer_3 = Address::generate(env);

    let mut signers: Vec<Address> = Vec::new(env);
    signers.push_back(signer_1.clone());
    signers.push_back(signer_2.clone());
    signers.push_back(signer_3.clone());

    client.set_signers(admin, &signers, &threshold);
    client.set_high_value_threshold(admin, &100);

    (signer_1, signer_2, signer_3)
}

#[test]
fn test_set_signers_and_threshold_works() {
    let (env, admin, client) = setup_test_contract();

    let signer_1 = Address::generate(&env);
    let signer_2 = Address::generate(&env);

    let mut signers: Vec<Address> = Vec::new(&env);
    signers.push_back(signer_1.clone());
    signers.push_back(signer_2.clone());

    client.set_signers(&admin, &signers, &2);

    let configured = client.get_signers();
    assert_eq!(configured.len(), 2);
    assert_eq!(configured.get(0), Some(signer_1));
    assert_eq!(configured.get(1), Some(signer_2));
    assert_eq!(client.get_threshold(), 2);
}

#[test]
fn test_get_multisig_threshold_returns_correct_value() {
    let (env, admin, client) = setup_test_contract();

    let signer_1 = Address::generate(&env);
    let signer_2 = Address::generate(&env);
    let signer_3 = Address::generate(&env);

    let mut signers: Vec<Address> = Vec::new(&env);
    signers.push_back(signer_1);
    signers.push_back(signer_2);
    signers.push_back(signer_3);

    client.set_signers(&admin, &signers, &2);

    assert_eq!(client.get_multisig_threshold(), 2);
}

#[test]
#[should_panic]
fn test_set_signers_admin_only() {
    let (env, admin, client) = setup_test_contract();

    let signer_1 = Address::generate(&env);
    let signer_2 = Address::generate(&env);

    let mut signers: Vec<Address> = Vec::new(&env);
    signers.push_back(signer_1);
    signers.push_back(signer_2);

    let unauthorized = Address::generate(&env);

    // Admin config works first, then unauthorized update is rejected.
    client.set_signers(&admin, &signers, &2);
    client.set_signers(&unauthorized, &signers, &1);
}

#[test]
#[should_panic]
fn test_invalid_threshold_zero_rejected() {
    let (env, admin, client) = setup_test_contract();

    let signer_1 = Address::generate(&env);

    let mut signers: Vec<Address> = Vec::new(&env);
    signers.push_back(signer_1);

    client.set_signers(&admin, &signers, &0);
}

#[test]
#[should_panic]
fn test_invalid_threshold_above_signers_rejected() {
    let (env, admin, client) = setup_test_contract();

    let signer_1 = Address::generate(&env);
    let signer_2 = Address::generate(&env);

    let mut signers: Vec<Address> = Vec::new(&env);
    signers.push_back(signer_1);
    signers.push_back(signer_2);

    client.set_signers(&admin, &signers, &3);
}

#[test]
#[should_panic]
fn test_duplicate_signers_rejected() {
    let (env, admin, client) = setup_test_contract();

    let signer = Address::generate(&env);

    let mut signers: Vec<Address> = Vec::new(&env);
    signers.push_back(signer.clone());
    signers.push_back(signer);

    client.set_signers(&admin, &signers, &1);
}

#[test]
fn test_low_value_executes_immediately_no_pending() {
    let (env, admin, client) = setup_test_contract();

    configure_multisig(&env, &client, &admin, 2);

    let from = Address::generate(&env);
    let to = Address::generate(&env);

    client.set_balance(&admin, &from, &1_000);

    let asset: Option<Address> = None;
    let pending_id = client.submit_transaction(&from, &to, &50, &symbol_short!("pay"), &asset);

    assert_eq!(pending_id, None);
    assert_eq!(client.get_balance(&from), 950);
    assert_eq!(client.get_balance(&to), 50);
}

#[test]
fn test_high_value_creates_pending_record() {
    let (env, admin, client) = setup_test_contract();

    configure_multisig(&env, &client, &admin, 2);

    let from = Address::generate(&env);
    let to = Address::generate(&env);

    client.set_balance(&admin, &from, &1_000);

    let asset: Option<Address> = None;
    let pending_id = client.submit_transaction(&from, &to, &100, &symbol_short!("pay"), &asset);
    let tx_id = pending_id.expect("expected pending tx id");

    let pending = client.get_pending_tx(&tx_id).expect("missing pending tx");
    assert_eq!(pending.id, tx_id);
    assert_eq!(pending.amount, 100);
    assert_eq!(pending.from, from);
    assert_eq!(pending.to, to);
    assert_eq!(pending.executed, false);

    // No balance movement before approvals reach threshold.
    assert_eq!(client.get_balance(&pending.from), 1_000);
    assert_eq!(client.get_balance(&pending.to), 0);
}

#[test]
#[should_panic]
fn test_unauthorized_approver_rejected() {
    let (env, admin, client) = setup_test_contract();

    let (signer_1, _signer_2, _signer_3) = configure_multisig(&env, &client, &admin, 2);

    let from = Address::generate(&env);
    let to = Address::generate(&env);
    client.set_balance(&admin, &from, &1_000);

    let asset: Option<Address> = None;
    let tx_id = client
        .submit_transaction(&from, &to, &300, &symbol_short!("pay"), &asset)
        .expect("expected pending tx id");

    // Signer 1 is valid; this outsider is not.
    let outsider = Address::generate(&env);
    assert_ne!(outsider, signer_1);
    client.approve(&tx_id, &outsider);
}

#[test]
fn test_signer_can_approve_once_and_approval_persists() {
    let (env, admin, client) = setup_test_contract();

    let (signer_1, signer_2, _signer_3) = configure_multisig(&env, &client, &admin, 2);

    let from = Address::generate(&env);
    let to = Address::generate(&env);
    client.set_balance(&admin, &from, &1_000);

    let asset: Option<Address> = None;
    let tx_id = client
        .submit_transaction(&from, &to, &300, &symbol_short!("pay"), &asset)
        .expect("expected pending tx id");

    client.approve(&tx_id, &signer_1);

    assert_eq!(client.get_approval_count(&tx_id), 1);
    assert!(client.has_approved(&tx_id, &signer_1));
    assert!(!client.has_approved(&tx_id, &signer_2));

    let pending = client.get_pending_tx(&tx_id).expect("missing pending tx");
    assert_eq!(pending.executed, false);
}

#[test]
#[should_panic]
fn test_duplicate_approval_rejected() {
    let (env, admin, client) = setup_test_contract();

    let (signer_1, _signer_2, _signer_3) = configure_multisig(&env, &client, &admin, 2);

    let from = Address::generate(&env);
    let to = Address::generate(&env);
    client.set_balance(&admin, &from, &1_000);

    let asset: Option<Address> = None;
    let tx_id = client
        .submit_transaction(&from, &to, &300, &symbol_short!("pay"), &asset)
        .expect("expected pending tx id");

    client.approve(&tx_id, &signer_1);
    client.approve(&tx_id, &signer_1);
}

#[test]
fn test_does_not_execute_before_threshold() {
    let (env, admin, client) = setup_test_contract();

    let (signer_1, _signer_2, _signer_3) = configure_multisig(&env, &client, &admin, 2);

    let from = Address::generate(&env);
    let to = Address::generate(&env);
    client.set_balance(&admin, &from, &1_000);

    let asset: Option<Address> = None;
    let tx_id = client
        .submit_transaction(&from, &to, &300, &symbol_short!("pay"), &asset)
        .expect("expected pending tx id");

    client.approve(&tx_id, &signer_1);

    let pending = client.get_pending_tx(&tx_id).expect("missing pending tx");
    assert_eq!(pending.executed, false);
    assert_eq!(client.get_balance(&from), 1_000);
    assert_eq!(client.get_balance(&to), 0);
}

#[test]
fn test_executes_exactly_at_threshold_and_changes_state_once() {
    let (env, admin, client) = setup_test_contract();

    let (signer_1, signer_2, _signer_3) = configure_multisig(&env, &client, &admin, 2);

    let from = Address::generate(&env);
    let to = Address::generate(&env);
    client.set_balance(&admin, &from, &1_000);

    let asset: Option<Address> = None;
    let tx_id = client
        .submit_transaction(&from, &to, &300, &symbol_short!("pay"), &asset)
        .expect("expected pending tx id");

    client.approve(&tx_id, &signer_1);
    assert_eq!(client.get_balance(&from), 1_000);
    assert_eq!(client.get_balance(&to), 0);

    client.approve(&tx_id, &signer_2);

    let pending = client.get_pending_tx(&tx_id).expect("missing pending tx");
    assert_eq!(pending.executed, true);
    assert_eq!(client.get_approval_count(&tx_id), 2);
    assert_eq!(client.get_balance(&from), 700);
    assert_eq!(client.get_balance(&to), 300);
}

#[test]
#[should_panic]
fn test_cannot_execute_twice() {
    let (env, admin, client) = setup_test_contract();

    let (signer_1, signer_2, signer_3) = configure_multisig(&env, &client, &admin, 2);

    let from = Address::generate(&env);
    let to = Address::generate(&env);
    client.set_balance(&admin, &from, &1_000);

    let asset: Option<Address> = None;
    let tx_id = client
        .submit_transaction(&from, &to, &300, &symbol_short!("pay"), &asset)
        .expect("expected pending tx id");

    client.approve(&tx_id, &signer_1);
    client.approve(&tx_id, &signer_2);

    // Already executed at threshold; later approvals are rejected.
    client.approve(&tx_id, &signer_3);
}

#[test]
#[should_panic]
fn test_approving_nonexistent_tx_fails() {
    let (env, admin, client) = setup_test_contract();

    let (signer_1, _signer_2, _signer_3) = configure_multisig(&env, &client, &admin, 2);

    client.approve(&999, &signer_1);
}

#[test]
fn test_emits_approval_and_execution_events() {
    let (env, admin, client) = setup_test_contract();

    let (signer_1, signer_2, _signer_3) = configure_multisig(&env, &client, &admin, 2);

    let from = Address::generate(&env);
    let to = Address::generate(&env);
    client.set_balance(&admin, &from, &1_000);

    let asset: Option<Address> = None;
    let tx_id = client
        .submit_transaction(&from, &to, &300, &symbol_short!("pay"), &asset)
        .expect("expected pending tx id");

    client.approve(&tx_id, &signer_1);
    client.approve(&tx_id, &signer_2);

    let events = env.events().all();
    let approve_sym = symbol_short!("approve");
    let record_sym = symbol_short!("record");
    let executed_sym = symbol_short!("executed");

    let approval_events = events
        .iter()
        .filter(|event| {
            let has_approve = event.1.iter().any(|topic| {
                Symbol::try_from_val(&env, &topic)
                    .map(|sym| sym == approve_sym)
                    .unwrap_or(false)
            });
            let has_record = event.1.iter().any(|topic| {
                Symbol::try_from_val(&env, &topic)
                    .map(|sym| sym == record_sym)
                    .unwrap_or(false)
            });
            has_approve && has_record
        })
        .count();

    let execution_events = events
        .iter()
        .filter(|event| {
            event.1.iter().any(|topic| {
                Symbol::try_from_val(&env, &topic)
                    .map(|sym| sym == executed_sym)
                    .unwrap_or(false)
            })
        })
        .count();

    assert!(approval_events >= 1);
    assert_eq!(execution_events, 1);
}

#[test]
#[should_panic]
fn test_set_high_value_threshold_admin_only() {
    let (env, admin, client) = setup_test_contract();

    let unauthorized = Address::generate(&env);
    client.set_high_value_threshold(&admin, &100);
    client.set_high_value_threshold(&unauthorized, &200);
}

#[test]
fn test_block_and_unblock_destination() {
    let (env, _admin, client) = setup_test_contract();
    let user = Address::generate(&env);
    let dest = Address::generate(&env);

    assert_eq!(client.is_destination_blocked(&user, &dest), false);

    client.block_destination(&user, &dest);
    assert_eq!(client.is_destination_blocked(&user, &dest), true);

    client.unblock_destination(&user, &dest);
    assert_eq!(client.is_destination_blocked(&user, &dest), false);
}

#[test]
#[should_panic]
fn test_submit_transaction_to_blocked_destination_fails() {
    let (env, admin, client) = setup_test_contract();
    configure_multisig(&env, &client, &admin, 2);

    let from = Address::generate(&env);
    let to = Address::generate(&env);
    client.set_balance(&admin, &from, &1_000);

    client.block_destination(&from, &to);

    let asset: Option<Address> = None;
    client.submit_transaction(&from, &to, &100, &symbol_short!("pay"), &asset);
}

#[test]
#[should_panic]
fn test_schedule_timelocked_transaction_to_blocked_destination_fails() {
    let (env, admin, client) = setup_test_contract();
    configure_multisig(&env, &client, &admin, 2);

    let from = Address::generate(&env);
    let to = Address::generate(&env);
    client.set_balance(&admin, &from, &1_000);

    client.block_destination(&from, &to);

    let asset: Option<Address> = None;
    let execute_at = env.ledger().timestamp() + 1000;
    client.schedule_timelocked_transaction(&from, &to, &100, &symbol_short!("pay"), &asset, &execute_at);
}


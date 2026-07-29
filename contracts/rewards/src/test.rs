#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env};

use crate::{
    storage::{
        get_account_status, get_lifetime_claimed, get_lifetime_earned, get_metadata_version,
        get_reward_account, get_reward_balance, get_reward_index, get_reward_transaction,
        get_reward_tx_counter, has_reward_account, set_account_status, set_lifetime_claimed,
        set_lifetime_earned, set_reward_account, set_reward_balance,
    },
    types::{
        AccountStatus, RewardAccount, RewardStatus, RewardTransaction, RewardType,
        DEFAULT_METADATA_VERSION,
    },
    RewardsContract, RewardsContractClient,
};

fn setup() -> (Env, Address, RewardsContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(RewardsContract, ());
    let client = RewardsContractClient::new(&env, &contract_id);
    (env, admin, client)
}

// ── Contract entry-point tests (from #875) ────────────────────────────────────

#[test]
fn test_initialize_sets_admin() {
    let (_env, admin, client) = setup();
    client.initialize(&admin);
    assert_eq!(client.get_admin(), admin);
}

#[test]
fn test_is_initialized_returns_true_after_init() {
    let (_env, admin, client) = setup();
    assert!(!client.is_initialized());
    client.initialize(&admin);
    assert!(client.is_initialized());
}

#[test]
#[should_panic]
fn test_double_initialize_panics() {
    let (_env, admin, client) = setup();
    client.initialize(&admin);
    client.initialize(&admin);
}

#[test]
#[should_panic]
fn test_get_admin_before_init_panics() {
    let (_env, _admin, client) = setup();
    client.get_admin();
}

// ── Storage helper tests (#876) ───────────────────────────────────────────────
//
// Storage helpers must be invoked from within a contract context.
// We use `env.as_contract(&contract_id, || { ... })` to satisfy that
// requirement without needing a dedicated accessor entry point on the contract.

#[test]
fn test_reward_balance_defaults_to_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let user = Address::generate(&env);
    let contract_id = env.register(RewardsContract, ());
    env.as_contract(&contract_id, || {
        assert_eq!(get_reward_balance(&env, &user), 0);
    });
}

#[test]
fn test_set_and_get_reward_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let user = Address::generate(&env);
    let contract_id = env.register(RewardsContract, ());
    env.as_contract(&contract_id, || {
        set_reward_balance(&env, &user, 5_000_000);
        assert_eq!(get_reward_balance(&env, &user), 5_000_000);
    });
}

#[test]
fn test_reward_balance_overwrite() {
    let env = Env::default();
    env.mock_all_auths();
    let user = Address::generate(&env);
    let contract_id = env.register(RewardsContract, ());
    env.as_contract(&contract_id, || {
        set_reward_balance(&env, &user, 1_000);
        set_reward_balance(&env, &user, 9_999);
        assert_eq!(get_reward_balance(&env, &user), 9_999);
    });
}

#[test]
fn test_lifetime_earned_defaults_to_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let user = Address::generate(&env);
    let contract_id = env.register(RewardsContract, ());
    env.as_contract(&contract_id, || {
        assert_eq!(get_lifetime_earned(&env, &user), 0);
    });
}

#[test]
fn test_set_and_get_lifetime_earned() {
    let env = Env::default();
    env.mock_all_auths();
    let user = Address::generate(&env);
    let contract_id = env.register(RewardsContract, ());
    env.as_contract(&contract_id, || {
        set_lifetime_earned(&env, &user, 100_000_000);
        assert_eq!(get_lifetime_earned(&env, &user), 100_000_000);
    });
}

#[test]
fn test_lifetime_claimed_defaults_to_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let user = Address::generate(&env);
    let contract_id = env.register(RewardsContract, ());
    env.as_contract(&contract_id, || {
        assert_eq!(get_lifetime_claimed(&env, &user), 0);
    });
}

#[test]
fn test_set_and_get_lifetime_claimed() {
    let env = Env::default();
    env.mock_all_auths();
    let user = Address::generate(&env);
    let contract_id = env.register(RewardsContract, ());
    env.as_contract(&contract_id, || {
        set_lifetime_claimed(&env, &user, 50_000_000);
        assert_eq!(get_lifetime_claimed(&env, &user), 50_000_000);
    });
}

#[test]
fn test_has_reward_account_false_before_creation() {
    let env = Env::default();
    env.mock_all_auths();
    let user = Address::generate(&env);
    let contract_id = env.register(RewardsContract, ());
    env.as_contract(&contract_id, || {
        assert!(!has_reward_account(&env, &user));
    });
}

#[test]
fn test_set_and_get_reward_account() {
    let env = Env::default();
    env.mock_all_auths();
    let user = Address::generate(&env);
    let contract_id = env.register(RewardsContract, ());

    env.as_contract(&contract_id, || {
        let record = RewardAccount {
            owner: user.clone(),
            balance: 2_000_000,
            lifetime_earned: 10_000_000,
            lifetime_claimed: 8_000_000,
            created_at: 100,
            last_updated: 200,
            account_status: AccountStatus::Active,
            metadata_version: DEFAULT_METADATA_VERSION,
        };

        set_reward_account(&env, &user, &record);
        assert!(has_reward_account(&env, &user));

        let fetched = get_reward_account(&env, &user).expect("account should exist");
        assert_eq!(fetched.owner, user);
        assert_eq!(fetched.balance, 2_000_000);
        assert_eq!(fetched.lifetime_earned, 10_000_000);
        assert_eq!(fetched.lifetime_claimed, 8_000_000);
        assert_eq!(fetched.created_at, 100);
        assert_eq!(fetched.last_updated, 200);
        assert_eq!(fetched.account_status, AccountStatus::Active);
        assert_eq!(fetched.metadata_version, DEFAULT_METADATA_VERSION);
    });
}

#[test]
fn test_reward_account_returns_none_when_absent() {
    let env = Env::default();
    env.mock_all_auths();
    let user = Address::generate(&env);
    let contract_id = env.register(RewardsContract, ());
    env.as_contract(&contract_id, || {
        assert!(get_reward_account(&env, &user).is_none());
    });
}

#[test]
fn test_balances_are_independent_per_user() {
    let env = Env::default();
    env.mock_all_auths();
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);
    let contract_id = env.register(RewardsContract, ());
    env.as_contract(&contract_id, || {
        set_reward_balance(&env, &user_a, 1_000);
        set_reward_balance(&env, &user_b, 2_000);
        assert_eq!(get_reward_balance(&env, &user_a), 1_000);
        assert_eq!(get_reward_balance(&env, &user_b), 2_000);
    });
}

// ── Reward account registration tests (#878) ─────────────────────────────────

#[test]
fn test_register_account_succeeds() {
    let (env, admin, client) = setup();
    client.initialize(&admin);
    let user = Address::generate(&env);
    client.register_account(&user);
    assert!(client.get_account(&user).is_some());
}

#[test]
fn test_register_account_stores_correct_defaults() {
    let (env, admin, client) = setup();
    client.initialize(&admin);
    let user = Address::generate(&env);
    client.register_account(&user);

    let account = client.get_account(&user).expect("account should exist");
    assert_eq!(account.owner, user);
    assert_eq!(account.balance, 0);
    assert_eq!(account.lifetime_earned, 0);
    assert_eq!(account.lifetime_claimed, 0);
}

#[test]
fn test_register_account_sets_timestamps() {
    let (env, admin, client) = setup();
    client.initialize(&admin);
    let user = Address::generate(&env);
    client.register_account(&user);

    let account = client.get_account(&user).expect("account should exist");
    assert_eq!(account.created_at, account.last_updated);
}

#[test]
#[should_panic]
fn test_duplicate_registration_panics() {
    let (env, admin, client) = setup();
    client.initialize(&admin);
    let user = Address::generate(&env);
    client.register_account(&user);
    client.register_account(&user);
}

#[test]
#[should_panic]
fn test_register_account_before_init_panics() {
    let (env, _admin, client) = setup();
    let user = Address::generate(&env);
    client.register_account(&user);
}

#[test]
fn test_get_account_returns_none_for_unregistered() {
    let (env, admin, client) = setup();
    client.initialize(&admin);
    let user = Address::generate(&env);
    assert!(client.get_account(&user).is_none());
}

#[test]
fn test_multiple_accounts_are_independent() {
    let (env, admin, client) = setup();
    client.initialize(&admin);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    client.register_account(&user_a);
    client.register_account(&user_b);

    let a = client.get_account(&user_a).expect("user_a should exist");
    let b = client.get_account(&user_b).expect("user_b should exist");
    assert_eq!(a.owner, user_a);
    assert_eq!(b.owner, user_b);
    assert_ne!(a.owner, b.owner);
}

// ── Reward crediting tests (#879) ─────────────────────────────────────────────

fn setup_with_user() -> (Env, Address, Address, RewardsContractClient<'static>) {
    let (env, admin, client) = setup();
    client.initialize(&admin);
    let user = Address::generate(&env);
    client.register_account(&user);
    (env, admin, user, client)
}

#[test]
fn test_credit_reward_updates_balance() {
    let (_env, admin, user, client) = setup_with_user();
    client.credit_reward(&user, &1_000_000, &RewardType::SpendingLimit);
    let account = client.get_account(&user).unwrap();
    assert_eq!(account.balance, 1_000_000);
}

#[test]
fn test_credit_reward_updates_lifetime_earned() {
    let (_env, admin, user, client) = setup_with_user();
    client.credit_reward(&user, &500_000, &RewardType::SavingsGoal);
    let account = client.get_account(&user).unwrap();
    assert_eq!(account.lifetime_earned, 500_000);
}

#[test]
fn test_credit_reward_does_not_change_lifetime_claimed() {
    let (_env, admin, user, client) = setup_with_user();
    client.credit_reward(&user, &250_000, &RewardType::Streak);
    let account = client.get_account(&user).unwrap();
    assert_eq!(account.lifetime_claimed, 0);
}

#[test]
fn test_credit_reward_accumulates_across_multiple_credits() {
    let (_env, admin, user, client) = setup_with_user();
    client.credit_reward(&user, &100_000, &RewardType::Referral);
    client.credit_reward(&user, &200_000, &RewardType::ManualGrant);
    client.credit_reward(&user, &300_000, &RewardType::Streak);
    let account = client.get_account(&user).unwrap();
    assert_eq!(account.balance, 600_000);
    assert_eq!(account.lifetime_earned, 600_000);
}

#[test]
fn test_credit_reward_returns_correct_transaction_fields() {
    let (_env, admin, user, client) = setup_with_user();
    let tx = client.credit_reward(&user, &750_000, &RewardType::SavingsGoal);
    assert_eq!(tx.recipient, user);
    assert_eq!(tx.amount, 750_000);
    assert_eq!(tx.reward_type, RewardType::SavingsGoal);
    assert_eq!(tx.status, RewardStatus::Confirmed);
    assert_eq!(tx.updated_at, 0);
}

#[test]
fn test_credit_reward_assigns_incrementing_tx_ids() {
    let (_env, admin, user, client) = setup_with_user();
    let tx0 = client.credit_reward(&user, &100, &RewardType::Streak);
    let tx1 = client.credit_reward(&user, &200, &RewardType::Streak);
    let tx2 = client.credit_reward(&user, &300, &RewardType::Streak);
    assert_eq!(tx0.id, 0);
    assert_eq!(tx1.id, 1);
    assert_eq!(tx2.id, 2);
}

#[test]
fn test_credit_reward_persists_transaction_record() {
    let (env, admin, user, client) = setup_with_user();
    let contract_id = client.address.clone();
    client.credit_reward(&user, &999, &RewardType::ManualGrant);
    env.as_contract(&contract_id, || {
        let tx = get_reward_transaction(&env, 0);
        assert!(tx.is_some());
        let tx = tx.unwrap();
        assert_eq!(tx.amount, 999);
        assert_eq!(tx.reward_type, RewardType::ManualGrant);
        assert_eq!(tx.status, RewardStatus::Confirmed);
    });
}

#[test]
fn test_credit_reward_advances_tx_counter() {
    let (env, admin, user, client) = setup_with_user();
    let contract_id = client.address.clone();
    env.as_contract(&contract_id, || {
        assert_eq!(get_reward_tx_counter(&env), 0);
    });
    client.credit_reward(&user, &100, &RewardType::Streak);
    client.credit_reward(&user, &200, &RewardType::Streak);
    env.as_contract(&contract_id, || {
        assert_eq!(get_reward_tx_counter(&env), 2);
    });
}

#[test]
fn test_credit_reward_scalar_storage_matches_account() {
    let (env, admin, user, client) = setup_with_user();
    let contract_id = client.address.clone();
    client.credit_reward(&user, &1_234_567, &RewardType::SpendingLimit);
    let account = client.get_account(&user).unwrap();
    env.as_contract(&contract_id, || {
        assert_eq!(get_reward_balance(&env, &user), account.balance);
        assert_eq!(get_lifetime_earned(&env, &user), account.lifetime_earned);
    });
}

#[test]
fn test_credit_reward_updates_last_updated() {
    let (_env, admin, user, client) = setup_with_user();
    client.credit_reward(&user, &1_000, &RewardType::Referral);
    let account = client.get_account(&user).unwrap();
    assert!(account.last_updated >= account.created_at);
}

#[test]
fn test_credit_reward_multiple_users_are_independent() {
    let (env, admin, client) = setup();
    client.initialize(&admin);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);
    client.register_account(&user_a);
    client.register_account(&user_b);

    client.credit_reward(&user_a, &1_000, &RewardType::Streak);
    client.credit_reward(&user_b, &5_000, &RewardType::ManualGrant);

    let a = client.get_account(&user_a).unwrap();
    let b = client.get_account(&user_b).unwrap();
    assert_eq!(a.balance, 1_000);
    assert_eq!(b.balance, 5_000);
}

#[test]
#[should_panic]
fn test_credit_reward_zero_amount_panics() {
    let (_env, admin, user, client) = setup_with_user();
    client.credit_reward(&user, &0, &RewardType::Streak);
}

#[test]
#[should_panic]
fn test_credit_reward_negative_amount_panics() {
    let (_env, admin, user, client) = setup_with_user();
    client.credit_reward(&user, &-1, &RewardType::Streak);
}

#[test]
#[should_panic]
fn test_credit_reward_unregistered_account_panics() {
    let (env, admin, client) = setup();
    client.initialize(&admin);
    let stranger = Address::generate(&env);
    client.credit_reward(&stranger, &1_000, &RewardType::Streak);
}

#[test]
#[should_panic]
fn test_credit_reward_before_init_panics() {
    let (env, admin, client) = setup();
    let user = Address::generate(&env);
    client.credit_reward(&user, &1_000, &RewardType::Streak);
}

#[test]
#[should_panic]
fn test_credit_reward_overflow_on_balance_panics() {
    let (_env, admin, user, client) = setup_with_user();
    client.credit_reward(&user, &i128::MAX, &RewardType::ManualGrant);
    client.credit_reward(&user, &1, &RewardType::ManualGrant);
}

#[test]
fn test_credit_reward_i128_max_is_accepted() {
    let (_env, admin, user, client) = setup_with_user();
    let tx = client.credit_reward(&user, &i128::MAX, &RewardType::ManualGrant);
    assert_eq!(tx.amount, i128::MAX);
    let account = client.get_account(&user).unwrap();
    assert_eq!(account.balance, i128::MAX);
    assert_eq!(account.lifetime_earned, i128::MAX);
}

// ── Reward debiting tests ─────────────────────────────────────────────────────

#[test]
fn test_debit_reward_reduces_balance() {
    let (_env, admin, user, client) = setup_with_user();
    client.credit_reward(&user, &1_000_000, &RewardType::SpendingLimit);
    client.debit_reward(&user, &400_000, &RewardType::SpendingLimit);
    let account = client.get_account(&user).unwrap();
    assert_eq!(account.balance, 600_000);
}

#[test]
fn test_debit_reward_updates_lifetime_claimed() {
    let (_env, admin, user, client) = setup_with_user();
    client.credit_reward(&user, &1_000_000, &RewardType::SavingsGoal);
    client.debit_reward(&user, &300_000, &RewardType::SavingsGoal);
    let account = client.get_account(&user).unwrap();
    assert_eq!(account.lifetime_claimed, 300_000);
}

#[test]
fn test_debit_reward_does_not_change_lifetime_earned() {
    let (_env, admin, user, client) = setup_with_user();
    client.credit_reward(&user, &500_000, &RewardType::Streak);
    client.debit_reward(&user, &200_000, &RewardType::Streak);
    let account = client.get_account(&user).unwrap();
    assert_eq!(account.lifetime_earned, 500_000);
}

#[test]
fn test_debit_reward_returns_claimed_transaction() {
    let (_env, admin, user, client) = setup_with_user();
    client.credit_reward(&user, &1_000_000, &RewardType::Referral);
    let tx = client.debit_reward(&user, &250_000, &RewardType::Referral);
    assert_eq!(tx.recipient, user);
    assert_eq!(tx.amount, 250_000);
    assert_eq!(tx.reward_type, RewardType::Referral);
    assert_eq!(tx.status, RewardStatus::Claimed);
}

#[test]
fn test_debit_reward_exact_balance_succeeds() {
    let (_env, admin, user, client) = setup_with_user();
    client.credit_reward(&user, &1_000_000, &RewardType::ManualGrant);
    client.debit_reward(&user, &1_000_000, &RewardType::ManualGrant);
    let account = client.get_account(&user).unwrap();
    assert_eq!(account.balance, 0);
    assert_eq!(account.lifetime_claimed, 1_000_000);
}

#[test]
fn test_debit_reward_accumulates_across_multiple_debits() {
    let (_env, admin, user, client) = setup_with_user();
    client.credit_reward(&user, &1_000_000, &RewardType::Streak);
    client.debit_reward(&user, &100_000, &RewardType::Streak);
    client.debit_reward(&user, &200_000, &RewardType::Streak);
    client.debit_reward(&user, &300_000, &RewardType::Streak);
    let account = client.get_account(&user).unwrap();
    assert_eq!(account.balance, 400_000);
    assert_eq!(account.lifetime_claimed, 600_000);
}

#[test]
fn test_debit_reward_assigns_incrementing_tx_ids() {
    let (_env, admin, user, client) = setup_with_user();
    client.credit_reward(&user, &1_000_000, &RewardType::Streak);
    let tx0 = client.debit_reward(&user, &100, &RewardType::Streak);
    let tx1 = client.debit_reward(&user, &200, &RewardType::Streak);
    // tx id 0 was consumed by the credit, so debit ids start at 1
    assert_eq!(tx1.id, tx0.id + 1);
}

#[test]
fn test_debit_reward_persists_transaction_record() {
    let (env, admin, user, client) = setup_with_user();
    let contract_id = client.address.clone();
    client.credit_reward(&user, &1_000_000, &RewardType::ManualGrant);
    let tx = client.debit_reward(&user, &500_000, &RewardType::ManualGrant);
    let tx_id = tx.id;
    env.as_contract(&contract_id, || {
        let stored = get_reward_transaction(&env, tx_id);
        assert!(stored.is_some());
        let stored = stored.unwrap();
        assert_eq!(stored.amount, 500_000);
        assert_eq!(stored.status, RewardStatus::Claimed);
    });
}

#[test]
fn test_debit_reward_scalar_storage_matches_account() {
    let (env, admin, user, client) = setup_with_user();
    let contract_id = client.address.clone();
    client.credit_reward(&user, &1_000_000, &RewardType::SpendingLimit);
    client.debit_reward(&user, &300_000, &RewardType::SpendingLimit);
    let account = client.get_account(&user).unwrap();
    env.as_contract(&contract_id, || {
        assert_eq!(get_reward_balance(&env, &user), account.balance);
        assert_eq!(get_lifetime_claimed(&env, &user), account.lifetime_claimed);
    });
}

#[test]
fn test_debit_reward_updates_last_updated() {
    let (_env, admin, user, client) = setup_with_user();
    client.credit_reward(&user, &1_000_000, &RewardType::Referral);
    let before = client.get_account(&user).unwrap().last_updated;
    client.debit_reward(&user, &500_000, &RewardType::Referral);
    let after = client.get_account(&user).unwrap().last_updated;
    assert!(after >= before);
}

#[test]
fn test_debit_reward_multiple_users_are_independent() {
    let (env, admin, client) = setup();
    client.initialize(&admin);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);
    client.register_account(&user_a);
    client.register_account(&user_b);

    client.credit_reward(&user_a, &1_000_000, &RewardType::Streak);
    client.credit_reward(&user_b, &2_000_000, &RewardType::Streak);
    client.debit_reward(&user_a, &400_000, &RewardType::Streak);

    let a = client.get_account(&user_a).unwrap();
    let b = client.get_account(&user_b).unwrap();
    assert_eq!(a.balance, 600_000);
    assert_eq!(b.balance, 2_000_000);
}

#[test]
#[should_panic]
fn test_debit_reward_overdraft_panics() {
    let (_env, admin, user, client) = setup_with_user();
    client.credit_reward(&user, &500_000, &RewardType::Streak);
    client.debit_reward(&user, &500_001, &RewardType::Streak);
}

#[test]
#[should_panic]
fn test_debit_reward_zero_amount_panics() {
    let (_env, admin, user, client) = setup_with_user();
    client.credit_reward(&user, &1_000_000, &RewardType::Streak);
    client.debit_reward(&user, &0, &RewardType::Streak);
}

#[test]
#[should_panic]
fn test_debit_reward_negative_amount_panics() {
    let (_env, admin, user, client) = setup_with_user();
    client.credit_reward(&user, &1_000_000, &RewardType::Streak);
    client.debit_reward(&user, &-1, &RewardType::Streak);
}

#[test]
#[should_panic]
fn test_debit_reward_unregistered_account_panics() {
    let (env, admin, client) = setup();
    client.initialize(&admin);
    let stranger = Address::generate(&env);
    client.debit_reward(&stranger, &1_000, &RewardType::Streak);
}

#[test]
#[should_panic]
fn test_debit_reward_before_init_panics() {
    let (env, admin, client) = setup();
    let user = Address::generate(&env);
    client.debit_reward(&user, &1_000, &RewardType::Streak);
}

// ── Data model tests (#877) ───────────────────────────────────────────────────

#[test]
fn test_reward_type_variants_are_distinct() {
    assert_ne!(RewardType::SpendingLimit, RewardType::SavingsGoal);
    assert_ne!(RewardType::SavingsGoal, RewardType::Streak);
    assert_ne!(RewardType::Streak, RewardType::Referral);
    assert_ne!(RewardType::Referral, RewardType::ManualGrant);
}

#[test]
fn test_reward_type_clone() {
    let rt = RewardType::SavingsGoal;
    let cloned = rt.clone();
    assert_eq!(rt, cloned);
}

#[test]
fn test_reward_status_variants_are_distinct() {
    assert_ne!(RewardStatus::Pending, RewardStatus::Confirmed);
    assert_ne!(RewardStatus::Confirmed, RewardStatus::Claimed);
    assert_ne!(RewardStatus::Claimed, RewardStatus::Cancelled);
    assert_ne!(RewardStatus::Pending, RewardStatus::Cancelled);
}

#[test]
fn test_reward_status_clone() {
    let s = RewardStatus::Confirmed;
    assert_eq!(s.clone(), RewardStatus::Confirmed);
}

#[test]
fn test_reward_status_pending_is_not_claimed() {
    let status = RewardStatus::Pending;
    assert_ne!(status, RewardStatus::Claimed);
}

#[test]
fn test_reward_transaction_fields_are_correct() {
    let env = Env::default();
    let recipient = Address::generate(&env);

    let tx = RewardTransaction {
        id: 42,
        recipient: recipient.clone(),
        amount: 1_000_000,
        reward_type: RewardType::Streak,
        status: RewardStatus::Confirmed,
        created_at: 500,
        updated_at: 600,
    };

    assert_eq!(tx.id, 42);
    assert_eq!(tx.recipient, recipient);
    assert_eq!(tx.amount, 1_000_000);
    assert_eq!(tx.reward_type, RewardType::Streak);
    assert_eq!(tx.status, RewardStatus::Confirmed);
    assert_eq!(tx.created_at, 500);
    assert_eq!(tx.updated_at, 600);
}

#[test]
fn test_reward_transaction_clone() {
    let env = Env::default();
    let recipient = Address::generate(&env);

    let tx = RewardTransaction {
        id: 1,
        recipient: recipient.clone(),
        amount: 500_000,
        reward_type: RewardType::Referral,
        status: RewardStatus::Pending,
        created_at: 100,
        updated_at: 0,
    };

    let cloned = tx.clone();
    assert_eq!(cloned.id, tx.id);
    assert_eq!(cloned.amount, tx.amount);
    assert_eq!(cloned.reward_type, RewardType::Referral);
    assert_eq!(cloned.status, RewardStatus::Pending);
    assert_eq!(cloned.updated_at, 0);
}

#[test]
fn test_reward_transaction_status_transition() {
    let env = Env::default();
    let recipient = Address::generate(&env);

    let mut tx = RewardTransaction {
        id: 10,
        recipient: recipient.clone(),
        amount: 250_000,
        reward_type: RewardType::ManualGrant,
        status: RewardStatus::Pending,
        created_at: 200,
        updated_at: 0,
    };

    assert_eq!(tx.status, RewardStatus::Pending);
    tx.status = RewardStatus::Confirmed;
    assert_eq!(tx.status, RewardStatus::Confirmed);
    tx.status = RewardStatus::Claimed;
    assert_eq!(tx.status, RewardStatus::Claimed);
}

#[test]
fn test_all_reward_types_can_be_used_in_transaction() {
    let env = Env::default();
    let recipient = Address::generate(&env);

    let types = [
        RewardType::SpendingLimit,
        RewardType::SavingsGoal,
        RewardType::Streak,
        RewardType::Referral,
        RewardType::ManualGrant,
    ];

    for reward_type in types {
        let tx = RewardTransaction {
            id: 1,
            recipient: recipient.clone(),
            amount: 100,
            reward_type: reward_type.clone(),
            status: RewardStatus::Pending,
            created_at: 1,
            updated_at: 0,
        };
        assert_eq!(tx.reward_type, reward_type);
    }
}

// ── Reward Ledger Index tests (#873) ──────────────────────────────────────────

#[test]
fn test_get_transactions_for_empty_before_any_credit() {
    let (env, admin, client) = setup();
    client.initialize(&admin);
    let user = Address::generate(&env);
    client.register_account(&user);
    let ids = client.get_transactions_for(&user);
    assert_eq!(ids.len(), 0);
}

#[test]
fn test_get_transactions_for_returns_empty_for_unregistered() {
    let (env, admin, client) = setup();
    client.initialize(&admin);
    let stranger = Address::generate(&env);
    let ids = client.get_transactions_for(&stranger);
    assert_eq!(ids.len(), 0);
}

#[test]
fn test_get_transactions_for_appends_after_credit() {
    let (_env, admin, user, client) = setup_with_user();
    client.credit_reward(&user, &100, &RewardType::Streak);
    let ids = client.get_transactions_for(&user);
    assert_eq!(ids.len(), 1);
    assert_eq!(ids.get(0).unwrap(), 0u64);
}

#[test]
fn test_get_transactions_for_multiple_credits() {
    let (_env, admin, user, client) = setup_with_user();
    client.credit_reward(&user, &100, &RewardType::Streak);
    client.credit_reward(&user, &200, &RewardType::Referral);
    client.credit_reward(&user, &300, &RewardType::ManualGrant);
    let ids = client.get_transactions_for(&user);
    assert_eq!(ids.len(), 3);
    assert_eq!(ids.get(0).unwrap(), 0u64);
    assert_eq!(ids.get(1).unwrap(), 1u64);
    assert_eq!(ids.get(2).unwrap(), 2u64);
}

#[test]
fn test_get_transactions_for_index_matches_stored_transactions() {
    let (env, admin, user, client) = setup_with_user();
    let contract_id = client.address.clone();
    client.credit_reward(&user, &500, &RewardType::SavingsGoal);
    client.credit_reward(&user, &750, &RewardType::SpendingLimit);

    let ids = client.get_transactions_for(&user);
    env.as_contract(&contract_id, || {
        for i in 0..ids.len() {
            let tx_id = ids.get(i).unwrap();
            let tx = get_reward_transaction(&env, tx_id).expect("tx should exist");
            assert_eq!(tx.id, tx_id);
            assert_eq!(tx.recipient, user);
        }
    });
}

#[test]
fn test_get_transactions_for_users_are_independent() {
    let (env, admin, client) = setup();
    client.initialize(&admin);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);
    client.register_account(&user_a);
    client.register_account(&user_b);

    client.credit_reward(&user_a, &100, &RewardType::Streak);
    client.credit_reward(&user_b, &200, &RewardType::Streak);
    client.credit_reward(&user_a, &300, &RewardType::Streak);

    let ids_a = client.get_transactions_for(&user_a);
    let ids_b = client.get_transactions_for(&user_b);

    assert_eq!(ids_a.len(), 2);
    assert_eq!(ids_b.len(), 1);
}

#[test]
fn test_reward_index_storage_helper_directly() {
    let env = Env::default();
    env.mock_all_auths();
    let user = Address::generate(&env);
    let contract_id = env.register(RewardsContract, ());
    env.as_contract(&contract_id, || {
        let empty = get_reward_index(&env, &user);
        assert_eq!(empty.len(), 0);
    });
}

// ── RewardTransaction / TxCounter storage helpers (#877) ─────────────────────

use crate::storage::{set_reward_transaction, set_reward_tx_counter};

#[test]
fn test_get_reward_transaction_returns_none_for_missing_id() {
    let env = Env::default();
    let contract_id = env.register(RewardsContract, ());
    env.as_contract(&contract_id, || {
        assert!(get_reward_transaction(&env, 0).is_none());
        assert!(get_reward_transaction(&env, 999).is_none());
    });
}

#[test]
fn test_set_and_get_reward_transaction() {
    let env = Env::default();
    let recipient = Address::generate(&env);
    let contract_id = env.register(RewardsContract, ());
    env.as_contract(&contract_id, || {
        let tx = RewardTransaction {
            id: 7,
            recipient: recipient.clone(),
            amount: 1_500_000,
            reward_type: RewardType::SavingsGoal,
            status: RewardStatus::Confirmed,
            created_at: 300,
            updated_at: 400,
        };
        set_reward_transaction(&env, 7, &tx);
        let loaded = get_reward_transaction(&env, 7).expect("must exist after set");
        assert_eq!(loaded.id, 7);
        assert_eq!(loaded.recipient, recipient);
        assert_eq!(loaded.amount, 1_500_000);
        assert_eq!(loaded.reward_type, RewardType::SavingsGoal);
        assert_eq!(loaded.status, RewardStatus::Confirmed);
        assert_eq!(loaded.created_at, 300);
        assert_eq!(loaded.updated_at, 400);
    });
}

#[test]
fn test_reward_transaction_overwrite_updates_status() {
    let env = Env::default();
    let recipient = Address::generate(&env);
    let contract_id = env.register(RewardsContract, ());
    env.as_contract(&contract_id, || {
        let tx = RewardTransaction {
            id: 1,
            recipient: recipient.clone(),
            amount: 500_000,
            reward_type: RewardType::Streak,
            status: RewardStatus::Pending,
            created_at: 100,
            updated_at: 0,
        };
        set_reward_transaction(&env, 1, &tx);
        assert_eq!(
            get_reward_transaction(&env, 1).unwrap().status,
            RewardStatus::Pending
        );
        let updated = RewardTransaction {
            status: RewardStatus::Claimed,
            updated_at: 200,
            ..tx
        };
        set_reward_transaction(&env, 1, &updated);
        let loaded = get_reward_transaction(&env, 1).unwrap();
        assert_eq!(loaded.status, RewardStatus::Claimed);
        assert_eq!(loaded.updated_at, 200);
    });
}

#[test]
fn test_multiple_reward_transactions_are_independent() {
    let env = Env::default();
    let addr_a = Address::generate(&env);
    let addr_b = Address::generate(&env);
    let contract_id = env.register(RewardsContract, ());
    env.as_contract(&contract_id, || {
        let tx_a = RewardTransaction {
            id: 0,
            recipient: addr_a.clone(),
            amount: 100,
            reward_type: RewardType::SpendingLimit,
            status: RewardStatus::Confirmed,
            created_at: 1,
            updated_at: 0,
        };
        let tx_b = RewardTransaction {
            id: 1,
            recipient: addr_b.clone(),
            amount: 200,
            reward_type: RewardType::Referral,
            status: RewardStatus::Pending,
            created_at: 2,
            updated_at: 0,
        };
        set_reward_transaction(&env, 0, &tx_a);
        set_reward_transaction(&env, 1, &tx_b);
        let a = get_reward_transaction(&env, 0).unwrap();
        let b = get_reward_transaction(&env, 1).unwrap();
        assert_eq!(a.recipient, addr_a);
        assert_eq!(a.amount, 100);
        assert_eq!(b.recipient, addr_b);
        assert_eq!(b.amount, 200);
        assert_ne!(a.reward_type, b.reward_type);
    });
}

#[test]
fn test_reward_tx_counter_defaults_to_zero() {
    let env = Env::default();
    let contract_id = env.register(RewardsContract, ());
    env.as_contract(&contract_id, || {
        assert_eq!(get_reward_tx_counter(&env), 0);
    });
}

#[test]
fn test_set_and_get_reward_tx_counter() {
    let env = Env::default();
    let contract_id = env.register(RewardsContract, ());
    env.as_contract(&contract_id, || {
        set_reward_tx_counter(&env, 42);
        assert_eq!(get_reward_tx_counter(&env), 42);
        set_reward_tx_counter(&env, 0);
        assert_eq!(get_reward_tx_counter(&env), 0);
    });
}

#[test]
fn test_reward_tx_counter_increments_correctly() {
    let env = Env::default();
    let contract_id = env.register(RewardsContract, ());
    env.as_contract(&contract_id, || {
        for i in 0u64..5 {
            assert_eq!(get_reward_tx_counter(&env), i);
            set_reward_tx_counter(&env, i + 1);
        }
        assert_eq!(get_reward_tx_counter(&env), 5);
    });
}

// ── Metadata tests ─────────────────────────────────────────────────────────────

#[test]
fn test_metadata_initialized_correctly() {
    let (env, admin, client) = setup();
    client.initialize(&admin);
    let user = Address::generate(&env);

    client.register_account(&user);

    let account = client.get_account(&user).expect("account must exist");
    assert_eq!(account.created_at, env.ledger().timestamp());
    assert_eq!(account.last_updated, env.ledger().timestamp());
    assert_eq!(account.account_status, AccountStatus::Active);
    assert_eq!(account.metadata_version, DEFAULT_METADATA_VERSION);

    assert_eq!(
        client.get_account_status(&user),
        Some(AccountStatus::Active)
    );
    assert_eq!(
        client.get_metadata_version(&user),
        Some(DEFAULT_METADATA_VERSION)
    );
}

#[test]
fn test_metadata_updates_automatically_and_queries_expose_it() {
    let (env, admin, client) = setup();
    client.initialize(&admin);
    let user = Address::generate(&env);

    client.register_account(&user);
    let initial_acc = client.get_account(&user).unwrap();

    // Fast-forward timestamp
    env.ledger().set_timestamp(env.ledger().timestamp() + 500);

    // Credit reward -> automatically updates last_updated timestamp
    client.credit_reward(&user, &1_000, &RewardType::ManualGrant);

    let updated_acc = client.get_account(&user).unwrap();
    assert_eq!(updated_acc.created_at, initial_acc.created_at);
    assert_eq!(updated_acc.last_updated, initial_acc.last_updated + 500);

    // Update account status -> updates last_updated timestamp
    env.ledger().set_timestamp(env.ledger().timestamp() + 300);
    client.set_account_status(&user, &AccountStatus::Suspended);

    assert_eq!(
        client.get_account_status(&user),
        Some(AccountStatus::Suspended)
    );
    let suspended_acc = client.get_account(&user).unwrap();
    assert_eq!(suspended_acc.account_status, AccountStatus::Suspended);
    assert_eq!(suspended_acc.last_updated, updated_acc.last_updated + 300);
}

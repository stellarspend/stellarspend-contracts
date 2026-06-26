#![cfg(test)]

use super::{Beneficiary, BudgetContract, BudgetContractClient};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env, Symbol};

fn setup() -> (Env, Address, Address, BudgetContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(BudgetContract, ());
    let client = BudgetContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin);
    (env, admin, user, client)
}

fn setup_categories(
    client: &BudgetContractClient,
    admin: &Address,
    user: &Address,
) -> (Symbol, Symbol) {
    let food = symbol_short!("food");
    let travel = symbol_short!("travel");
    client.set_category_budget(admin, user, &food, &1_000);
    client.set_category_budget(admin, user, &travel, &500);
    (food, travel)
}

#[test]
fn test_initialize() {
    let (_, admin, _, client) = setup();
    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_suspicious_activity_count(), 0);
}

#[test]
fn test_transfer_between_categories() {
    let (_, admin, user, client) = setup();
    let (food, travel) = setup_categories(&client, &admin, &user);

    let transfer_id = client.transfer_between_categories(&user, &food, &travel, &200);

    assert_eq!(transfer_id, 1);
    assert_eq!(client.get_category_balance(&user, &food), 800);
    assert_eq!(client.get_category_balance(&user, &travel), 700);

    let history = client.get_transfer_history(&user);
    assert_eq!(history.len(), 1);
    assert_eq!(history.get(0).unwrap().amount, 200);
    assert_eq!(history.get(0).unwrap().from_category, food);
    assert_eq!(history.get(0).unwrap().to_category, travel);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_transfer_insufficient_balance() {
    let (_, admin, user, client) = setup();
    let (food, travel) = setup_categories(&client, &admin, &user);
    client.transfer_between_categories(&user, &food, &travel, &2_000);
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_transfer_same_category() {
    let (_, admin, user, client) = setup();
    let (food, _) = setup_categories(&client, &admin, &user);
    client.transfer_between_categories(&user, &food, &food, &100);
}

#[test]
fn test_spend_and_remaining_balance() {
    let (_, admin, user, client) = setup();
    let (food, _) = setup_categories(&client, &admin, &user);

    let remaining = client.spend_from_category(&user, &food, &300);
    assert_eq!(remaining, 700);
    assert_eq!(client.get_category_balance(&user, &food), 700);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_rapid_spending_triggers_freeze() {
    let (_, admin, user, client) = setup();
    let (food, _) = setup_categories(&client, &admin, &user);

    client.spend_from_category(&user, &food, &10);
    client.spend_from_category(&user, &food, &10);
    client.spend_from_category(&user, &food, &10);
    client.spend_from_category(&user, &food, &10);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_frozen_budget_blocks_transfer() {
    let (_, admin, user, client) = setup();
    let (food, travel) = setup_categories(&client, &admin, &user);

    client.spend_from_category(&user, &food, &10);
    client.spend_from_category(&user, &food, &10);
    client.spend_from_category(&user, &food, &10);

    assert!(client.is_frozen(&user));
    client.transfer_between_categories(&user, &food, &travel, &50);
}

#[test]
fn test_manual_unfreeze() {
    let (_, admin, user, client) = setup();
    let (food, _) = setup_categories(&client, &admin, &user);

    client.spend_from_category(&user, &food, &10);
    client.spend_from_category(&user, &food, &10);
    client.spend_from_category(&user, &food, &10);

    assert!(client.is_frozen(&user));
    client.unfreeze_budget(&admin, &user);
    assert!(!client.is_frozen(&user));

    let remaining = client.spend_from_category(&user, &food, &10);
    assert_eq!(remaining, 960);
}

#[test]
fn test_user_can_unfreeze_own_budget() {
    let (_, _, user, client) = setup();
    let admin = client.get_admin();
    let (food, _) = setup_categories(&client, &admin, &user);

    client.spend_from_category(&user, &food, &10);
    client.spend_from_category(&user, &food, &10);
    client.spend_from_category(&user, &food, &10);

    client.unfreeze_budget(&user, &user);
    assert!(!client.is_frozen(&user));
}

#[test]
fn test_archive_and_unarchive_budget() {
    let (_, admin, user, client) = setup();
    client.update_budget(&admin, &user, &1_000, &None);

    client.archive_budget(&admin, &user);
    assert!(client.get_budget(&user).is_none());

    let archived = client.get_archived_budget(&user).unwrap();
    assert_eq!(archived.amount, 1_000);
    assert!(archived.is_archived);

    client.unarchive_budget(&admin, &user);
    let restored = client.get_budget(&user).unwrap();
    assert_eq!(restored.amount, 1_000);
    assert!(!restored.is_archived);
}

#[test]
fn test_budget_recovery() {
    let (_, admin, user, client) = setup();
    let (food, travel) = setup_categories(&client, &admin, &user);

    // Initial state (sum = 1500)
    assert_eq!(client.get_category_balance(&user, &food), 1_000);
    assert_eq!(client.get_category_balance(&user, &travel), 500);

    // Create checkpoint
    client.create_recovery_checkpoint(&user);

    // Modify budget
    client.set_category_budget(&admin, &user, &food, &2_000);
    assert_eq!(client.get_category_balance(&user, &food), 2_000);

    // Restore from checkpoint
    client.restore_budget_from_checkpoint(&user);

    // Verify restored state (restored to 'default' category with sum of 1500)
    let default_cat = symbol_short!("default");
    assert_eq!(client.get_category_balance(&user, &default_cat), 1_500);
    
    // Original categories should be cleared as the whole UserBudget was replaced
    // Testing this behavior depends on get_category_balance panic behavior
}

#[test]
#[should_panic(expected = "Error(Contract, #16)")]
fn test_restore_without_checkpoint() {
    let (_, _, user, client) = setup();
    client.restore_budget_from_checkpoint(&user);
}

#[test]
fn test_transfer_history_preserved() {
    let (_, admin, user, client) = setup();
    let (food, travel) = setup_categories(&client, &admin, &user);

    client.transfer_between_categories(&user, &food, &travel, &100);
    client.transfer_between_categories(&user, &travel, &food, &50);

    let history = client.get_transfer_history(&user);
    assert_eq!(history.len(), 2);

    let first = client.get_transfer(&1);
    assert_eq!(first.amount, 100);
    let second = client.get_transfer(&2);
    assert_eq!(second.amount, 50);
}

#[test]
fn test_inactivity_timeout_and_ownership_transfer() {
    let (env, admin, owner, client) = setup();
    let beneficiary = Address::generate(&env);

    client.update_budget(&admin, &owner, &1_000, &None);
    client.set_category_budget(&admin, &owner, &symbol_short!("food"), &1_000);

    let inheritance = soroban_sdk::vec![&env, beneficiary.clone()];
    client.set_inheritance_bens(&owner, &inheritance);

    client.set_inactivity_timeout(&owner, &10);

    // Let's advance ledger timestamp by 15 seconds
    env.ledger().set_timestamp(15);

    client.claim_ownership(&beneficiary, &owner);

    // Ownership should be transferred
    assert_eq!(client.get_budget(&beneficiary).unwrap().amount, 1_000);
    assert!(client.get_budget(&owner).is_none());

    // Category should be transferred
    assert_eq!(
        client.get_category_balance(&beneficiary, &symbol_short!("food")),
        1_000
    );
}

#[test]
fn test_distribute_remaining_funds() {
    let (env, admin, owner, client) = setup();
    let beneficiary1 = Address::generate(&env);
    let beneficiary2 = Address::generate(&env);

    client.update_budget(&admin, &owner, &1_000, &None);

    let beneficiaries = soroban_sdk::vec![
        &env,
        Beneficiary {
            address: beneficiary1.clone(),
            percentage: 60,
        },
        Beneficiary {
            address: beneficiary2.clone(),
            percentage: 40,
        }
    ];

    client.register_beneficiaries(&owner, &beneficiaries);
    client.set_inactivity_timeout(&owner, &10);

    // Advance timestamp
    env.ledger().set_timestamp(15);

    client.distribute_remaining_funds(&beneficiary1, &owner);

    // Check balances/budgets
    assert_eq!(client.get_budget(&beneficiary1).unwrap().amount, 600);
    assert_eq!(client.get_budget(&beneficiary2).unwrap().amount, 400);
    assert!(client.get_budget(&owner).is_none());
}

#[test]
#[should_panic(expected = "Error(Contract, #15)")]
fn test_register_beneficiaries_invalid_percentages() {
    let (env, _, owner, client) = setup();
    let beneficiary1 = Address::generate(&env);
    let beneficiary2 = Address::generate(&env);

    let beneficiaries = soroban_sdk::vec![
        &env,
        Beneficiary {
            address: beneficiary1,
            percentage: 50,
        },
        Beneficiary {
            address: beneficiary2,
            percentage: 40, // 50 + 40 = 90 (not 100)
        }
    ];

    client.register_beneficiaries(&owner, &beneficiaries);
}

// ── Budget config history versioning (issue #621) ────────────────────────────

#[test]
fn test_budget_history_records_version_on_set_category() {
    let (_, admin, user, client) = setup();
    let food = symbol_short!("food");

    // No history yet
    assert_eq!(client.get_budget_history(&user).len(), 0);

    client.set_category_budget(&admin, &user, &food, &1_000);
    let history = client.get_budget_history(&user);
    assert_eq!(history.len(), 1);
    assert_eq!(history.get(0).unwrap().version, 1);
}

#[test]
fn test_budget_history_version_increments() {
    let (_, admin, user, client) = setup();
    let food = symbol_short!("food");
    let travel = symbol_short!("travel");

    client.set_category_budget(&admin, &user, &food, &1_000);
    client.set_category_budget(&admin, &user, &travel, &500);
    client.set_category_budget(&admin, &user, &food, &2_000);

    let history = client.get_budget_history(&user);
    assert_eq!(history.len(), 3);
    assert_eq!(history.get(0).unwrap().version, 1);
    assert_eq!(history.get(1).unwrap().version, 2);
    assert_eq!(history.get(2).unwrap().version, 3);
}

#[test]
fn test_budget_history_timestamps_recorded() {
    let (env, admin, user, client) = setup();
    let food = symbol_short!("food");

    env.ledger().set_timestamp(100);
    client.set_category_budget(&admin, &user, &food, &1_000);

    env.ledger().set_timestamp(200);
    client.set_category_budget(&admin, &user, &food, &2_000);

    let history = client.get_budget_history(&user);
    assert_eq!(history.len(), 2);
    assert_eq!(history.get(0).unwrap().updated_at, 100);
    assert_eq!(history.get(1).unwrap().updated_at, 200);
}

#[test]
fn test_get_budget_version_retrieves_correct_snapshot() {
    let (_, admin, user, client) = setup();
    let food = symbol_short!("food");

    client.set_category_budget(&admin, &user, &food, &1_000);
    client.set_category_budget(&admin, &user, &food, &2_000);

    let v1 = client.get_budget_version(&user, &1);
    assert_eq!(v1.version, 1);
    assert_eq!(v1.categories.get(food.clone()).unwrap().limit, 1_000);

    let v2 = client.get_budget_version(&user, &2);
    assert_eq!(v2.version, 2);
    assert_eq!(v2.categories.get(food).unwrap().limit, 2_000);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_get_budget_version_not_found_panics() {
    let (_, _, user, client) = setup();
    client.get_budget_version(&user, &99);
}

#[test]
fn test_budget_history_queryable_after_multiple_changes() {
    let (_, admin, user, client) = setup();
    let food = symbol_short!("food");
    let travel = symbol_short!("travel");
    let rent = symbol_short!("rent");

    client.set_category_budget(&admin, &user, &food, &500);
    client.set_category_budget(&admin, &user, &travel, &300);
    client.set_category_budget(&admin, &user, &rent, &1_200);
    client.set_category_budget(&admin, &user, &food, &800);

    let history = client.get_budget_history(&user);
    assert_eq!(history.len(), 4);

    // Latest version should reflect food=800, travel=300, rent=1200
    let latest = history.get(3).unwrap();
    assert_eq!(latest.categories.get(food).unwrap().limit, 800);
    assert_eq!(latest.categories.get(travel).unwrap().limit, 300);
    assert_eq!(latest.categories.get(rent).unwrap().limit, 1_200);
}

// ── Budget expiration logic ───────────────────────────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #18)")]
fn test_budget_expired_panic() {
    let (env, admin, user, client) = setup();
    let (food, _) = setup_categories(&client, &admin, &user);

    client.update_budget(&admin, &user, &1_000, &None);
    client.configure_expiration(&admin, &user, &100);

    env.ledger().set_timestamp(105);
    client.spend_from_category(&user, &food, &10);
}

#[test]
#[should_panic(expected = "Error(Contract, #19)")]
fn test_budget_inactive_panic() {
    let (_, admin, user, client) = setup();
    let (food, _) = setup_categories(&client, &admin, &user);

    client.update_budget(&admin, &user, &1_000, &None);
    client.mark_inactive(&admin, &user);

    client.spend_from_category(&user, &food, &10);
}

#[test]
#[should_panic(expected = "Error(Contract, #19)")]
fn test_transfer_with_inactive_budget() {
    let (env, admin, user, client) = setup();
    let (food, travel) = setup_categories(&client, &admin, &user);

    client.update_budget(&admin, &user, &1_000, &None);
    client.configure_expiration(&admin, &user, &100);

    env.ledger().set_timestamp(105);
    client.deactivate_if_expired(&user);

    client.transfer_between_categories(&user, &food, &travel, &10);
}

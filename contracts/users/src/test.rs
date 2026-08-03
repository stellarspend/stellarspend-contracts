use crate::{UsersContract, UsersContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

#[test]
fn test_initialize_and_get_admin() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register(UsersContract, ());
    let client = UsersContractClient::new(&env, &contract_id);

    // Test initialization
    client.initialize(&admin);

    // Verify admin is set
    assert_eq!(client.get_admin(), Some(admin.clone()));
}

#[test]
#[should_panic]
fn test_initialize_duplicate_fails() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register(UsersContract, ());
    let client = UsersContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let admin2 = Address::generate(&env);
    client.initialize(&admin2);
}

#[test]
fn test_register_user_and_count() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register(UsersContract, ());
    let client = UsersContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let user3 = Address::generate(&env);

    // Test initial count is 0
    assert_eq!(client.get_all_users_count(), 0);

    // Register first user
    let is_new1 = client.register_user(&user1);
    assert!(is_new1);
    assert_eq!(client.get_all_users_count(), 1);
    assert!(client.is_user_registered(&user1));

    // Register second user
    let is_new2 = client.register_user(&user2);
    assert!(is_new2);
    assert_eq!(client.get_all_users_count(), 2);
    assert!(client.is_user_registered(&user2));

    // Register third user
    let is_new3 = client.register_user(&user3);
    assert!(is_new3);
    assert_eq!(client.get_all_users_count(), 3);
    assert!(client.is_user_registered(&user3));

    // Test duplicate registration (should not increase count)
    let is_duplicate = client.register_user(&user1);
    assert!(!is_duplicate);
    assert_eq!(client.get_all_users_count(), 3);
}

#[test]
fn test_get_all_users_admin_only() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let contract_id = env.register(UsersContract, ());
    let client = UsersContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    // Register some users
    client.register_user(&user);

    // Test admin can get all users
    let all_users = client.get_all_users(&admin);
    assert_eq!(all_users.len(), 1);
    assert_eq!(all_users.get(0), Some(user));
}

#[test]
#[should_panic]
fn test_get_all_users_non_admin_fails() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let contract_id = env.register(UsersContract, ());
    let client = UsersContractClient::new(&env, &contract_id);

    client.initialize(&admin);
    client.register_user(&user);

    client.get_all_users(&user);
}

#[test]
fn test_user_exists_functionality() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register(UsersContract, ());
    let client = UsersContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    // Test non-existent user
    assert!(!client.is_user_registered(&user1));
    assert!(!client.is_user_registered(&user2));

    // Register user1
    client.register_user(&user1);

    // Test user1 exists, user2 doesn't
    assert!(client.is_user_registered(&user1));
    assert!(!client.is_user_registered(&user2));

    // Register user2
    client.register_user(&user2);

    // Test both users exist
    assert!(client.is_user_registered(&user1));
    assert!(client.is_user_registered(&user2));
}

#[test]
fn test_reset_user_data() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register(UsersContract, ());
    let client = UsersContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let user = Address::generate(&env);

    client.register_user(&user);
    assert!(client.is_user_registered(&user));
    assert_eq!(client.get_all_users_count(), 1);

    let success = client.reset_user_data(&user);
    assert!(success);
    assert!(!client.is_user_registered(&user));
    assert_eq!(client.get_all_users_count(), 0);

    // Resetting again should return false because the user is no longer registered
    let result = client.reset_user_data(&user);
    assert!(!result);
}

// ── Issue #336: check_user_exists ────────────────────────────────────────────

#[test]
fn test_check_user_exists_returns_false_for_unregistered() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let _contract_id = env.register(UsersContract, ());

    UsersContract::initialize(env.clone(), admin.clone());

    let stranger = Address::generate(&env);
    assert!(!UsersContract::check_user_exists(env.clone(), stranger));
}

#[test]
fn test_check_user_exists_returns_true_after_registration() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let _contract_id = env.register(UsersContract, ());

    UsersContract::initialize(env.clone(), admin.clone());

    let user = Address::generate(&env);
    // Before registration → false
    assert!(!UsersContract::check_user_exists(env.clone(), user.clone()));

    // After registration → true
    UsersContract::register_user(env.clone(), user.clone());
    assert!(UsersContract::check_user_exists(env.clone(), user.clone()));
}

#[test]
fn test_check_user_exists_matches_is_user_registered() {
    // check_user_exists is a deliberate alias for is_user_registered.
    // This test enforces parity so any future divergence is caught.
    let env = Env::default();
    let admin = Address::generate(&env);
    let _contract_id = env.register(UsersContract, ());

    UsersContract::initialize(env.clone(), admin.clone());

    let registered = Address::generate(&env);
    let unregistered = Address::generate(&env);

    UsersContract::register_user(env.clone(), registered.clone());

    // Both functions must agree on a registered user
    assert_eq!(
        UsersContract::check_user_exists(env.clone(), registered.clone()),
        UsersContract::is_user_registered(env.clone(), registered.clone()),
    );

    // Both functions must agree on an unregistered user
    assert_eq!(
        UsersContract::check_user_exists(env.clone(), unregistered.clone()),
        UsersContract::is_user_registered(env.clone(), unregistered.clone()),
    );
}

#[test]
fn test_multiple_unique_users() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register(UsersContract, ());
    let client = UsersContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let mut users = Vec::new(&env);

    // Create and register 10 unique users
    for _ in 0..10 {
        let user = Address::generate(&env);
        users.push_back(user.clone());
        client.register_user(&user);
    }

    // Verify count matches
    assert_eq!(client.get_all_users_count(), 10);

    // Verify all users are registered
    for i in 0..10 {
        let user = users.get(i).unwrap();
        assert!(client.is_user_registered(&user));
    }
}

#[test]
fn test_set_and_get_default_currency() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register(UsersContract, ());
    let client = UsersContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let user = Address::generate(&env);
    client.register_user(&user);

    let currency = String::from_str(&env, "USD");
    client.set_default_currency(&user, &currency);

    assert_eq!(client.get_default_currency(&user), Some(currency));
}

#[test]
#[should_panic]
fn test_set_default_currency_fails_for_unregistered_user() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register(UsersContract, ());
    let client = UsersContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let user = Address::generate(&env);
    let currency = String::from_str(&env, "USD");

    client.set_default_currency(&user, &currency);
}

#[test]
fn test_deactivate_user_marks_user_inactive() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register(UsersContract, ());
    let client = UsersContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let user = Address::generate(&env);
    client.register_user(&user);
    assert!(client.is_user_active(&user));

    let success = client.deactivate_user(&user);
    assert!(success);
    assert!(!client.is_user_active(&user));
}

#[test]
fn test_get_user_settings_empty_state() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(UsersContract, ());
    let client = UsersContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    // User has never been registered — empty state should return defaults
    let stranger = Address::generate(&env);
    let settings = client.get_user_settings(&stranger);

    assert!(!settings.is_active);
    assert_eq!(settings.currency, String::from_str(&env, "USD"));
    assert!(settings.default_currency.is_none());
    assert!(settings.last_login.is_none());
    assert!(settings.nickname.is_none());
}

#[test]
fn test_get_user_settings_returns_saved_values() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(UsersContract, ());
    let client = UsersContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let user = Address::generate(&env);
    client.register_user(&user);

    let currency = String::from_str(&env, "EUR");
    let nickname = String::from_str(&env, "alice");

    client.set_user_currency(&user, &currency);
    client.set_default_currency(&user, &currency);
    client.update_nickname(&user, &nickname);

    let settings = client.get_user_settings(&user);

    assert!(settings.is_active);
    assert_eq!(settings.currency, currency);
    assert_eq!(settings.default_currency, Some(currency));
    assert!(settings.last_login.is_some());
    assert_eq!(settings.nickname, Some(nickname));
}

// ── Issue #997: get_users_count view ─────────────────────────────────────────

#[test]
fn get_users_count_returns_registered_user_total() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register(UsersContract, ());
    let client = UsersContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let first_user = Address::generate(&env);
    let second_user = Address::generate(&env);

    assert_eq!(client.get_users_count(), 0);

    client.register_user(&first_user);
    assert_eq!(client.get_users_count(), 1);

    client.register_user(&second_user);
    assert_eq!(client.get_users_count(), 2);

    // Duplicate registration does not increase count.
    client.register_user(&first_user);
    assert_eq!(client.get_users_count(), 2);
}

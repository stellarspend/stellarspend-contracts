#![cfg(test)]

use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env, String, Vec};

use merchant_tagging::MerchantTaggingContractClient;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn setup() -> (Env, Address, MerchantTaggingContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, merchant_tagging::MerchantTaggingContract);
    let client = MerchantTaggingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.init(&admin);
    (env, admin, client)
}

fn empty_tags(env: &Env) -> Vec<soroban_sdk::Symbol> {
    Vec::new(env)
}

fn make_tags(env: &Env, tags: &[&str]) -> Vec<soroban_sdk::Symbol> {
    let mut v = Vec::new(env);
    for t in tags {
        v.push_back(soroban_sdk::Symbol::new(env, t));
    }
    v
}

// ── Init tests ────────────────────────────────────────────────────────────────

#[test]
fn test_init_sets_admin() {
    let (_, admin, client) = setup();
    assert_eq!(client.get_admin(), admin);
}

#[test]
#[should_panic(expected = "already initialized")]
fn test_double_init_panics() {
    let (env, admin, client) = setup();
    let other = Address::generate(&env);
    client.init(&other); // second init should panic
}

// ── Merchant registration ─────────────────────────────────────────────────────

#[test]
fn test_register_merchant() {
    let (env, admin, client) = setup();
    let id = symbol_short!("AMAZON");
    let name = String::from_str(&env, "Amazon");
    let tags = make_tags(&env, &["retail", "ecommerce"]);

    client.register_merchant(&admin, &id, &name, &tags, &None);

    let merchant = client.get_merchant(&id).expect("merchant should exist");
    assert_eq!(merchant.id, id);
    assert_eq!(merchant.name, name);
    assert!(merchant.active);
    assert_eq!(merchant.tags.len(), 2);
}

#[test]
#[should_panic(expected = "merchant already exists")]
fn test_register_duplicate_merchant_panics() {
    let (env, admin, client) = setup();
    let id = symbol_short!("AMAZON");
    let name = String::from_str(&env, "Amazon");
    client.register_merchant(&admin, &id, &name, &empty_tags(&env), &None);
    client.register_merchant(&admin, &id, &name, &empty_tags(&env), &None);
}

#[test]
#[should_panic(expected = "unauthorized")]
fn test_register_merchant_non_admin_panics() {
    let (env, _, client) = setup();
    let non_admin = Address::generate(&env);
    let id = symbol_short!("AMAZON");
    let name = String::from_str(&env, "Amazon");
    client.register_merchant(&non_admin, &id, &name, &empty_tags(&env), &None);
}

#[test]
fn test_list_merchants() {
    let (env, admin, client) = setup();
    client.register_merchant(
        &admin,
        &symbol_short!("AMAZON"),
        &String::from_str(&env, "Amazon"),
        &empty_tags(&env),
        &None,
    );
    client.register_merchant(
        &admin,
        &symbol_short!("NETFLIX"),
        &String::from_str(&env, "Netflix"),
        &empty_tags(&env),
        &None,
    );

    let list = client.list_merchants();
    assert_eq!(list.len(), 2);
}

// ── Merchant update / deactivation ───────────────────────────────────────────

#[test]
fn test_update_merchant_name() {
    let (env, admin, client) = setup();
    let id = symbol_short!("AMAZON");
    client.register_merchant(
        &admin,
        &id,
        &String::from_str(&env, "Amazon"),
        &empty_tags(&env),
        &None,
    );

    let new_name = String::from_str(&env, "Amazon Inc.");
    client.update_merchant(&admin, &id, &Some(new_name.clone()), &None, &None);

    let merchant = client.get_merchant(&id).unwrap();
    assert_eq!(merchant.name, new_name);
}

#[test]
fn test_deactivate_merchant() {
    let (env, admin, client) = setup();
    let id = symbol_short!("AMAZON");
    client.register_merchant(
        &admin,
        &id,
        &String::from_str(&env, "Amazon"),
        &empty_tags(&env),
        &None,
    );

    client.deactivate_merchant(&admin, &id);

    let merchant = client.get_merchant(&id).unwrap();
    assert!(!merchant.active);
}

// ── Transaction tagging ───────────────────────────────────────────────────────

#[test]
fn test_tag_transaction() {
    let (env, admin, client) = setup();
    let merchant_id = symbol_short!("STARBUCKS");
    client.register_merchant(
        &admin,
        &merchant_id,
        &String::from_str(&env, "Starbucks"),
        &make_tags(&env, &["food", "coffee"]),
        &None,
    );

    let tagger = Address::generate(&env);
    let asset = symbol_short!("XLM");
    let note = String::from_str(&env, "Morning coffee");

    client.tag_transaction(&tagger, &1u64, &merchant_id, &500_000i128, &asset, &note);

    let tag = client
        .get_transaction_tag(&1u64, &merchant_id)
        .expect("tag should exist");
    assert_eq!(tag.tx_id, 1);
    assert_eq!(tag.amount, 500_000);
    assert_eq!(tag.merchant_id, merchant_id);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn test_tag_zero_amount_panics() {
    let (env, admin, client) = setup();
    let merchant_id = symbol_short!("STARBUCKS");
    client.register_merchant(
        &admin,
        &merchant_id,
        &String::from_str(&env, "Starbucks"),
        &empty_tags(&env),
        &None,
    );
    let tagger = Address::generate(&env);
    client.tag_transaction(
        &tagger,
        &1u64,
        &merchant_id,
        &0i128,
        &symbol_short!("XLM"),
        &String::from_str(&env, ""),
    );
}

#[test]
#[should_panic(expected = "merchant inactive")]
fn test_tag_inactive_merchant_panics() {
    let (env, admin, client) = setup();
    let merchant_id = symbol_short!("STARBUCKS");
    client.register_merchant(
        &admin,
        &merchant_id,
        &String::from_str(&env, "Starbucks"),
        &empty_tags(&env),
        &None,
    );
    client.deactivate_merchant(&admin, &merchant_id);

    let tagger = Address::generate(&env);
    client.tag_transaction(
        &tagger,
        &1u64,
        &merchant_id,
        &500_000i128,
        &symbol_short!("XLM"),
        &String::from_str(&env, ""),
    );
}

#[test]
#[should_panic(expected = "already tagged")]
fn test_duplicate_tag_panics() {
    let (env, admin, client) = setup();
    let merchant_id = symbol_short!("STARBUCKS");
    client.register_merchant(
        &admin,
        &merchant_id,
        &String::from_str(&env, "Starbucks"),
        &empty_tags(&env),
        &None,
    );
    let tagger = Address::generate(&env);
    let asset = symbol_short!("XLM");
    let note = String::from_str(&env, "");
    client.tag_transaction(
        &tagger,
        &1u64,
        &merchant_id,
        &500_000i128,
        &asset,
        &note.clone(),
    );
    client.tag_transaction(&tagger, &1u64, &merchant_id, &500_000i128, &asset, &note);
}

// ── Query functions ───────────────────────────────────────────────────────────

#[test]
fn test_get_merchant_transactions() {
    let (env, admin, client) = setup();
    let merchant_id = symbol_short!("AMAZON");
    client.register_merchant(
        &admin,
        &merchant_id,
        &String::from_str(&env, "Amazon"),
        &empty_tags(&env),
        &None,
    );

    let tagger = Address::generate(&env);
    let asset = symbol_short!("USDC");
    let note = String::from_str(&env, "");

    client.tag_transaction(
        &tagger,
        &10u64,
        &merchant_id,
        &1_000_000i128,
        &asset,
        &note.clone(),
    );
    client.tag_transaction(
        &tagger,
        &11u64,
        &merchant_id,
        &2_000_000i128,
        &asset,
        &note.clone(),
    );
    client.tag_transaction(&tagger, &12u64, &merchant_id, &3_000_000i128, &asset, &note);

    let txs = client.get_merchant_transactions(&merchant_id);
    assert_eq!(txs.len(), 3);
}

#[test]
fn test_merchant_analytics_accumulate() {
    let (env, admin, client) = setup();
    let merchant_id = symbol_short!("AMAZON");
    client.register_merchant(
        &admin,
        &merchant_id,
        &String::from_str(&env, "Amazon"),
        &empty_tags(&env),
        &None,
    );

    let tagger = Address::generate(&env);
    let asset = symbol_short!("XLM");
    let note = String::from_str(&env, "");

    client.tag_transaction(
        &tagger,
        &1u64,
        &merchant_id,
        &1_000i128,
        &asset,
        &note.clone(),
    );
    client.tag_transaction(&tagger, &2u64, &merchant_id, &2_000i128, &asset, &note);

    let analytics = client
        .get_merchant_analytics(&merchant_id)
        .expect("analytics should exist");
    assert_eq!(analytics.tx_count, 2);
    assert_eq!(analytics.total_volume, 3_000);
}

#[test]
fn test_get_merchants_by_tag() {
    let (env, admin, client) = setup();

    client.register_merchant(
        &admin,
        &symbol_short!("AMAZON"),
        &String::from_str(&env, "Amazon"),
        &make_tags(&env, &["retail"]),
        &None,
    );
    client.register_merchant(
        &admin,
        &symbol_short!("WALMART"),
        &String::from_str(&env, "Walmart"),
        &make_tags(&env, &["retail", "grocery"]),
        &None,
    );
    client.register_merchant(
        &admin,
        &symbol_short!("NETFLIX"),
        &String::from_str(&env, "Netflix"),
        &make_tags(&env, &["streaming"]),
        &None,
    );

    let retail_tag = soroban_sdk::Symbol::new(&env, "retail");
    let retail_merchants = client.get_merchants_by_tag(&retail_tag);
    assert_eq!(retail_merchants.len(), 2);

    let streaming_tag = soroban_sdk::Symbol::new(&env, "streaming");
    let streaming_merchants = client.get_merchants_by_tag(&streaming_tag);
    assert_eq!(streaming_merchants.len(), 1);
}

// ── Get merchant tag ──────────────────────────────────────────────────────────

#[test]
fn test_get_merchant_tag_returns_correct_tag() {
    let (env, admin, client) = setup();
    let merchant_address = Address::generate(&env);
    let id = symbol_short!("AMAZON");
    let name = String::from_str(&env, "Amazon");
    let tags = make_tags(&env, &["retail", "ecommerce"]);

    client.register_merchant(&admin, &id, &name, &tags, &Some(merchant_address.clone()));

    let tag = client
        .get_merchant_tag(&merchant_address)
        .expect("should return a tag for a tagged merchant");
    assert_eq!(tag, soroban_sdk::Symbol::new(&env, "retail"));
}

#[test]
fn test_get_merchant_tag_returns_none_for_untagged_address() {
    let (env, admin, client) = setup();
    let merchant_address = Address::generate(&env);
    let unknown_address = Address::generate(&env);
    let id = symbol_short!("AMAZON");
    let name = String::from_str(&env, "Amazon");

    client.register_merchant(
        &admin,
        &id,
        &name,
        &make_tags(&env, &["retail"]),
        &Some(merchant_address),
    );

    let tag = client.get_merchant_tag(&unknown_address);
    assert!(tag.is_none());
}

#[test]
fn test_get_merchant_tag_returns_none_for_merchant_without_address() {
    let (env, admin, client) = setup();
    let id = symbol_short!("AMAZON");
    let name = String::from_str(&env, "Amazon");

    // Register merchant without an address
    client.register_merchant(&admin, &id, &name, &make_tags(&env, &["retail"]), &None);

    let some_address = Address::generate(&env);
    let tag = client.get_merchant_tag(&some_address);
    assert!(tag.is_none());
}

#[test]
fn test_total_tagged_counter() {
    let (env, admin, client) = setup();
    let merchant_id = symbol_short!("AMAZON");
    client.register_merchant(
        &admin,
        &merchant_id,
        &String::from_str(&env, "Amazon"),
        &empty_tags(&env),
        &None,
    );

    assert_eq!(client.get_total_tagged(), 0);

    let tagger = Address::generate(&env);
    let asset = symbol_short!("XLM");
    let note = String::from_str(&env, "");
    client.tag_transaction(
        &tagger,
        &1u64,
        &merchant_id,
        &1_000i128,
        &asset,
        &note.clone(),
    );
    client.tag_transaction(&tagger, &2u64, &merchant_id, &2_000i128, &asset, &note);

    assert_eq!(client.get_total_tagged(), 2);
}

// ── Tag removal ───────────────────────────────────────────────────────────────

#[test]
fn test_remove_tag() {
    let (env, admin, client) = setup();
    let merchant_id = symbol_short!("AMAZON");
    client.register_merchant(
        &admin,
        &merchant_id,
        &String::from_str(&env, "Amazon"),
        &empty_tags(&env),
        &None,
    );

    let tagger = Address::generate(&env);
    client.tag_transaction(
        &tagger,
        &1u64,
        &merchant_id,
        &1_000i128,
        &symbol_short!("XLM"),
        &String::from_str(&env, ""),
    );

    assert!(client.get_transaction_tag(&1u64, &merchant_id).is_some());

    client.remove_tag(&admin, &1u64, &merchant_id);

    assert!(client.get_transaction_tag(&1u64, &merchant_id).is_none());
}

//! # Merchant Tagging Contract
//!
//! Adds merchant metadata to transactions so they can be grouped and
//! analysed by merchant. Resolves issue #495.
//!
//! ## Features
//!
//! - **Merchant registry** — register merchants with a name, category tags,
//!   and an optional Stellar address.
//! - **Transaction tagging** — attach a merchant ID, amount, asset, and note
//!   to any transaction ID.
//! - **Queryable index** — retrieve all transaction IDs for a given merchant.
//! - **Merchant analytics** — running totals of transaction count and volume
//!   per merchant, updated on every tag operation.
//! - **Tag removal** — remove a merchant tag from a transaction.
//! - **Admin controls** — only the admin can register/update/deactivate
//!   merchants; any address can tag its own transactions.

#![no_std]

extern crate alloc;

pub mod events;
pub mod types;

use alloc::string::ToString;
use soroban_sdk::{contract, contractimpl, Address, Env, String, Symbol, Vec};

use crate::events::{
    emit_merchant_deactivated, emit_merchant_registered, emit_merchant_updated, emit_tag_removed,
    emit_transaction_tagged,
};
use crate::types::{
    DataKey, Merchant, MerchantAnalytics, TransactionMerchantTag, MAX_MERCHANT_NAME_LEN,
    MAX_QUERY_RESULTS, MAX_TAGS_PER_MERCHANT, MAX_TAG_LEN,
};

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct MerchantTaggingContract;

#[contractimpl]
impl MerchantTaggingContract {
    // ── Initialisation ────────────────────────────────────────────────────────

    /// Initialise the contract with an admin address.
    ///
    /// Panics if already initialised.
    pub fn init(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::TotalTagged, &0u32);
    }

    /// Return the admin address.
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized")
    }

    // ── Merchant registry ─────────────────────────────────────────────────────

    /// Register a new merchant (admin only).
    ///
    /// # Arguments
    /// * `caller`  — must be the admin.
    /// * `id`      — unique merchant symbol (e.g. `symbol_short!("AMAZON")`).
    /// * `name`    — human-readable name (max `MAX_MERCHANT_NAME_LEN` chars).
    /// * `tags`    — category tags (max `MAX_TAGS_PER_MERCHANT`, each max `MAX_TAG_LEN`).
    /// * `address` — optional Stellar address for the merchant.
    ///
    /// # Panics
    /// - `"not initialized"` if `init` was not called.
    /// - `"unauthorized"` if `caller` is not the admin.
    /// - `"merchant already exists"` if `id` is already registered.
    /// - `"name too long"` / `"too many tags"` / `"tag too long"` on validation failure.
    pub fn register_merchant(
        env: Env,
        caller: Address,
        id: Symbol,
        name: String,
        tags: Vec<Symbol>,
        address: Option<Address>,
    ) {
        Self::require_admin(&env, &caller);

        if env.storage().instance().has(&DataKey::Merchant(id.clone())) {
            panic!("merchant already exists");
        }

        Self::validate_name(&name);
        Self::validate_tags(&tags);

        let merchant = Merchant {
            id: id.clone(),
            name,
            tags,
            address,
            registered_at: env.ledger().timestamp(),
            active: true,
        };

        env.storage()
            .instance()
            .set(&DataKey::Merchant(id.clone()), &merchant);

        // Add to global index
        let mut index: Vec<Symbol> = env
            .storage()
            .instance()
            .get(&DataKey::MerchantIndex)
            .unwrap_or_else(|| Vec::new(&env));
        index.push_back(id.clone());
        env.storage()
            .instance()
            .set(&DataKey::MerchantIndex, &index);

        emit_merchant_registered(&env, &id, &caller);
    }

    /// Update an existing merchant's name, tags, or address (admin only).
    ///
    /// Pass `None` for any field to leave it unchanged.
    pub fn update_merchant(
        env: Env,
        caller: Address,
        id: Symbol,
        name: Option<String>,
        tags: Option<Vec<Symbol>>,
        address: Option<Address>,
    ) {
        Self::require_admin(&env, &caller);

        let mut merchant: Merchant = env
            .storage()
            .instance()
            .get(&DataKey::Merchant(id.clone()))
            .expect("merchant not found");

        if let Some(n) = name {
            Self::validate_name(&n);
            merchant.name = n;
        }
        if let Some(t) = tags {
            Self::validate_tags(&t);
            merchant.tags = t;
        }
        if let Some(a) = address {
            merchant.address = Some(a);
        }

        env.storage()
            .instance()
            .set(&DataKey::Merchant(id.clone()), &merchant);

        emit_merchant_updated(&env, &id, &caller);
    }

    /// Deactivate a merchant (admin only). Existing tags are preserved.
    pub fn deactivate_merchant(env: Env, caller: Address, id: Symbol) {
        Self::require_admin(&env, &caller);

        let mut merchant: Merchant = env
            .storage()
            .instance()
            .get(&DataKey::Merchant(id.clone()))
            .expect("merchant not found");

        merchant.active = false;
        env.storage()
            .instance()
            .set(&DataKey::Merchant(id.clone()), &merchant);

        emit_merchant_deactivated(&env, &id, &caller);
    }

    /// Retrieve a merchant record by ID.
    pub fn get_merchant(env: Env, id: Symbol) -> Option<Merchant> {
        env.storage().instance().get(&DataKey::Merchant(id))
    }

    /// Return all registered merchant IDs.
    pub fn list_merchants(env: Env) -> Vec<Symbol> {
        env.storage()
            .instance()
            .get(&DataKey::MerchantIndex)
            .unwrap_or_else(|| Vec::new(&env))
    }

    // ── Transaction tagging ───────────────────────────────────────────────────

    /// Tag a transaction with a merchant.
    ///
    /// The `tagger` must authorise this call. The merchant must be active.
    ///
    /// # Arguments
    /// * `tagger`      — address performing the tag (must sign).
    /// * `tx_id`       — unique transaction identifier.
    /// * `merchant_id` — ID of the merchant to associate.
    /// * `amount`      — transaction amount in stroops (must be > 0).
    /// * `asset`       — asset code symbol (e.g. `symbol_short!("XLM")`).
    /// * `note`        — optional free-text note (pass empty string for none).
    ///
    /// # Panics
    /// - `"merchant not found"` if `merchant_id` is not registered.
    /// - `"merchant inactive"` if the merchant has been deactivated.
    /// - `"amount must be positive"` if `amount <= 0`.
    /// - `"already tagged"` if `tx_id` is already tagged with this merchant.
    pub fn tag_transaction(
        env: Env,
        tagger: Address,
        tx_id: u64,
        merchant_id: Symbol,
        amount: i128,
        asset: Symbol,
        note: String,
    ) {
        tagger.require_auth();

        if amount <= 0 {
            panic!("amount must be positive");
        }

        let merchant: Merchant = env
            .storage()
            .instance()
            .get(&DataKey::Merchant(merchant_id.clone()))
            .expect("merchant not found");

        if !merchant.active {
            panic!("merchant inactive");
        }

        let tag_key = DataKey::TransactionMerchant(tx_id, merchant_id.clone());
        if env.storage().persistent().has(&tag_key) {
            panic!("already tagged");
        }

        let tag = TransactionMerchantTag {
            tx_id,
            merchant_id: merchant_id.clone(),
            amount,
            asset: asset.clone(),
            timestamp: env.ledger().timestamp(),
            note,
        };

        env.storage().persistent().set(&tag_key, &tag);

        // Update merchant transaction index
        let idx_key = DataKey::MerchantTransactions(merchant_id.clone());
        let mut tx_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&idx_key)
            .unwrap_or_else(|| Vec::new(&env));
        tx_ids.push_back(tx_id);
        env.storage().persistent().set(&idx_key, &tx_ids);

        // Update merchant analytics
        let analytics_key = DataKey::MerchantAnalytics(merchant_id.clone());
        let mut analytics: MerchantAnalytics = env
            .storage()
            .instance()
            .get(&analytics_key)
            .unwrap_or(MerchantAnalytics {
                merchant_id: merchant_id.clone(),
                tx_count: 0,
                total_volume: 0,
                last_tx_at: 0,
            });

        analytics.tx_count += 1;
        analytics.total_volume = analytics
            .total_volume
            .checked_add(amount)
            .expect("volume overflow");
        analytics.last_tx_at = env.ledger().timestamp();
        env.storage().instance().set(&analytics_key, &analytics);

        // Increment global counter
        let total: u32 = env
            .storage()
            .instance()
            .get(&DataKey::TotalTagged)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::TotalTagged, &(total + 1));

        emit_transaction_tagged(&env, tx_id, &merchant_id, amount, &asset);
    }

    /// Remove a merchant tag from a transaction.
    ///
    /// The `caller` must be the admin or the original tagger.
    /// Analytics totals are NOT reversed (audit trail preserved).
    pub fn remove_tag(env: Env, caller: Address, tx_id: u64, merchant_id: Symbol) {
        caller.require_auth();
        // Only admin can remove tags (to prevent abuse)
        Self::require_admin(&env, &caller);

        let tag_key = DataKey::TransactionMerchant(tx_id, merchant_id.clone());
        if !env.storage().persistent().has(&tag_key) {
            panic!("tag not found");
        }

        env.storage().persistent().remove(&tag_key);
        emit_tag_removed(&env, tx_id, &merchant_id);
    }

    // ── Query functions ───────────────────────────────────────────────────────

    /// Retrieve the merchant tag for a specific transaction.
    pub fn get_transaction_tag(
        env: Env,
        tx_id: u64,
        merchant_id: Symbol,
    ) -> Option<TransactionMerchantTag> {
        env.storage()
            .persistent()
            .get(&DataKey::TransactionMerchant(tx_id, merchant_id))
    }

    /// Return all transaction IDs tagged with a given merchant.
    ///
    /// Results are capped at `MAX_QUERY_RESULTS` to avoid resource exhaustion.
    pub fn get_merchant_transactions(env: Env, merchant_id: Symbol) -> Vec<u64> {
        let all: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::MerchantTransactions(merchant_id))
            .unwrap_or_else(|| Vec::new(&env));

        // Cap results
        if all.len() <= MAX_QUERY_RESULTS {
            return all;
        }
        let mut capped: Vec<u64> = Vec::new(&env);
        for i in 0..MAX_QUERY_RESULTS {
            capped.push_back(all.get(i).unwrap());
        }
        capped
    }

    /// Return aggregate analytics for a merchant.
    pub fn get_merchant_analytics(env: Env, merchant_id: Symbol) -> Option<MerchantAnalytics> {
        env.storage()
            .instance()
            .get(&DataKey::MerchantAnalytics(merchant_id))
    }

    /// Return the total number of tagged transactions across all merchants.
    pub fn get_total_tagged(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::TotalTagged)
            .unwrap_or(0)
    }

    /// Retrieve the tag assigned to a merchant address.
    ///
    /// Iterates the merchant index to find a merchant whose registered address
    /// matches the given address, then returns the first category tag.
    /// Returns `None` if no merchant is found with that address or if the
    /// merchant has no tags.
    pub fn get_merchant_tag(env: Env, merchant: Address) -> Option<Symbol> {
        let index: Vec<Symbol> = env
            .storage()
            .instance()
            .get(&DataKey::MerchantIndex)
            .unwrap_or_else(|| Vec::new(&env));

        for merchant_id in index.iter() {
            if let Some(m) = env
                .storage()
                .instance()
                .get::<_, Merchant>(&DataKey::Merchant(merchant_id))
            {
                if m.address.as_ref() == Some(&merchant) {
                    return m.tags.get(0);
                }
            }
        }
        None
    }

    /// Query all merchants that have a specific category tag.
    ///
    /// Iterates the merchant index and returns matching merchant IDs.
    pub fn get_merchants_by_tag(env: Env, tag: Symbol) -> Vec<Symbol> {
        let index: Vec<Symbol> = env
            .storage()
            .instance()
            .get(&DataKey::MerchantIndex)
            .unwrap_or_else(|| Vec::new(&env));

        let mut result: Vec<Symbol> = Vec::new(&env);
        for merchant_id in index.iter() {
            if let Some(merchant) = env
                .storage()
                .instance()
                .get::<_, Merchant>(&DataKey::Merchant(merchant_id.clone()))
            {
                if merchant.tags.contains(&tag) {
                    result.push_back(merchant_id);
                }
            }
        }
        result
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    fn require_admin(env: &Env, caller: &Address) {
        caller.require_auth();
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if caller != &admin {
            panic!("unauthorized");
        }
    }

    fn validate_name(name: &String) {
        if name.len() > MAX_MERCHANT_NAME_LEN {
            panic!("name too long");
        }
        if name.len() == 0 {
            panic!("name cannot be empty");
        }
    }

    fn validate_tags(tags: &Vec<Symbol>) {
        if tags.len() > MAX_TAGS_PER_MERCHANT {
            panic!("too many tags");
        }
        for tag in tags.iter() {
            if tag.to_string().len() as u32 > MAX_TAG_LEN {
                panic!("tag too long");
            }
        }
    }
}

use soroban_sdk::{contracttype, Address, Env, Map, Symbol, Vec};

/// Maximum transfer history entries kept per user.
pub const MAX_TRANSFER_HISTORY: u32 = 100;

/// Time window (seconds) for rapid-spending detection.
pub const RAPID_SPEND_WINDOW_SECONDS: u64 = 60;

/// Number of spends within the window that triggers a freeze.
pub const RAPID_SPEND_THRESHOLD: u32 = 3;

/// Default automatic freeze duration (seconds) after suspicious activity.
pub const DEFAULT_FREEZE_DURATION_SECONDS: u64 = 3_600;

/// Budget category with limit and spent tracking.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CategoryBudget {
    pub name: Symbol,
    pub limit: i128,
    pub spent: i128,
}

/// User budget configuration with per-category envelopes.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserBudget {
    pub user: Address,
    pub categories: Map<Symbol, CategoryBudget>,
    pub last_updated: u64,
}

/// Record of a category-to-category transfer.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CategoryTransfer {
    pub transfer_id: u64,
    pub user: Address,
    pub from_category: Symbol,
    pub to_category: Symbol,
    pub amount: i128,
    pub timestamp: u64,
}

/// Freeze state for a user's budget after suspicious activity.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BudgetFreeze {
    pub is_frozen: bool,
    pub frozen_at: u64,
    pub auto_unfreeze_at: u64,
}

/// Recent spend timestamps used for rapid-spending detection.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpendingWindow {
    pub timestamps: Vec<u64>,
}

/// Budget template for reusable category configurations.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BudgetTemplate {
    pub id: Symbol,
    pub name: Symbol,
    pub categories: Map<Symbol, CategoryBudget>,
    pub created_by: Address,
    pub created_at: u64,
}

/// Checkpoint of a user budget for recovery purposes.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BudgetCheckpoint {
    pub owner: Address,
    pub limit: i128,
    pub spent: i128,
    pub version: u32,
}

/// A historical snapshot of a user's budget configuration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BudgetConfigVersion {
    pub version: u32,
    pub categories: Map<Symbol, CategoryBudget>,
    pub updated_at: u64,
}

/// Maximum number of budget config history entries kept per user.
pub const MAX_CONFIG_HISTORY: u32 = 50;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    UserBudget(Address),
    TransferCounter,
    UserTransfers(Address),
    Transfer(u64),
    BudgetFreeze(Address),
    SpendingWindow(Address),
    SuspiciousActivityCount,
    Budget(Address),
    ArchivedBudget(Address),
    BudgetAsset(Address, Address),
    ArchivedBudgetAsset(Address, Address),
    UserAssets(Address),
    TotalAllocated,
    PendingDeletion(Address),
    LastActivity(Address),
    InactivityTimeout(Address),
    InheritanceBeneficiaries(Address),
    Beneficiaries(Address),
    Template(Symbol),
    UserTemplates(Address),
    BudgetCheckpoint(Address),
    BudgetHistory(Address),
    BudgetVersionCounter(Address),
    Delegation(Address, Address),
    OwnerDelegates(Address),
}

pub fn get_user_budget(env: &Env, user: &Address) -> Option<UserBudget> {
    env.storage()
        .persistent()
        .get(&DataKey::UserBudget(user.clone()))
}

pub fn set_user_budget(env: &Env, budget: &UserBudget) {
    env.storage()
        .persistent()
        .set(&DataKey::UserBudget(budget.user.clone()), budget);
}

pub fn get_category_available(category: &CategoryBudget) -> i128 {
    category.limit.saturating_sub(category.spent)
}

pub fn next_transfer_id(env: &Env) -> u64 {
    let id: u64 = env
        .storage()
        .instance()
        .get(&DataKey::TransferCounter)
        .unwrap_or(0)
        + 1;
    env.storage().instance().set(&DataKey::TransferCounter, &id);
    id
}

pub fn record_transfer(env: &Env, transfer: &CategoryTransfer) {
    env.storage()
        .persistent()
        .set(&DataKey::Transfer(transfer.transfer_id), transfer);

    let mut history: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKey::UserTransfers(transfer.user.clone()))
        .unwrap_or_else(|| Vec::new(env));

    history.push_back(transfer.transfer_id);

    while history.len() > MAX_TRANSFER_HISTORY {
        let oldest = history.get(0).unwrap();
        env.storage()
            .persistent()
            .remove(&DataKey::Transfer(oldest));
        let mut trimmed = Vec::new(env);
        for i in 1..history.len() {
            trimmed.push_back(history.get(i).unwrap());
        }
        history = trimmed;
    }

    env.storage()
        .persistent()
        .set(&DataKey::UserTransfers(transfer.user.clone()), &history);
}

pub fn get_transfer(env: &Env, transfer_id: u64) -> Option<CategoryTransfer> {
    env.storage()
        .persistent()
        .get(&DataKey::Transfer(transfer_id))
}

pub fn get_user_transfers(env: &Env, user: &Address) -> Vec<CategoryTransfer> {
    let ids: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKey::UserTransfers(user.clone()))
        .unwrap_or_else(|| Vec::new(env));

    let mut transfers = Vec::new(env);
    for id in ids.iter() {
        if let Some(transfer) = get_transfer(env, id) {
            transfers.push_back(transfer);
        }
    }
    transfers
}

pub fn get_budget_freeze(env: &Env, user: &Address) -> Option<BudgetFreeze> {
    env.storage()
        .persistent()
        .get(&DataKey::BudgetFreeze(user.clone()))
}

pub fn set_budget_freeze(env: &Env, user: &Address, freeze: &BudgetFreeze) {
    env.storage()
        .persistent()
        .set(&DataKey::BudgetFreeze(user.clone()), freeze);
}

pub fn clear_budget_freeze(env: &Env, user: &Address) {
    env.storage()
        .persistent()
        .remove(&DataKey::BudgetFreeze(user.clone()));
}

pub fn is_budget_frozen(env: &Env, user: &Address, now: u64) -> bool {
    match get_budget_freeze(env, user) {
        Some(freeze) if freeze.is_frozen => {
            if freeze.auto_unfreeze_at > 0 && now >= freeze.auto_unfreeze_at {
                clear_budget_freeze(env, user);
                false
            } else {
                true
            }
        }
        _ => false,
    }
}

pub fn record_spend_timestamp(env: &Env, user: &Address, timestamp: u64) -> u32 {
    let mut window: SpendingWindow = env
        .storage()
        .persistent()
        .get(&DataKey::SpendingWindow(user.clone()))
        .unwrap_or(SpendingWindow {
            timestamps: Vec::new(env),
        });

    let cutoff = timestamp.saturating_sub(RAPID_SPEND_WINDOW_SECONDS);
    let mut recent = Vec::new(env);
    for ts in window.timestamps.iter() {
        if ts >= cutoff {
            recent.push_back(ts);
        }
    }
    recent.push_back(timestamp);
    window.timestamps = recent.clone();

    env.storage()
        .persistent()
        .set(&DataKey::SpendingWindow(user.clone()), &window);

    recent.len()
}

pub fn increment_suspicious_count(env: &Env) -> u64 {
    let count: u64 = env
        .storage()
        .instance()
        .get(&DataKey::SuspiciousActivityCount)
        .unwrap_or(0)
        + 1;
    env.storage()
        .instance()
        .set(&DataKey::SuspiciousActivityCount, &count);
    count
}

pub fn get_last_activity(env: &Env, user: &Address) -> u64 {
    env.storage()
        .persistent()
        .get(&DataKey::LastActivity(user.clone()))
        .unwrap_or(0)
}

pub fn set_last_activity(env: &Env, user: &Address, timestamp: u64) {
    env.storage()
        .persistent()
        .set(&DataKey::LastActivity(user.clone()), &timestamp);
}

pub fn get_inactivity_timeout(env: &Env, user: &Address) -> u64 {
    env.storage()
        .persistent()
        .get(&DataKey::InactivityTimeout(user.clone()))
        .unwrap_or(30 * 24 * 60 * 60) // default 30 days
}

pub fn set_inactivity_timeout(env: &Env, user: &Address, timeout: u64) {
    env.storage()
        .persistent()
        .set(&DataKey::InactivityTimeout(user.clone()), &timeout);
}

pub fn get_inheritance_beneficiaries(env: &Env, user: &Address) -> Vec<Address> {
    env.storage()
        .persistent()
        .get(&DataKey::InheritanceBeneficiaries(user.clone()))
        .unwrap_or_else(|| Vec::new(env))
}

pub fn set_inheritance_beneficiaries(env: &Env, user: &Address, beneficiaries: &Vec<Address>) {
    env.storage().persistent().set(
        &DataKey::InheritanceBeneficiaries(user.clone()),
        beneficiaries,
    );
}

pub fn get_beneficiaries(env: &Env, user: &Address) -> Vec<crate::types::Beneficiary> {
    env.storage()
        .persistent()
        .get(&DataKey::Beneficiaries(user.clone()))
        .unwrap_or_else(|| Vec::new(env))
}

pub fn set_beneficiaries(
    env: &Env,
    user: &Address,
    beneficiaries: &Vec<crate::types::Beneficiary>,
) {
    env.storage()
        .persistent()
        .set(&DataKey::Beneficiaries(user.clone()), beneficiaries);
}

pub fn save_template(env: &Env, template: &BudgetTemplate) {
    env.storage()
        .persistent()
        .set(&DataKey::Template(template.id.clone()), template);

    let mut user_templates: Vec<Symbol> = env
        .storage()
        .persistent()
        .get(&DataKey::UserTemplates(template.created_by.clone()))
        .unwrap_or_else(|| Vec::new(env));

    if !user_templates.contains(&template.id) {
        user_templates.push_back(template.id.clone());
        env.storage().persistent().set(
            &DataKey::UserTemplates(template.created_by.clone()),
            &user_templates,
        );
    }
}

pub fn get_template(env: &Env, template_id: Symbol) -> Option<BudgetTemplate> {
    env.storage()
        .persistent()
        .get(&DataKey::Template(template_id))
}

pub fn get_user_templates(env: &Env, user: &Address) -> Vec<Symbol> {
    env.storage()
        .persistent()
        .get(&DataKey::UserTemplates(user.clone()))
        .unwrap_or_else(|| Vec::new(env))
}

pub fn delete_template(env: &Env, template_id: Symbol, user: &Address) {
    if let Some(template) = get_template(env, template_id.clone()) {
        if template.created_by == *user {
            env.storage()
                .persistent()
                .remove(&DataKey::Template(template_id.clone()));

            let user_templates = get_user_templates(env, user);
            let mut new_templates = Vec::new(env);
            for id in user_templates.iter() {
                if id != template_id {
                    new_templates.push_back(id);
                }
            }
            if new_templates.len() > 0 {
                env.storage()
                    .persistent()
                    .set(&DataKey::UserTemplates(user.clone()), &new_templates);
            } else {
                env.storage()
                    .persistent()
                    .remove(&DataKey::UserTemplates(user.clone()));
            }
        }
    }
}

/// Records a new version of the user's budget configuration into history.
/// Increments the version counter and trims history to MAX_CONFIG_HISTORY.
pub fn save_budget_config_version(
    env: &Env,
    user: &Address,
    categories: &Map<Symbol, CategoryBudget>,
    updated_at: u64,
) {
    let version: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::BudgetVersionCounter(user.clone()))
        .unwrap_or(0)
        + 1;

    env.storage()
        .persistent()
        .set(&DataKey::BudgetVersionCounter(user.clone()), &version);

    let entry = BudgetConfigVersion {
        version,
        categories: categories.clone(),
        updated_at,
    };

    let mut history: Vec<BudgetConfigVersion> = env
        .storage()
        .persistent()
        .get(&DataKey::BudgetHistory(user.clone()))
        .unwrap_or_else(|| Vec::new(env));

    history.push_back(entry);

    // Trim to MAX_CONFIG_HISTORY
    while history.len() > MAX_CONFIG_HISTORY {
        let mut trimmed = Vec::new(env);
        for i in 1..history.len() {
            trimmed.push_back(history.get(i).unwrap());
        }
        history = trimmed;
    }

    env.storage()
        .persistent()
        .set(&DataKey::BudgetHistory(user.clone()), &history);
}

/// Returns the full budget config history for a user (oldest first).
pub fn get_budget_config_history(env: &Env, user: &Address) -> Vec<BudgetConfigVersion> {
    env.storage()
        .persistent()
        .get(&DataKey::BudgetHistory(user.clone()))
        .unwrap_or_else(|| Vec::new(env))
}

/// Returns a specific version from the user's budget config history, or None.
pub fn get_budget_config_version(
    env: &Env,
    user: &Address,
    version: u32,
) -> Option<BudgetConfigVersion> {
    let history = get_budget_config_history(env, user);
    for entry in history.iter() {
        if entry.version == version {
            return Some(entry);
        }
    }
    None
}

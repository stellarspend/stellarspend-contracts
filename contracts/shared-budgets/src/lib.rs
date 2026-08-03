#![no_std]

mod types;
mod validation;

use soroban_sdk::{contract, contractimpl, panic_with_error, token, Address, Env, Symbol, Vec};

pub use crate::types::{
    ArchivedBudget, Budget, BudgetContribution, BudgetSpendingRule, BudgetUtilizationBand,
    BudgetUtilizationSummary, DataKey, SharedBudgetEvents, MAX_BUDGET_MEMBERS, MAX_SPENDING_RULES,
};
use crate::validation::{validate_amount, validate_percentage};

/// Error codes for the shared budget contract.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum SharedBudgetError {
    /// Contract not initialized
    NotInitialized = 1,
    /// Caller is not authorized
    Unauthorized = 2,
    /// Budget does not exist
    BudgetNotFound = 3,
    /// Member already exists in budget
    MemberAlreadyExists = 4,
    /// Member not found in budget
    MemberNotFound = 5,
    /// Spending rule not found
    RuleNotFound = 6,
    /// Invalid amount
    InvalidAmount = 7,
    /// Insufficient balance
    InsufficientBalance = 8,
    /// Invalid percentage value
    InvalidPercentage = 9,
    /// Budget is already active
    BudgetAlreadyActive = 10,
    /// Budget is not active
    BudgetNotActive = 11,
    /// Too many members in budget
    TooManyMembers = 12,
    /// Too many spending rules
    TooManyRules = 13,
    /// Archive retention period must be greater than zero
    InvalidArchiveRetention = 14,
    /// Archived budget was not found
    ArchivedBudgetNotFound = 15,
}

impl From<SharedBudgetError> for soroban_sdk::Error {
    fn from(e: SharedBudgetError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

#[contract]
pub struct SharedBudgetContract;

#[contractimpl]
impl SharedBudgetContract {
    /// Initializes the contract with an admin address.
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Contract already initialized");
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::TotalBudgetsCreated, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::TotalContributionsProcessed, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::ArchiveRetentionPeriod, &0u64);
    }

    /// Creates a new shared budget with specified members and spending rules.
    pub fn create_budget(
        env: Env,
        creator: Address,
        budget_name: Symbol,
        members: Vec<Address>,
        token: Address,
        spending_rules: Vec<BudgetSpendingRule>,
    ) -> u64 {
        creator.require_auth();

        // Validate member count
        if members.len() as u32 > MAX_BUDGET_MEMBERS {
            panic_with_error!(&env, SharedBudgetError::TooManyMembers);
        }

        // Validate spending rules count
        if spending_rules.len() as u32 > MAX_SPENDING_RULES {
            panic_with_error!(&env, SharedBudgetError::TooManyRules);
        }

        // Validate spending rules
        for rule in spending_rules.iter() {
            validate_percentage(rule.percentage_threshold).unwrap_or_else(|_| {
                panic_with_error!(&env, SharedBudgetError::InvalidPercentage);
            });
        }

        // Get next budget ID and increment counter
        let budget_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalBudgetsCreated)
            .unwrap_or(0)
            + 1;

        // Create budget structure
        let budget = Budget {
            id: budget_id,
            name: budget_name,
            creator: creator.clone(),
            token: token.clone(),
            members: members.clone(),
            balance: 0,
            total_contributed: 0,
            spending_rules: spending_rules.clone(),
            is_active: true,
            created_at: env.ledger().timestamp(),
        };

        // Store the budget
        env.storage()
            .persistent()
            .set(&DataKey::Budget(budget_id), &budget);

        // Update counter
        env.storage()
            .instance()
            .set(&DataKey::TotalBudgetsCreated, &budget_id);

        // Add members to budget
        for member in members.iter() {
            env.storage()
                .persistent()
                .set(&DataKey::BudgetMember(budget_id, member.clone()), &true);
        }

        // Emit event
        SharedBudgetEvents::budget_created(&env, budget_id, &creator, &members, &token);

        budget_id
    }

    /// Contributes to a shared budget.
    pub fn contribute_to_budget(
        env: Env,
        contributor: Address,
        budget_id: u64,
        amount: i128,
        memo: Option<Symbol>,
    ) {
        contributor.require_auth();

        // Validate amount
        validate_amount(amount).unwrap_or_else(|_| {
            panic_with_error!(&env, SharedBudgetError::InvalidAmount);
        });

        // Load budget
        let mut budget: Budget = env
            .storage()
            .persistent()
            .get(&DataKey::Budget(budget_id))
            .unwrap_or_else(|| panic_with_error!(&env, SharedBudgetError::BudgetNotFound));

        if !budget.is_active {
            panic_with_error!(&env, SharedBudgetError::BudgetNotActive);
        }

        // Transfer tokens from contributor to contract
        let token_client = token::Client::new(&env, &budget.token);
        token_client.transfer(&contributor, &env.current_contract_address(), &amount);

        // Update budget balance and contribution tracking
        budget.balance += amount;
        budget.total_contributed += amount;

        // Store updated budget
        env.storage()
            .persistent()
            .set(&DataKey::Budget(budget_id), &budget);

        // Track contribution
        let contribution = BudgetContribution {
            budget_id,
            contributor: contributor.clone(),
            amount,
            memo: memo.clone(),
            timestamp: env.ledger().timestamp(),
        };

        // Get next contribution ID
        let contribution_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalContributionsProcessed)
            .unwrap_or(0)
            + 1;

        env.storage()
            .persistent()
            .set(&DataKey::Contribution(contribution_id), &contribution);

        let mut contribution_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::BudgetContributions(budget_id))
            .unwrap_or_else(|| Vec::new(&env));
        contribution_ids.push_back(contribution_id);
        env.storage()
            .persistent()
            .set(&DataKey::BudgetContributions(budget_id), &contribution_ids);

        env.storage()
            .instance()
            .set(&DataKey::TotalContributionsProcessed, &contribution_id);

        // Emit event
        SharedBudgetEvents::contribution_added(&env, budget_id, &contributor, amount, memo);
    }

    /// Spend from a shared budget with spending rule enforcement.
    pub fn spend_from_budget(
        env: Env,
        spender: Address,
        budget_id: u64,
        recipient: Address,
        amount: i128,
    ) {
        spender.require_auth();

        // Validate amount
        validate_amount(amount).unwrap_or_else(|_| {
            panic_with_error!(&env, SharedBudgetError::InvalidAmount);
        });

        // Load budget
        let mut budget: Budget = env
            .storage()
            .persistent()
            .get(&DataKey::Budget(budget_id))
            .unwrap_or_else(|| panic_with_error!(&env, SharedBudgetError::BudgetNotFound));

        if !budget.is_active {
            panic_with_error!(&env, SharedBudgetError::BudgetNotActive);
        }

        // Check if spender is a member of the budget
        let is_member = env
            .storage()
            .persistent()
            .get(&DataKey::BudgetMember(budget_id, spender.clone()))
            .unwrap_or(false);

        if !is_member {
            panic_with_error!(&env, SharedBudgetError::MemberNotFound);
        }

        // Check if budget has sufficient balance
        if budget.balance < amount {
            panic_with_error!(&env, SharedBudgetError::InsufficientBalance);
        }

        // Enforce spending rules
        Self::enforce_spending_rules(&env, &budget, &spender, amount);

        // Transfer tokens from contract to recipient
        let token_client = token::Client::new(&env, &budget.token);
        token_client.transfer(&env.current_contract_address(), &recipient, &amount);

        // Update budget balance
        budget.balance -= amount;

        // Store updated budget
        env.storage()
            .persistent()
            .set(&DataKey::Budget(budget_id), &budget);

        // Emit event
        SharedBudgetEvents::expense_incurred(&env, budget_id, &spender, &recipient, amount);
    }

    /// Transfers budget ownership from the current owner to a new account.
    ///
    /// Both the current owner and the new owner must authorize the transfer.
    /// After transfer, the previous owner loses owner-level control.
    pub fn transfer_budget_ownership(
        env: Env,
        current_owner: Address,
        budget_id: u64,
        new_owner: Address,
    ) {
        current_owner.require_auth();
        new_owner.require_auth();

        let mut budget: Budget = env
            .storage()
            .persistent()
            .get(&DataKey::Budget(budget_id))
            .unwrap_or_else(|| panic_with_error!(&env, SharedBudgetError::BudgetNotFound));

        if current_owner != budget.creator {
            panic_with_error!(&env, SharedBudgetError::Unauthorized);
        }

        if current_owner == new_owner {
            return;
        }

        budget.creator = new_owner.clone();

        env.storage()
            .persistent()
            .set(&DataKey::Budget(budget_id), &budget);

        SharedBudgetEvents::ownership_transferred(&env, budget_id, &current_owner, &new_owner);
    }

    /// Add a member to an existing budget.
    pub fn add_member_to_budget(env: Env, caller: Address, budget_id: u64, new_member: Address) {
        caller.require_auth();

        // Load budget
        let mut budget: Budget = env
            .storage()
            .persistent()
            .get(&DataKey::Budget(budget_id))
            .unwrap_or_else(|| panic_with_error!(&env, SharedBudgetError::BudgetNotFound));

        // Only creators or admins can add members
        if caller != budget.creator {
            Self::require_admin(&env, &caller);
        }

        // Check if member already exists
        let member_exists = env
            .storage()
            .persistent()
            .get(&DataKey::BudgetMember(budget_id, new_member.clone()))
            .unwrap_or(false);

        if member_exists {
            panic_with_error!(&env, SharedBudgetError::MemberAlreadyExists);
        }

        // Check member limit
        let mut member_count = 0u32;
        for _member in budget.members.iter() {
            member_count += 1;
        }

        if member_count >= MAX_BUDGET_MEMBERS {
            panic_with_error!(&env, SharedBudgetError::TooManyMembers);
        }

        // Add member to budget
        budget.members.push_back(new_member.clone());

        env.storage()
            .persistent()
            .set(&DataKey::BudgetMember(budget_id, new_member.clone()), &true);

        env.storage()
            .persistent()
            .set(&DataKey::Budget(budget_id), &budget);
    }

    /// Add a spending rule to an existing budget.
    pub fn add_spending_rule(env: Env, caller: Address, budget_id: u64, rule: BudgetSpendingRule) {
        caller.require_auth();

        let mut budget: Budget = env
            .storage()
            .persistent()
            .get(&DataKey::Budget(budget_id))
            .unwrap_or_else(|| panic_with_error!(&env, SharedBudgetError::BudgetNotFound));

        if caller != budget.creator {
            Self::require_admin(&env, &caller);
        }

        validate_percentage(rule.percentage_threshold).unwrap_or_else(|_| {
            panic_with_error!(&env, SharedBudgetError::InvalidPercentage);
        });

        if budget.spending_rules.len() as u32 >= MAX_SPENDING_RULES {
            panic_with_error!(&env, SharedBudgetError::TooManyRules);
        }

        SharedBudgetEvents::spending_rule_added(&env, budget_id, &rule);

        budget.spending_rules.push_back(rule);

        env.storage()
            .persistent()
            .set(&DataKey::Budget(budget_id), &budget);
    }

    /// Configures how long inactive budgets remain in active storage before archiving.
    pub fn set_archive_retention_period(env: Env, caller: Address, retention_seconds: u64) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        if retention_seconds == 0 {
            panic_with_error!(&env, SharedBudgetError::InvalidArchiveRetention);
        }

        env.storage()
            .instance()
            .set(&DataKey::ArchiveRetentionPeriod, &retention_seconds);
    }

    /// Returns the configured archive retention period in seconds.
    pub fn get_archive_retention_period(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::ArchiveRetentionPeriod)
            .unwrap_or(0)
    }

    /// Marks a budget inactive so it becomes eligible for scheduled archiving.
    pub fn deactivate_budget(env: Env, caller: Address, budget_id: u64) {
        caller.require_auth();

        let mut budget: Budget = env
            .storage()
            .persistent()
            .get(&DataKey::Budget(budget_id))
            .unwrap_or_else(|| panic_with_error!(&env, SharedBudgetError::BudgetNotFound));

        if caller != budget.creator {
            Self::require_admin(&env, &caller);
        }

        if !budget.is_active {
            return;
        }

        budget.is_active = false;
        let deactivated_at = env.ledger().timestamp();

        env.storage()
            .persistent()
            .set(&DataKey::Budget(budget_id), &budget);
        env.storage()
            .persistent()
            .set(&DataKey::BudgetDeactivatedAt(budget_id), &deactivated_at);

        SharedBudgetEvents::budget_deactivated(&env, budget_id, &caller, deactivated_at);
    }

    /// Archives inactive budgets that have exceeded the configured retention period.
    ///
    /// Returns the number of budgets archived in this run.
    pub fn archive_inactive_budgets(env: Env, caller: Address, max_to_archive: u32) -> u32 {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let retention_seconds = Self::get_archive_retention_period(env.clone());
        if retention_seconds == 0 {
            panic_with_error!(&env, SharedBudgetError::InvalidArchiveRetention);
        }
        if max_to_archive == 0 {
            return 0;
        }

        let now = env.ledger().timestamp();
        let total_budgets = Self::get_total_budgets_created(env.clone());
        let mut archived = 0u32;

        for budget_id in 1..=total_budgets {
            if archived >= max_to_archive {
                break;
            }

            let budget: Budget = match env.storage().persistent().get(&DataKey::Budget(budget_id)) {
                Some(budget) => budget,
                None => continue,
            };

            if budget.is_active {
                continue;
            }

            let deactivated_at = env
                .storage()
                .persistent()
                .get(&DataKey::BudgetDeactivatedAt(budget_id))
                .unwrap_or(budget.created_at);

            if now < deactivated_at.saturating_add(retention_seconds) {
                continue;
            }

            let contribution_ids = env
                .storage()
                .persistent()
                .get(&DataKey::BudgetContributions(budget_id))
                .unwrap_or_else(|| Vec::new(&env));

            let archived_budget = ArchivedBudget {
                budget: budget.clone(),
                deactivated_at,
                archived_at: now,
                contribution_ids,
            };

            env.storage()
                .persistent()
                .set(&DataKey::ArchivedBudget(budget_id), &archived_budget);
            env.storage()
                .persistent()
                .remove(&DataKey::Budget(budget_id));
            env.storage()
                .persistent()
                .remove(&DataKey::BudgetDeactivatedAt(budget_id));

            SharedBudgetEvents::budget_archived(&env, budget_id, now);
            archived += 1;
        }

        archived
    }

    /// Get budget details.
    pub fn get_budget(env: Env, budget_id: u64) -> Budget {
        env.storage()
            .persistent()
            .get(&DataKey::Budget(budget_id))
            .unwrap_or_else(|| panic_with_error!(&env, SharedBudgetError::BudgetNotFound))
    }

    /// Returns an archived budget snapshot, if one exists.
    pub fn get_archived_budget(env: Env, budget_id: u64) -> Option<ArchivedBudget> {
        env.storage()
            .persistent()
            .get(&DataKey::ArchivedBudget(budget_id))
    }

    /// Get member status for a budget.
    pub fn is_budget_member(env: Env, budget_id: u64, member: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::BudgetMember(budget_id, member))
            .unwrap_or(false)
    }

    /// Returns the role of an account within a budget: "OWNER" for the creator,
    /// "MEMBER" for a member, or "NONE" if the account is unrelated to the budget.
    pub fn get_member_role(env: Env, budget_id: u64, account: Address) -> Symbol {
        let budget: Budget = env
            .storage()
            .persistent()
            .get(&DataKey::Budget(budget_id))
            .unwrap_or_else(|| panic_with_error!(&env, SharedBudgetError::BudgetNotFound));

        if account == budget.creator {
            return Symbol::new(&env, "OWNER");
        }

        let is_member = env
            .storage()
            .persistent()
            .get(&DataKey::BudgetMember(budget_id, account))
            .unwrap_or(false);

        if is_member {
            Symbol::new(&env, "MEMBER")
        } else {
            Symbol::new(&env, "NONE")
        }
    }

    /// Returns budget utilization analytics as
    /// `(utilization_percent, total_spent, avg_spending_per_member, remaining_balance)`.
    ///
    /// Utilization is the share of contributed funds that have been spent.
    pub fn get_budget_utilization(env: Env, budget_id: u64) -> (u32, i128, i128, i128) {
        let summary = Self::get_budget_utilization_summary(env, budget_id);
        (
            summary.utilization_percent,
            summary.total_spent,
            summary.avg_spending_per_member,
            summary.remaining_balance,
        )
    }

    /// Returns the standardized utilization band for a budget.
    pub fn get_budget_utilization_band(env: Env, budget_id: u64) -> BudgetUtilizationBand {
        Self::get_budget_utilization_summary(env, budget_id).utilization_band
    }

    /// Returns budget utilization analytics including the standardized utilization band.
    pub fn get_budget_utilization_summary(env: Env, budget_id: u64) -> BudgetUtilizationSummary {
        let budget: Budget = env
            .storage()
            .persistent()
            .get(&DataKey::Budget(budget_id))
            .unwrap_or_else(|| panic_with_error!(&env, SharedBudgetError::BudgetNotFound));

        let total_spent = budget.total_contributed - budget.balance;
        let utilization_percent = if budget.total_contributed > 0 {
            (total_spent * 100 / budget.total_contributed) as u32
        } else {
            0
        };

        let member_count = budget.members.len() as i128;
        let avg_spending_per_member = if member_count > 0 {
            total_spent / member_count
        } else {
            0
        };

        BudgetUtilizationSummary {
            utilization_percent,
            total_spent,
            avg_spending_per_member,
            remaining_balance: budget.balance,
            utilization_band: Self::classify_utilization(utilization_percent),
        }
    }

    /// Get contribution details.
    pub fn get_contribution(env: Env, contribution_id: u64) -> BudgetContribution {
        env.storage()
            .persistent()
            .get(&DataKey::Contribution(contribution_id))
            .unwrap_or_else(|| panic_with_error!(&env, SharedBudgetError::RuleNotFound))
        // Using RuleNotFound as a generic error
    }

    fn get_budget_contribution_ids(env: &Env, budget_id: u64) -> Vec<u64> {
        if !env.storage().persistent().has(&DataKey::Budget(budget_id))
            && !env
                .storage()
                .persistent()
                .has(&DataKey::ArchivedBudget(budget_id))
        {
            panic_with_error!(env, SharedBudgetError::BudgetNotFound);
        }

        env.storage()
            .persistent()
            .get(&DataKey::BudgetContributions(budget_id))
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Get paginated contributions for a budget.
    pub fn get_contributions_paginated(
        env: Env,
        budget_id: u64,
        offset: u32,
        limit: u32,
    ) -> Vec<BudgetContribution> {
        let limit = limit.min(100);
        let contribution_ids = Self::get_budget_contribution_ids(&env, budget_id);
        let total_count = contribution_ids.len();

        let mut result: Vec<BudgetContribution> = Vec::new(&env);
        if offset >= total_count || limit == 0 {
            return result;
        }

        let end = (offset + limit).min(total_count as u32);
        for i in offset..end {
            if let Some(contribution_id) = contribution_ids.get(i) {
                if let Some(contribution) = env
                    .storage()
                    .persistent()
                    .get(&DataKey::Contribution(contribution_id))
                {
                    result.push_back(contribution);
                }
            }
        }

        result
    }

    /// Returns the admin address.
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Contract not initialized")
    }

    /// Updates the admin address.
    pub fn set_admin(env: Env, current_admin: Address, new_admin: Address) {
        current_admin.require_auth();
        Self::require_admin(&env, &current_admin);

        env.storage().instance().set(&DataKey::Admin, &new_admin);
    }

    /// Returns the total number of budgets created.
    pub fn get_total_budgets_created(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalBudgetsCreated)
            .unwrap_or(0)
    }

    /// Returns the total number of contributions processed.
    pub fn get_total_contribs_processed(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalContributionsProcessed)
            .unwrap_or(0)
    }

    // Internal helper to enforce spending rules
    fn enforce_spending_rules(env: &Env, budget: &Budget, spender: &Address, amount: i128) {
        // Check each spending rule to see if it applies
        for rule in budget.spending_rules.iter() {
            // If this rule applies to the spender and the amount exceeds threshold
            if rule.applicable_to == *spender {
                // Check if rule applies to this specific spender
                let threshold_amount = if budget.total_contributed > 0 {
                    (budget.total_contributed as f64 * (rule.percentage_threshold as f64 / 100.0))
                        as i128
                } else {
                    0 // If no contributions yet, threshold is 0
                };

                if amount > threshold_amount && !rule.requires_approval {
                    panic_with_error!(env, SharedBudgetError::Unauthorized);
                }
            }
        }
    }

    fn classify_utilization(utilization_percent: u32) -> BudgetUtilizationBand {
        match utilization_percent {
            0..=24 => BudgetUtilizationBand::Low,
            25..=49 => BudgetUtilizationBand::Moderate,
            50..=79 => BudgetUtilizationBand::High,
            _ => BudgetUtilizationBand::Critical,
        }
    }

    // Internal helper to verify admin
    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Contract not initialized");

        if *caller != admin {
            panic_with_error!(env, SharedBudgetError::Unauthorized);
        }
    }

    /// Returns all contributions made to a specific budget, in chronological order.
    ///
    /// Returns an empty Vec if the budget exists but has received no contributions.
    /// Panics with `BudgetNotFound` if the budget does not exist.
    pub fn get_contributions(env: Env, budget_id: u64) -> Vec<BudgetContribution> {
        let contribution_ids = Self::get_budget_contribution_ids(&env, budget_id);

        let mut result: Vec<BudgetContribution> = Vec::new(&env);
        for id in contribution_ids.iter() {
            if let Some(contrib) = env.storage().persistent().get(&DataKey::Contribution(id)) {
                result.push_back(contrib);
            }
        }
        result
    }
}

#[cfg(test)]
mod test;

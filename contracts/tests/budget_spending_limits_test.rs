//! Integration tests verifying that the budget and spending-limits contracts
//! enforce limits correctly when used together.

use budget::{BudgetContract, BudgetContractClient};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env, Vec};
use spending_limits::{
    LimitStrategy, SpendingLimitRequest, SpendingLimitsContract, SpendingLimitsContractClient,
};
use std::panic::{catch_unwind, AssertUnwindSafe};

fn setup() -> (
    Env,
    Address,
    BudgetContractClient<'static>,
    SpendingLimitsContractClient<'static>,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let budget_id = env.register(BudgetContract, ());
    let limits_id = env.register(SpendingLimitsContract, ());

    let budget = BudgetContractClient::new(&env, &budget_id);
    let limits = SpendingLimitsContractClient::new(&env, &limits_id);

    budget.initialize(&admin);
    limits.initialize(&admin);

    (env, admin, budget, limits, user)
}

fn configure_user(
    env: &Env,
    admin: &Address,
    budget: &BudgetContractClient,
    limits: &SpendingLimitsContractClient,
    user: &Address,
    category_limit: i128,
    daily_limit: i128,
    monthly_limit: i128,
) {
    let category = symbol_short!("food");

    budget.update_budget(admin, user, &category_limit, &None);
    budget.set_category_budget(admin, user, &category, &category_limit);

    // Whitelist the user so enforce_spending_limit passes its destination check.
    limits.whitelist_destination(admin, user);

    let mut requests = Vec::new(env);
    requests.push_back(SpendingLimitRequest {
        user: user.clone(),
        monthly_limit,
        daily_limit,
        hourly_limit: daily_limit,
        reset_window_seconds: 86_400,
        category: Some(category),
        strategy: LimitStrategy::Static,
    });
    limits.batch_update_spending_limits(admin, &requests);
}

/// Budget category and spending limit are both satisfied: spend succeeds.
#[test]
fn spend_within_limit_is_allowed() {
    let (env, admin, budget, limits, user) = setup();
    let category = symbol_short!("food");

    configure_user(&env, &admin, &budget, &limits, &user, 500_000_000, 100_000_000, 500_000_000);

    let remaining = budget.spend_from_category(&user, &category, &50_000_000i128);
    assert_eq!(remaining, 450_000_000);

    limits.enforce_spending_limit(&user, &50_000_000i128, &Some(category));

    let stored = limits.get_spending_limit_details(&user).unwrap();
    assert_eq!(stored.current_spending, 50_000_000);
}

/// Spending more than the budget category allows is rejected by the budget contract.
#[test]
fn spend_exceeding_budget_category_is_rejected() {
    let (env, admin, budget, limits, user) = setup();
    let category = symbol_short!("food");

    configure_user(&env, &admin, &budget, &limits, &user, 100_000_000, 200_000_000, 500_000_000);

    let result = catch_unwind(AssertUnwindSafe(|| {
        budget.spend_from_category(&user, &category, &150_000_000i128);
    }));
    assert!(result.is_err(), "budget should reject InsufficientBalance");
}

/// Spending more than the spending limit allows is rejected by the spending-limits contract.
#[test]
fn spend_exceeding_spending_limit_is_rejected() {
    let (env, admin, budget, limits, user) = setup();
    let category = symbol_short!("food");

    // Budget allows 500, but daily limit is only 80.
    configure_user(&env, &admin, &budget, &limits, &user, 500_000_000, 80_000_000, 500_000_000);

    let result = catch_unwind(AssertUnwindSafe(|| {
        limits.enforce_spending_limit(&user, &100_000_000i128, &Some(category));
    }));
    assert!(result.is_err(), "spending-limits should reject DailyLimitExceeded");
}

/// Sequential spends accumulate correctly; once either limit is exhausted both contracts reject.
#[test]
fn budget_and_limit_both_enforced_sequentially() {
    let (env, admin, budget, limits, user) = setup();
    let category = symbol_short!("food");

    configure_user(&env, &admin, &budget, &limits, &user, 300_000_000, 300_000_000, 300_000_000);

    // Two valid spends of 100 each.
    budget.spend_from_category(&user, &category, &100_000_000i128);
    limits.enforce_spending_limit(&user, &100_000_000i128, &Some(category.clone()));

    budget.spend_from_category(&user, &category, &100_000_000i128);
    limits.enforce_spending_limit(&user, &100_000_000i128, &Some(category.clone()));

    // Third spend of 200 exceeds remaining 100 in both contracts.
    let budget_result = catch_unwind(AssertUnwindSafe(|| {
        budget.spend_from_category(&user, &category, &200_000_000i128);
    }));
    assert!(budget_result.is_err(), "budget should reject the overspend");

    let limit_result = catch_unwind(AssertUnwindSafe(|| {
        limits.enforce_spending_limit(&user, &200_000_000i128, &Some(category));
    }));
    assert!(limit_result.is_err(), "spending-limits should reject the overspend");
}

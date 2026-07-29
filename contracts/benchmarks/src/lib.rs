#![cfg(test)]
#![no_std]

use soroban_sdk::{
    testutils::{Address as _, Events, Ledger, LedgerInfo},
    vec, Address, Bytes, Env, Symbol,
};

// Use underscores for crate names with dashes
use budget::{BudgetContract, BudgetContractClient};
use savings_goals::{
    GoalResult, SavingsGoalRequest, SavingsGoalsContract, SavingsGoalsContractClient,
};

// ─── Benchmark Helpers ────────────────────────────────────────────────────────

fn make_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_info(&env, 1_700_000_000);
    env
}

fn set_ledger_info(env: &Env, ts: u64) {
    env.ledger().set(LedgerInfo {
        timestamp: ts,
        protocol_version: 22,
        sequence_number: 1,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 16,
        min_persistent_entry_ttl: 4096,
        max_entry_ttl: 6_312_000,
    });
}

fn cpu_instructions(env: &Env) -> u64 {
    env.cost_estimate().budget().cpu_instruction_cost()
}

fn mem_bytes(env: &Env) -> u64 {
    env.cost_estimate().budget().memory_bytes_cost()
}

fn print_metrics(name: &str, cpu: u64, mem: u64) {
    extern crate std;
    std::println!(
        "[BENCHMARK] {} | CPU: {} | Memory: {} bytes",
        name,
        cpu,
        mem
    );
}

fn idempotency_token(env: &Env, seed: u8) -> Bytes {
    let mut token = Bytes::new(env);
    token.push_back(seed);
    token
}

fn first_goal_id(result: GoalResult) -> u64 {
    match result {
        GoalResult::Success(goal) => goal.goal_id,
        GoalResult::Failure(_, code) => panic!("goal creation failed in benchmark: {}", code),
    }
}

// ─── Budget Benchmarks ────────────────────────────────────────────────────────

#[test]
fn benchmark_budget_creation() {
    let env = make_env();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let cid = env.register(BudgetContract, ());
    let client = BudgetContractClient::new(&env, &cid);

    client.initialize(&admin);

    env.cost_estimate().budget().reset_default();
    let cpu_before = cpu_instructions(&env);
    let mem_before = mem_bytes(&env);

    client.update_budget(&admin, &user, &1000i128, &None);

    let cpu_used = cpu_instructions(&env) - cpu_before;
    let mem_used = mem_bytes(&env) - mem_before;

    print_metrics("Budget Creation (initial record)", cpu_used, mem_used);

    assert!(
        cpu_used < 5_000_000,
        "Budget creation CPU regression: {}",
        cpu_used
    );
}

#[test]
fn benchmark_spending_validation() {
    let env = make_env();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let category = Symbol::new(&env, "food");

    let cid = env.register(BudgetContract, ());
    let client = BudgetContractClient::new(&env, &cid);

    client.initialize(&admin);
    client.update_budget(&admin, &user, &5000i128, &None);
    client.set_category_budget(&admin, &user, &category, &1000i128);

    env.cost_estimate().budget().reset_default();
    let cpu_before = cpu_instructions(&env);
    let mem_before = mem_bytes(&env);

    client.spend_from_category(&user, &category, &100i128);

    let cpu_used = cpu_instructions(&env) - cpu_before;
    let mem_used = mem_bytes(&env) - mem_before;

    print_metrics(
        "Spending Validation (spend_from_category)",
        cpu_used,
        mem_used,
    );

    assert!(
        cpu_used < 5_000_000,
        "Spending validation CPU regression: {}",
        cpu_used
    );
}

// ─── Goal Benchmarks ──────────────────────────────────────────────────────────

#[test]
fn benchmark_goal_creation() {
    let env = make_env();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let cid = env.register(SavingsGoalsContract, ());
    let client = SavingsGoalsContractClient::new(&env, &cid);

    client.initialize(&admin);

    let request = SavingsGoalRequest {
        user: user.clone(),
        goal_name: Symbol::new(&env, "NewCar"),
        target_amount: 50_000_000,
        deadline: 10_000,
        initial_contribution: 10_000_000,
        priority: 0,
        lock_duration_seconds: 0,
        penalty_bps: 0,
        expiration_seconds: 0,
    };

    let requests = vec![&env, request];

    env.cost_estimate().budget().reset_default();
    let cpu_before = cpu_instructions(&env);
    let mem_before = mem_bytes(&env);

    client.batch_set_savings_goals(&admin, &requests);

    let cpu_used = cpu_instructions(&env) - cpu_before;
    let mem_used = mem_bytes(&env) - mem_before;

    print_metrics(
        "Goal Creation (batch_set_savings_goals n=1)",
        cpu_used,
        mem_used,
    );

    assert!(
        cpu_used < 8_000_000,
        "Goal creation CPU regression: {}",
        cpu_used
    );
}

#[test]
fn benchmark_goal_contribution() {
    let env = make_env();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let cid = env.register(SavingsGoalsContract, ());
    let client = SavingsGoalsContractClient::new(&env, &cid);

    client.initialize(&admin);

    let request = SavingsGoalRequest {
        user: user.clone(),
        goal_name: Symbol::new(&env, "House"),
        target_amount: 100_000_000,
        deadline: 10_000,
        initial_contribution: 0,
        priority: 0,
        lock_duration_seconds: 0,
        penalty_bps: 0,
        expiration_seconds: 0,
    };
    let result = client.batch_set_savings_goals(&admin, &vec![&env, request]);
    let goal_id = first_goal_id(result.results.get(0).unwrap());

    env.cost_estimate().budget().reset_default();
    let cpu_before = cpu_instructions(&env);
    let mem_before = mem_bytes(&env);

    client.contribute_to_goal(&user, &goal_id, &5_000_000i128, &idempotency_token(&env, 1));

    let cpu_used = cpu_instructions(&env) - cpu_before;
    let mem_used = mem_bytes(&env) - mem_before;

    print_metrics("Goal Contribution (contribute_to_goal)", cpu_used, mem_used);

    assert!(
        cpu_used < 5_000_000,
        "Goal contribution CPU regression: {}",
        cpu_used
    );
}

#[test]
fn benchmark_goal_withdrawal() {
    let env = make_env();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let cid = env.register(SavingsGoalsContract, ());
    let client = SavingsGoalsContractClient::new(&env, &cid);

    client.initialize(&admin);

    let request = SavingsGoalRequest {
        user: user.clone(),
        goal_name: Symbol::new(&env, "Vacation"),
        target_amount: 20_000_000,
        deadline: 10_000,
        initial_contribution: 20_000_000, // Fully funded
        priority: 0,
        lock_duration_seconds: 0,
        penalty_bps: 0,
        expiration_seconds: 0,
    };
    let result = client.batch_set_savings_goals(&admin, &vec![&env, request]);
    let goal_id = first_goal_id(result.results.get(0).unwrap());

    env.cost_estimate().budget().reset_default();
    let cpu_before = cpu_instructions(&env);
    let mem_before = mem_bytes(&env);

    client.withdraw_from_goal(&user, &goal_id, &5_000_000i128, &None);

    let cpu_used = cpu_instructions(&env) - cpu_before;
    let mem_used = mem_bytes(&env) - mem_before;

    print_metrics("Goal Withdrawal (withdraw_from_goal)", cpu_used, mem_used);

    assert!(
        cpu_used < 5_000_000,
        "Goal withdrawal CPU regression: {}",
        cpu_used
    );
}

// ─── Event Emission Benchmarks ────────────────────────────────────────────────

#[test]
fn benchmark_event_emission_overhead() {
    let env = make_env();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let cid = env.register(BudgetContract, ());
    let client = BudgetContractClient::new(&env, &cid);
    client.initialize(&admin);

    // Measure first call (includes storage setup)
    env.cost_estimate().budget().reset_default();
    client.update_budget(&admin, &user, &100i128, &None);
    let _ = cpu_instructions(&env);

    // Measure second call (mostly event emission and small update)
    env.cost_estimate().budget().reset_default();
    let cpu_before = cpu_instructions(&env);
    let mem_before = mem_bytes(&env);
    client.update_budget(&admin, &user, &200i128, &None);
    let cpu_used = cpu_instructions(&env) - cpu_before;
    let mem_used = mem_bytes(&env) - mem_before;

    print_metrics(
        "Event Emission Overhead (approx from update_budget)",
        cpu_used,
        mem_used,
    );

    assert!(
        cpu_used < 2_000_000,
        "Event emission overhead regression: {}",
        cpu_used
    );

    let events = env.events().all();
    assert!(events.len() >= 1, "Expected at least 1 event emitted");
}

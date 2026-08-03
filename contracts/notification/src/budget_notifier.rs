use soroban_sdk::Env;

use crate::events::emit_budget_event;

pub struct BudgetNotifier;

impl BudgetNotifier {
    pub fn check_usage(env: &Env, used: i128, limit: i128) {
        if limit == 0 {
            return;
        }

        let usage_percent = (used * 100) / limit;

        // 80% used warning
        if usage_percent >= 80 && usage_percent < 100 {
            emit_budget_event(env, "warning", "80% of budget used");
        }

        // limit exceeded
        if usage_percent >= 100 {
            emit_budget_event(env, "exceeded", "Budget limit exceeded");
        }
    }

    pub fn goal_completed(env: &Env) {
        emit_budget_event(env, "goal_completed", "Savings goal completed");
    }
}

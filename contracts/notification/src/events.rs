use soroban_sdk::{Env, String};

pub fn emit_budget_event(env: &Env, event_type: &str, message: &str) {
    env.events().publish(
        (String::from_str(env, "budget"),),
        (
            String::from_str(env, event_type),
            String::from_str(env, message),
        ),
    );
}

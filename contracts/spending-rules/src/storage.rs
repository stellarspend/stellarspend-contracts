use soroban_sdk::{Address, Env, Symbol};

use crate::types::{Config, DataKey, Rule};

/// Reads contract configuration from instance storage.
pub fn read_config(env: &Env) -> Option<Config> {
    env.storage().instance().get(&DataKey::Config)
}

/// Writes contract configuration to instance storage.
pub fn write_config(env: &Env, config: &Config) {
    env.storage().instance().set(&DataKey::Config, config);
}

/// Reads a user's rule for `category`, if any.
pub fn read_rule(env: &Env, user: &Address, category: &Symbol) -> Option<Rule> {
    env.storage()
        .persistent()
        .get(&DataKey::Rule(user.clone(), category.clone()))
}

/// Writes (or replaces) a user's rule for `category`.
pub fn write_rule(env: &Env, user: &Address, category: &Symbol, rule: &Rule) {
    env.storage()
        .persistent()
        .set(&DataKey::Rule(user.clone(), category.clone()), rule);
}

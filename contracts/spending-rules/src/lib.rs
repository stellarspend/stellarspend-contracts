#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, symbol_short, Address, Bytes, Env, Symbol,
};

mod engine;
mod storage;
#[cfg(test)]
mod test;
pub mod types;
pub mod validation;

use types::Rule;

/// Typed errors for the spending_rules contract.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// Contract has already been initialized.
    AlreadyInitialized = 1,
    /// Caller is not authorized to perform this action.
    Unauthorized = 2,
    /// Amount validation failed (must be strictly positive).
    InvalidAmount = 3,
    /// No rule is configured for this (user, category) pair.
    RuleNotFound = 4,
    /// Amount exceeds the ZK-required threshold and no proof was supplied.
    ZkProofRequired = 5,
    /// A proof was supplied but failed zk-verifier verification.
    ZkProofInvalid = 6,
    /// Payment would exceed the rule's weekly category cap.
    CategoryLimitExceeded = 7,
    /// Payment would exceed the wallet-level spending limit.
    WalletLimitExceeded = 8,
}

/// The native asset the wallet-level spending-limit check is performed in.
const NATIVE_ASSET: &str = "XLM";
/// Period symbol for the weekly category cap.
const WEEKLY: &str = "weekly";

#[contract]
pub struct Contract;

#[contractimpl]
impl Contract {
    /// Initializes the composition engine with the administrator and the
    /// addresses of the three contracts it composes cross-contract.
    pub fn initialize(
        env: Env,
        admin: Address,
        limits_contract: Address,
        categories_contract: Address,
        zk_verifier_contract: Address,
    ) -> Result<(), Error> {
        if storage::read_config(&env).is_some() {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        storage::write_config(
            &env,
            &types::Config {
                admin,
                limits_contract,
                categories_contract,
                zk_verifier_contract,
            },
        );
        Ok(())
    }

    /// Sets (or replaces) `user`'s rule for `category`: a weekly spending cap
    /// plus the amount above which a ZK proof is required. Only `user` may set
    /// their own rule.
    pub fn set_rule(
        env: Env,
        user: Address,
        category: Symbol,
        weekly_limit: i128,
        zk_required_above: i128,
    ) -> Result<(), Error> {
        user.require_auth();
        validation::validate_rule(weekly_limit, zk_required_above)?;

        let rule = Rule {
            category: category.clone(),
            weekly_limit,
            zk_required_above,
        };
        storage::write_rule(&env, &user, &category, &rule);

        env.events().publish(
            (symbol_short!("rules"), symbol_short!("set"), user),
            (category, weekly_limit, zk_required_above),
        );
        Ok(())
    }

    /// Returns `user`'s rule for `category`, if any.
    pub fn get_rule(env: Env, user: Address, category: Symbol) -> Option<Rule> {
        storage::read_rule(&env, &user, &category)
    }

    /// Evaluates a proposed payment of `amount` in `category` for `user`
    /// against the composed rules, returning a typed pass/fail verdict.
    ///
    /// The engine performs, in order:
    /// 1. A ZK-required check against `zk_required_above` (cross-contract call
    ///    into zk-verifier to validate `zk_proof` when required).
    /// 2. A weekly category cap check (cross-contract call into
    ///    spending-categories for the amount already spent this week).
    /// 3. A wallet-level cap check (cross-contract call into spending-limits).
    pub fn evaluate(
        env: Env,
        user: Address,
        category: Symbol,
        amount: i128,
        zk_proof: Option<Bytes>,
    ) -> Result<(), Error> {
        validation::validate_amount(amount)?;

        let config = storage::read_config(&env).ok_or(Error::Unauthorized)?;
        let rule = storage::read_rule(&env, &user, &category).ok_or(Error::RuleNotFound)?;

        // 1. ZK threshold — above it, a verified proof is mandatory.
        if engine::zk_proof_required(&rule, amount) {
            let proof = zk_proof.ok_or(Error::ZkProofRequired)?;
            let zk_client =
                zk_verifier::ZkVerifierContractClient::new(&env, &config.zk_verifier_contract);
            if !zk_client.verify_spending_proof(&user, &proof) {
                return Err(Error::ZkProofInvalid);
            }
        }

        // 2. Weekly category cap — already-spent comes from spending-categories.
        let categories_client =
            spending_categories::ContractClient::new(&env, &config.categories_contract);
        let already_spent =
            categories_client.get_category_total(&user, &category, &Symbol::new(&env, WEEKLY));
        engine::check_weekly_cap(&rule, already_spent, amount)?;

        // 3. Wallet-level cap — enforced against the native asset.
        let limits_client = spending_limits::ContractClient::new(&env, &config.limits_contract);
        if !limits_client.check_limit(&user, &Symbol::new(&env, NATIVE_ASSET), &amount) {
            return Err(Error::WalletLimitExceeded);
        }

        env.events().publish(
            (symbol_short!("rules"), symbol_short!("eval"), user),
            (category, amount),
        );
        Ok(())
    }
}

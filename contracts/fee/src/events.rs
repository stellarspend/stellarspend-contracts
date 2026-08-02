use soroban_sdk::{contracttype, symbol_short, Address, Env, IntoVal, String, Symbol, Val};

use crate::storage::{DEFAULT_FEE_BPS, DEFAULT_MIN_FEE};
use crate::utils::format_amount;

/// Event version for backward compatibility.
const EVENT_VERSION: u32 = 1;

pub struct FeeEvents;
pub struct TierEvents;
pub struct ConfigEvents;
pub struct FeeConfigEvents;

/// Common helper to standardize event publication.
fn publish<T>(env: &Env, category: Symbol, action: Symbol, data: T)
where
    T: IntoVal<Env, Val>,
{
    env.events()
        .publish((category, action, EVENT_VERSION), data);
}

//
// ──────────────────────────────────────────────────────────
// Fee Event Payloads
// ──────────────────────────────────────────────────────────
//

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeCollectedEvent {
    pub user: Address,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeEscrowedEvent {
    pub payer: Address,
    pub amount: i128,
    pub cycle: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeReleasedEvent {
    pub cycle: u64,
    pub amount: i128,
    pub treasury: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeDistributedEvent {
    pub total_amount: i128,
    pub treasury_amount: i128,
    pub stakeholder_amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeRolloverEvent {
    pub from_cycle: u64,
    pub to_cycle: u64,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeReconciliationEvent {
    pub stored: i128,
    pub calculated: i128,
    pub discrepancy: i128,
}

//
// ──────────────────────────────────────────────────────────
// Fee Events
// ──────────────────────────────────────────────────────────
//

impl FeeEvents {
    pub fn fee_collected(env: &Env, user: &Address, amount: i128) {
        publish(
            env,
            symbol_short!("fee"),
            symbol_short!("collect"),
            FeeCollectedEvent {
                user: user.clone(),
                amount,
            },
        );
    }

    pub fn fee_escrowed(env: &Env, payer: &Address, amount: i128, cycle: u64) {
        publish(
            env,
            symbol_short!("fee"),
            symbol_short!("escrow"),
            FeeEscrowedEvent {
                payer: payer.clone(),
                amount,
                cycle,
            },
        );
    }

    pub fn fee_batched(
        env: &Env,
        payer: &Address,
        total_amount: i128,
        batch_size: u32,
        cycle: u64,
    ) {
        publish(
            env,
            symbol_short!("fee"),
            symbol_short!("batch"),
            (payer.clone(), total_amount, batch_size, cycle),
        );
    }

    pub fn fee_released(env: &Env, cycle: u64, amount: i128, treasury: &Address) {
        publish(
            env,
            symbol_short!("fee"),
            symbol_short!("release"),
            FeeReleasedEvent {
                cycle,
                amount,
                treasury: treasury.clone(),
            },
        );
    }

    pub fn fee_bps_updated(env: &Env, fee_bps: u32) {
        publish(env, symbol_short!("fee"), symbol_short!("bps"), fee_bps);
    }

    pub fn treasury_updated(env: &Env, treasury: &Address) {
        publish(
            env,
            symbol_short!("fee"),
            symbol_short!("treasury"),
            treasury.clone(),
        );
    }

    pub fn reconciliation_completed(env: &Env, is_match: bool, difference: i128) {
        let topics = (symbol_short!("fee"), symbol_short!("reconcile"));
        env.events().publish(topics, (is_match, difference));
    }

    pub fn min_fee_updated(env: &Env, min_fee: i128) {
        publish(env, symbol_short!("fee"), symbol_short!("minfee"), min_fee);
    }

    pub fn fee_rolled(env: &Env, from_cycle: u64, to_cycle: u64, amount: i128) {
        publish(
            env,
            symbol_short!("fee"),
            symbol_short!("roll"),
            FeeRolloverEvent {
                from_cycle,
                to_cycle,
                amount,
            },
        );
    }

    pub fn locked(env: &Env) {
        publish(env, symbol_short!("fee"), symbol_short!("locked"), ());
    }

    pub fn unlocked(env: &Env) {
        publish(env, symbol_short!("fee"), symbol_short!("unlock"), ());
    }

    pub fn fee_reconciled(env: &Env, stored: i128, calculated: i128) {
        publish(
            env,
            symbol_short!("fee"),
            symbol_short!("recon"),
            FeeReconciliationEvent {
                stored,
                calculated,
                discrepancy: 0,
            },
        );
    }

    pub fn fee_discrepancy(env: &Env, stored: i128, calculated: i128, discrepancy: i128) {
        publish(
            env,
            symbol_short!("fee"),
            symbol_short!("discrep"),
            FeeReconciliationEvent {
                stored,
                calculated,
                discrepancy,
            },
        );
    }
}

//
// ──────────────────────────────────────────────────────────
// Tier Events
// ──────────────────────────────────────────────────────────
//

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TierAssignmentEvent {
    pub admin: Address,
    pub user: Address,
    pub tier: Symbol,
}

impl TierEvents {
    pub fn tier_set(env: &Env, admin: &Address, user: &Address, tier: &Symbol) {
        publish(
            env,
            symbol_short!("tier"),
            symbol_short!("set"),
            TierAssignmentEvent {
                admin: admin.clone(),
                user: user.clone(),
                tier: tier.clone(),
            },
        );
    }

    pub fn tier_removed(env: &Env, admin: &Address, user: &Address) {
        publish(
            env,
            symbol_short!("tier"),
            symbol_short!("remove"),
            (admin.clone(), user.clone()),
        );
    }
}

//
// ──────────────────────────────────────────────────────────
// Fee Reset Event
// ──────────────────────────────────────────────────────────
//

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct FeeResetEventData {
    pub admin: Address,
    pub fee_bps: u32,
    pub min_fee: i128,
    pub formatted_min_fee: String,
}

impl ConfigEvents {
    pub fn fee_reset(env: &Env, admin: &Address) {
        publish(
            env,
            symbol_short!("fee"),
            symbol_short!("reset"),
            FeeResetEventData {
                admin: admin.clone(),
                fee_bps: DEFAULT_FEE_BPS,
                min_fee: DEFAULT_MIN_FEE,
                formatted_min_fee: format_amount(env, DEFAULT_MIN_FEE),
            },
        );
    }
}

//
// ──────────────────────────────────────────────────────────
// Fee Configuration Events
// ──────────────────────────────────────────────────────────
//

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct FeeConfigUpdatedEvent {
    pub admin: Address,
    pub fee_bps: Option<u32>,
    pub min_fee: Option<i128>,
    pub max_fee: Option<i128>,
}

impl FeeConfigEvents {
    pub fn fee_config_updated(
        env: &Env,
        admin: &Address,
        fee_bps: Option<u32>,
        min_fee: Option<i128>,
        max_fee: Option<i128>,
    ) {
        publish(
            env,
            symbol_short!("fee"),
            symbol_short!("cfgupd"),
            FeeConfigUpdatedEvent {
                admin: admin.clone(),
                fee_bps,
                min_fee,
                max_fee,
            },
        );
    }
}

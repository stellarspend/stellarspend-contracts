#![no_std]
use soroban_sdk::{contract, contractimpl, log, Address, Env, Symbol, Vec};

pub mod events;
#[cfg(test)]
#[path = "../tests/integration.rs"]
mod integration;
#[cfg(test)]
mod test;
pub mod types;

use crate::events::emit_spending_updated;
use crate::types::{
    aggregate_by_category_window, CategorySpend, CategorySpendWindow, CategorySpending, DataKey,
    MonthlyAnalytics, TimeFilter, TransactionEvent,
};

#[contract]
pub struct CategoryAnalytics;

#[contractimpl]
impl CategoryAnalytics {
    /// Initializes the contract with an admin address
    pub fn init(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
    }

    /// Records spending for a user and category.
    /// Updates both current spending aggregations and monthly history.
    pub fn record_spending(env: Env, user: Address, category: Symbol, amount: i128) {
        user.require_auth();
        record_category_spending(&env, &user, category.clone(), amount);

        log!(
            &env,
            "recorded spending: user={}, category={}, amount={}",
            user,
            category,
            amount
        );
    }

    /// Records a batch of spending entries for a user across one or more categories.
    pub fn record_spending_batch(env: Env, user: Address, spendings: Vec<CategorySpend>) {
        user.require_auth();

        if spendings.len() == 0 {
            panic!("batch must not be empty");
        }

        for spending in spendings.iter() {
            record_category_spending(&env, &user, spending.category, spending.amount);
        }
    }

    pub fn process_events(env: Env, caller: Address, events: Vec<TransactionEvent>) {
        caller.require_auth();
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if caller != admin {
            panic!("unauthorized");
        }
        for event in events.iter() {
            record_category_spending(&env, &event.from, event.category.clone(), event.amount);
        }
    }

    pub fn spending_by_category_in_window(
        _env: Env,
        events: Vec<TransactionEvent>,
        window_start: u64,
        window_end: u64,
    ) -> Vec<CategorySpendWindow> {
        aggregate_by_category_window(&events, window_start, window_end)
    }

    /// Retrieves current aggregate spending for a user and category.
    pub fn get_current_spending(env: Env, user: Address, category: Symbol) -> CategorySpending {
        let key = DataKey::CurrentSpending(user, category);
        env.storage()
            .instance()
            .get(&key)
            .unwrap_or(CategorySpending {
                count: 0,
                volume: 0,
            })
    }

    /// Retrieves analytics for a user and category in a specific month
    pub fn get_category_metrics(
        env: Env,
        user: Address,
        category: Symbol,
        year: u32,
        month: u32,
    ) -> MonthlyAnalytics {
        let key = DataKey::MonthlyAnalytics(year, month, user.clone(), category.clone());
        env.storage()
            .persistent()
            .get(&key)
            .unwrap_or(MonthlyAnalytics {
                user,
                category,
                year,
                month,
                volume: 0,
                count: 0,
                last_updated: 0,
            })
    }

    /// Aggregates yearly trend for a user and category
    pub fn get_yearly_trend(
        env: Env,
        user: Address,
        category: Symbol,
        year: u32,
    ) -> CategorySpending {
        let mut total_volume: i128 = 0;
        let mut total_count: u32 = 0;

        for month in 1..=12 {
            let analytics = Self::get_category_metrics(
                env.clone(),
                user.clone(),
                category.clone(),
                year,
                month,
            );
            total_volume = total_volume
                .checked_add(analytics.volume)
                .expect("volume overflow");
            total_count += analytics.count;
        }

        CategorySpending {
            count: total_count,
            volume: total_volume,
        }
    }

    /// Retrieves analytics for a user and category in a specific month with time filtering.
    ///
    /// Filters metrics to ensure they fall within the specified `time_filter` range.
    /// Results outside the range are excluded, and empty ranges (start > end) return
    /// empty results without panicking.
    pub fn get_category_metrics_filtered(
        env: Env,
        user: Address,
        category: Symbol,
        year: u32,
        month: u32,
        time_filter: TimeFilter,
    ) -> MonthlyAnalytics {
        // Check for empty/invalid time range (start > end)
        if time_filter.start_timestamp > time_filter.end_timestamp {
            return MonthlyAnalytics {
                user,
                category,
                year,
                month,
                volume: 0,
                count: 0,
                last_updated: 0,
            };
        }

        let analytics =
            Self::get_category_metrics(env.clone(), user.clone(), category.clone(), year, month);

        // Check if the analytics entry is within the time range
        if analytics.last_updated >= time_filter.start_timestamp
            && analytics.last_updated <= time_filter.end_timestamp
        {
            analytics
        } else {
            MonthlyAnalytics {
                user,
                category,
                year,
                month,
                volume: 0,
                count: 0,
                last_updated: 0,
            }
        }
    }

    /// Aggregates yearly trend for a user and category with time filtering.
    ///
    /// Excludes month metrics that do not fall within the `time_filter` range.
    /// Empty ranges (start > end) return zero totals without panicking.
    pub fn get_yearly_trend_filtered(
        env: Env,
        user: Address,
        category: Symbol,
        year: u32,
        time_filter: TimeFilter,
    ) -> CategorySpending {
        // Check for empty/invalid time range (start > end)
        if time_filter.start_timestamp > time_filter.end_timestamp {
            return CategorySpending {
                count: 0,
                volume: 0,
            };
        }

        let mut total_volume: i128 = 0;
        let mut total_count: u32 = 0;

        for month in 1..=12 {
            let analytics = Self::get_category_metrics_filtered(
                env.clone(),
                user.clone(),
                category.clone(),
                year,
                month,
                time_filter.clone(),
            );
            total_volume = total_volume
                .checked_add(analytics.volume)
                .expect("volume overflow");
            total_count += analytics.count;
        }

        CategorySpending {
            count: total_count,
            volume: total_volume,
        }
    }
}

fn record_category_spending(env: &Env, user: &Address, category: Symbol, amount: i128) {
    if amount <= 0 {
        panic!("amount must be positive");
    }

    let current_key = DataKey::CurrentSpending(user.clone(), category.clone());
    let mut current = env
        .storage()
        .instance()
        .get(&current_key)
        .unwrap_or(CategorySpending {
            count: 0,
            volume: 0,
        });

    current.count += 1;
    current.volume = current.volume.checked_add(amount).expect("volume overflow");
    env.storage().instance().set(&current_key, &current);

    let ledger_timestamp = env.ledger().timestamp();
    let (year, month) = get_year_month(ledger_timestamp);

    let monthly_key = DataKey::MonthlyAnalytics(year, month, user.clone(), category.clone());
    let mut monthly = env
        .storage()
        .persistent()
        .get(&monthly_key)
        .unwrap_or(MonthlyAnalytics {
            user: user.clone(),
            category: category.clone(),
            year,
            month,
            volume: 0,
            count: 0,
            last_updated: ledger_timestamp,
        });

    monthly.volume = monthly.volume.checked_add(amount).expect("volume overflow");
    monthly.count += 1;
    monthly.last_updated = ledger_timestamp;

    env.storage().persistent().set(&monthly_key, &monthly);

    emit_spending_updated(env, user.clone(), category, amount);
}

/// Helper function to estimate year and month from timestamp
/// Note: This is a simplified calculation for simulation
fn get_year_month(timestamp: u64) -> (u32, u32) {
    let _seconds_in_day = 86400;
    let seconds_in_year = 31536000;
    let seconds_in_month = 2592000; // Average month (30 days)

    let year = 1970 + (timestamp / seconds_in_year) as u32;
    let month = 1 + ((timestamp % seconds_in_year) / seconds_in_month) as u32;

    (year, month % 13) // Ensure month is 1-12
}

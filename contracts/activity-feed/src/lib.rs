#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Env, Symbol, Vec};

#[cfg(test)]
mod test;

#[contracttype]
#[derive(Clone, Debug)]
pub struct ActivityEvent {
    pub event_type: Symbol,
    pub timestamp: u64,
    pub sequence: u64,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    TotalEvents,
    Event(u64),
}

#[contract]
pub struct ActivityFeedContract;

#[contractimpl]
impl ActivityFeedContract {
    /// Record a new protocol event in the activity feed.
    pub fn record_event(env: Env, event_type: Symbol) -> u64 {
        let seq: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalEvents)
            .unwrap_or(0)
            + 1;

        let event = ActivityEvent {
            event_type: event_type.clone(),
            timestamp: env.ledger().timestamp(),
            sequence: seq,
        };

        env.storage().persistent().set(&DataKey::Event(seq), &event);
        env.storage().instance().set(&DataKey::TotalEvents, &seq);
        env.events()
            .publish((symbol_short!("activity"),), (event_type, seq));
        seq
    }

    /// Get a paginated page of recent events (newest first).
    /// page starts at 1, page_size max 50.
    pub fn get_feed(env: Env, page: u64, page_size: u64) -> Vec<ActivityEvent> {
        let total: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalEvents)
            .unwrap_or(0);

        let size = if page_size == 0 || page_size > 50 {
            20
        } else {
            page_size
        };
        let p = if page == 0 { 1 } else { page };
        let end = total.saturating_sub((p - 1) * size);
        let start = end.saturating_sub(size) + 1;

        let mut results: Vec<ActivityEvent> = Vec::new(&env);
        let mut i = end;
        while i >= start && i > 0 {
            if let Some(ev) = env.storage().persistent().get(&DataKey::Event(i)) {
                results.push_back(ev);
            }
            if i == 0 {
                break;
            }
            i -= 1;
        }
        results
    }

    /// Get the total number of recorded events.
    pub fn total_events(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalEvents)
            .unwrap_or(0)
    }
}

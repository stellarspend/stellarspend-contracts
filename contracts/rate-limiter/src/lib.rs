//! Starting scaffold for a shared rate-limiter (issue #916). Neither
//! contracts/rate_limit.rs nor contracts/throttling.rs exist in the
//! current tree to consolidate, so this is a fresh minimal sliding-window
//! limiter, not yet wired into the batch-* contracts or the workspace.

use soroban_sdk::{contracttype, Env};

#[derive(Clone)]
#[contracttype]
pub struct RateLimitWindow {
    pub window_started_at: u64,
    pub count: u32,
}

/// Checks whether another call is allowed within the current window and,
/// if so, returns the updated window state to persist. Bounded: only
/// ever stores one (window_started_at, count) pair per caller.
pub fn check_and_record(
    env: &Env,
    window: Option<RateLimitWindow>,
    max_per_window: u32,
    window_secs: u64,
) -> Option<RateLimitWindow> {
    let now = env.ledger().timestamp();
    match window {
        Some(w) if now < w.window_started_at + window_secs => {
            if w.count >= max_per_window {
                None
            } else {
                Some(RateLimitWindow { count: w.count + 1, ..w })
            }
        }
        _ => Some(RateLimitWindow { window_started_at: now, count: 1 }),
    }
}

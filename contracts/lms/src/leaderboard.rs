use soroban_sdk::{contracttype, Address, Env, Vec};

/// A learner's leaderboard statistics.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeaderboardEntry {
    pub learner: Address,
    pub xp: u64,
    pub courses_completed: u32,
    pub badges: u32,
}

/// Return the LMS leaderboard.
///
/// NOTE:
/// Replace the mock data loading with your project's storage.
pub fn get_leaderboard(env: &Env) -> Vec<LeaderboardEntry> {
    let mut leaderboard = Vec::new(env);

    // TODO:
    // Replace with actual learners loaded from storage.
    //
    // Example:
    //
    // for learner in get_all_students(env) {
    //     leaderboard.push_back(LeaderboardEntry {
    //         learner,
    //         xp: get_xp(env, learner),
    //         courses_completed: get_completed_courses(env, learner),
    //         badges: get_badges(env, learner),
    //     });
    // }

    sort_leaderboard(env, &mut leaderboard);

    leaderboard
}

/// Sorts learners by:
///
/// 1. XP
/// 2. Courses completed
/// 3. Badges earned
fn sort_leaderboard(env: &Env, entries: &mut Vec<LeaderboardEntry>) {
    let mut temp: std::vec::Vec<LeaderboardEntry> = entries.iter().collect();

    temp.sort_by(|a, b| {
        b.xp
            .cmp(&a.xp)
            .then(b.courses_completed.cmp(&a.courses_completed))
            .then(b.badges.cmp(&a.badges))
    });

    let mut sorted = Vec::new(env);

    for learner in temp {
        sorted.push_back(learner);
    }

    *entries = sorted;
}
#![allow(dead_code)]

use soroban_sdk::{contracterror, contracttype, symbol_short, Address, Env, Vec};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum QuizError {
    /// `maximum_score` was zero, so no score could be graded against it
    InvalidMaximumScore = 200,
    /// `score` exceeded `maximum_score`
    ScoreOutOfRange = 201,
    /// `passing_score` exceeded `maximum_score`, making the quiz unpassable
    InvalidPassingScore = 202,
    /// The `(student, quiz)` pair already holds `u32::MAX` attempts
    TooManyAttempts = 203,
}

/// A single graded quiz attempt.
///
/// Attempts are append-only: once recorded, an entry is never mutated or
/// removed, so the sequence of attempts forms an immutable audit trail.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuizAttempt {
    /// Quiz this attempt belongs to
    pub quiz_id: u64,

    /// Learner who submitted the attempt
    pub student: Address,

    /// 1-based position in the learner's history for this quiz
    pub attempt_number: u32,

    /// Points awarded
    pub score: u32,

    /// Points available at the time of the attempt
    pub maximum_score: u32,

    /// Threshold the attempt was graded against
    pub passing_score: u32,

    /// Whether `score` met `passing_score`
    pub passed: bool,

    /// Ledger timestamp when the attempt was recorded
    pub timestamp: u64,
}

#[contracttype]
pub enum DataKey {
    /// Append-only attempt history for a `(student, quiz)` pair
    QuizHistory(Address, u64),
}

fn history_key(student: &Address, quiz_id: u64) -> DataKey {
    DataKey::QuizHistory(student.clone(), quiz_id)
}

/// Records a quiz attempt, appending it to the learner's history.
///
/// The attempt number is derived from the length of the stored history rather
/// than a separate counter, so numbering cannot drift out of sync with the
/// entries it labels. Existing entries are left untouched.
pub fn record_quiz_attempt(
    env: Env,
    student: Address,
    quiz_id: u64,
    score: u32,
    maximum_score: u32,
    passing_score: u32,
) -> Result<QuizAttempt, QuizError> {
    student.require_auth();

    let passed = evaluate_score(score, maximum_score, passing_score)?;

    let key = history_key(&student, quiz_id);
    let mut history: Vec<QuizAttempt> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(&env));

    if history.len() == u32::MAX {
        return Err(QuizError::TooManyAttempts);
    }

    let attempt = QuizAttempt {
        quiz_id,
        student: student.clone(),
        attempt_number: history.len() + 1,
        score,
        maximum_score,
        passing_score,
        passed,
        timestamp: env.ledger().timestamp(),
    };

    history.push_back(attempt.clone());
    env.storage().persistent().set(&key, &history);

    // The outcome lives in the topic, not the payload, so downstream reward and
    // progress logic can subscribe to passes alone without decoding every
    // attempt. Filtering on the leading `quiz` topic still yields all attempts.
    let outcome = if passed {
        symbol_short!("passed")
    } else {
        symbol_short!("failed")
    };
    env.events().publish(
        (symbol_short!("quiz"), outcome, quiz_id),
        (
            student,
            attempt.attempt_number,
            attempt.score,
            attempt.passing_score,
        ),
    );

    Ok(attempt)
}

/// Validates a submission and grades it against the required passing score.
///
/// This is the single place pass/fail is decided; `record_quiz_attempt` stores
/// whatever it returns. Grading is inclusive at the threshold — a score exactly
/// equal to `passing_score` passes.
///
/// Returns an error rather than a verdict when the submission could not be
/// graded meaningfully: a zero `maximum_score`, a `score` above the maximum, or
/// a `passing_score` above the maximum (which would make the quiz unpassable).
pub fn evaluate_score(
    score: u32,
    maximum_score: u32,
    passing_score: u32,
) -> Result<bool, QuizError> {
    if maximum_score == 0 {
        return Err(QuizError::InvalidMaximumScore);
    }
    if score > maximum_score {
        return Err(QuizError::ScoreOutOfRange);
    }
    if passing_score > maximum_score {
        return Err(QuizError::InvalidPassingScore);
    }

    Ok(score >= passing_score)
}

/// Returns every attempt a learner has made on a quiz, oldest first.
///
/// An empty vector is returned when the learner has no recorded attempts.
pub fn get_quiz_history(env: &Env, student: &Address, quiz_id: u64) -> Vec<QuizAttempt> {
    env.storage()
        .persistent()
        .get(&history_key(student, quiz_id))
        .unwrap_or_else(|| Vec::new(env))
}

/// Number of attempts a learner has made on a quiz.
pub fn get_attempt_count(env: &Env, student: &Address, quiz_id: u64) -> u32 {
    get_quiz_history(env, student, quiz_id).len()
}

/// Looks up a single attempt by its 1-based attempt number.
pub fn get_quiz_attempt(
    env: &Env,
    student: &Address,
    quiz_id: u64,
    attempt_number: u32,
) -> Option<QuizAttempt> {
    if attempt_number == 0 {
        return None;
    }
    get_quiz_history(env, student, quiz_id).get(attempt_number - 1)
}

/// Most recent attempt, or `None` when the learner has never attempted the quiz.
pub fn get_latest_attempt(env: &Env, student: &Address, quiz_id: u64) -> Option<QuizAttempt> {
    get_quiz_history(env, student, quiz_id).last()
}

/// Highest score across all attempts, or `None` when there are none.
pub fn get_best_score(env: &Env, student: &Address, quiz_id: u64) -> Option<u32> {
    let history = get_quiz_history(env, student, quiz_id);
    if history.is_empty() {
        return None;
    }

    let mut best = 0u32;
    for attempt in history.iter() {
        if attempt.score > best {
            best = attempt.score;
        }
    }
    Some(best)
}

/// Whether any attempt in the learner's history passed the quiz.
pub fn has_passed_quiz(env: &Env, student: &Address, quiz_id: u64) -> bool {
    get_quiz_history(env, student, quiz_id)
        .iter()
        .any(|attempt| attempt.passed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        contract, contractimpl,
        testutils::{Address as _, Events, Ledger},
        Env, Symbol, TryFromVal,
    };

    const MAX: u32 = 100;
    const PASS: u32 = 70;

    /// Storage and `require_auth` are only reachable from inside a running
    /// contract, so the tests drive the module through this host.
    #[contract]
    struct QuizHost;

    #[contractimpl]
    impl QuizHost {}

    /// Test harness that enters a fresh contract frame per call.
    ///
    /// One frame per submission matters: `require_auth` rejects a second
    /// authorization of the same address within one frame, so sharing a frame
    /// across attempts would not model a learner re-taking a quiz in separate
    /// transactions.
    struct Host {
        env: Env,
        id: Address,
    }

    impl Host {
        fn new() -> Self {
            let env = Env::default();
            env.mock_all_auths();
            let id = env.register(QuizHost, ());
            Host { env, id }
        }

        fn student(&self) -> Address {
            Address::generate(&self.env)
        }

        fn at(&self, timestamp: u64) {
            self.env.ledger().set_timestamp(timestamp);
        }

        fn try_record(
            &self,
            student: &Address,
            quiz_id: u64,
            score: u32,
            maximum_score: u32,
            passing_score: u32,
        ) -> Result<QuizAttempt, QuizError> {
            self.env.as_contract(&self.id, || {
                record_quiz_attempt(
                    self.env.clone(),
                    student.clone(),
                    quiz_id,
                    score,
                    maximum_score,
                    passing_score,
                )
            })
        }

        fn record(&self, student: &Address, quiz_id: u64, score: u32) -> QuizAttempt {
            self.try_record(student, quiz_id, score, MAX, PASS)
                .expect("attempt should be recorded")
        }

        fn history(&self, student: &Address, quiz_id: u64) -> Vec<QuizAttempt> {
            self.env
                .as_contract(&self.id, || get_quiz_history(&self.env, student, quiz_id))
        }

        fn count(&self, student: &Address, quiz_id: u64) -> u32 {
            self.env
                .as_contract(&self.id, || get_attempt_count(&self.env, student, quiz_id))
        }

        fn attempt(
            &self,
            student: &Address,
            quiz_id: u64,
            attempt_number: u32,
        ) -> Option<QuizAttempt> {
            self.env.as_contract(&self.id, || {
                get_quiz_attempt(&self.env, student, quiz_id, attempt_number)
            })
        }

        fn latest(&self, student: &Address, quiz_id: u64) -> Option<QuizAttempt> {
            self.env
                .as_contract(&self.id, || get_latest_attempt(&self.env, student, quiz_id))
        }

        fn best(&self, student: &Address, quiz_id: u64) -> Option<u32> {
            self.env
                .as_contract(&self.id, || get_best_score(&self.env, student, quiz_id))
        }

        fn passed(&self, student: &Address, quiz_id: u64) -> bool {
            self.env
                .as_contract(&self.id, || has_passed_quiz(&self.env, student, quiz_id))
        }

        /// Second topic of the most recent event — `passed` or `failed`.
        fn last_outcome_topic(&self) -> Symbol {
            let events = self.env.events().all();
            let (_, topics, _) = events.last().expect("an event should have been published");
            Symbol::try_from_val(&self.env, &topics.get(1).expect("outcome topic"))
                .expect("outcome topic should be a Symbol")
        }
    }

    #[test]
    fn records_a_single_attempt() {
        let host = Host::new();
        let student = host.student();

        host.at(1_000);
        let attempt = host.record(&student, 1, 85);

        assert_eq!(attempt.quiz_id, 1);
        assert_eq!(attempt.student, student);
        assert_eq!(attempt.attempt_number, 1);
        assert_eq!(attempt.score, 85);
        assert_eq!(attempt.maximum_score, MAX);
        assert_eq!(attempt.passing_score, PASS);
        assert!(attempt.passed);
        assert_eq!(attempt.timestamp, 1_000);

        assert_eq!(host.count(&student, 1), 1);
        assert_eq!(host.history(&student, 1).get(0), Some(attempt));
    }

    #[test]
    fn stores_multiple_attempts_in_order() {
        let host = Host::new();
        let student = host.student();
        let scores = [40u32, 65, 72, 95];

        for (index, score) in scores.iter().enumerate() {
            // Each attempt lands on a distinct ledger timestamp.
            host.at(500 * (index as u64 + 1));
            let attempt = host.record(&student, 7, *score);
            assert_eq!(attempt.attempt_number, index as u32 + 1);
        }

        let history = host.history(&student, 7);
        assert_eq!(history.len(), 4);
        assert_eq!(host.count(&student, 7), 4);

        for (index, score) in scores.iter().enumerate() {
            let stored = history.get(index as u32).expect("attempt should exist");
            assert_eq!(stored.attempt_number, index as u32 + 1);
            assert_eq!(stored.score, *score);
            assert_eq!(stored.passed, *score >= PASS);
            assert_eq!(stored.timestamp, 500 * (index as u64 + 1));
        }
    }

    #[test]
    fn history_is_preserved_across_later_attempts() {
        let host = Host::new();
        let student = host.student();

        host.at(100);
        let first = host.record(&student, 3, 30);

        host.at(200);
        host.record(&student, 3, 90);

        // The earliest entry must survive verbatim — no overwrite, no re-grade.
        let preserved = host.attempt(&student, 3, 1).expect("first attempt exists");
        assert_eq!(preserved, first);
        assert_eq!(preserved.score, 30);
        assert!(!preserved.passed);
        assert_eq!(preserved.timestamp, 100);

        // A failing attempt after a pass does not erase the pass either.
        host.at(300);
        host.record(&student, 3, 10);

        assert_eq!(host.count(&student, 3), 3);
        assert_eq!(host.attempt(&student, 3, 1), Some(first));
        assert!(host.attempt(&student, 3, 2).unwrap().passed);
        assert!(!host.attempt(&student, 3, 3).unwrap().passed);
    }

    #[test]
    fn a_passing_score_is_marked_passed() {
        let host = Host::new();
        let student = host.student();

        // Comfortably above the required 70.
        let attempt = host.record(&student, 1, 85);

        assert!(attempt.passed);
        assert_eq!(attempt.passing_score, PASS);
        assert_eq!(host.attempt(&student, 1, 1).unwrap().passed, true);
        assert!(host.passed(&student, 1));
    }

    #[test]
    fn a_failing_score_is_marked_failed() {
        let host = Host::new();
        let student = host.student();

        // Comfortably below the required 70.
        let attempt = host.record(&student, 1, 45);

        assert!(!attempt.passed);
        assert_eq!(attempt.passing_score, PASS);
        assert_eq!(host.attempt(&student, 1, 1).unwrap().passed, false);
        assert!(!host.passed(&student, 1));
    }

    #[test]
    fn grades_pass_and_fail_at_the_boundary() {
        let host = Host::new();
        let student = host.student();

        // The threshold itself passes; one point below it does not.
        assert!(!host.record(&student, 1, PASS - 1).passed);
        assert!(host.record(&student, 1, PASS).passed);
        assert!(host.record(&student, 1, MAX).passed);
        assert!(!host.record(&student, 1, 0).passed);
    }

    #[test]
    fn evaluate_score_grades_without_recording() {
        // Failing side of the threshold.
        assert_eq!(evaluate_score(0, MAX, PASS), Ok(false));
        assert_eq!(evaluate_score(PASS - 1, MAX, PASS), Ok(false));

        // Passing side, inclusive at the threshold.
        assert_eq!(evaluate_score(PASS, MAX, PASS), Ok(true));
        assert_eq!(evaluate_score(MAX, MAX, PASS), Ok(true));

        // A zero threshold passes everything, including a zero score.
        assert_eq!(evaluate_score(0, MAX, 0), Ok(true));

        // Ungradeable submissions yield an error, not a verdict.
        assert_eq!(evaluate_score(0, 0, 0), Err(QuizError::InvalidMaximumScore));
        assert_eq!(
            evaluate_score(MAX + 1, MAX, PASS),
            Err(QuizError::ScoreOutOfRange)
        );
        assert_eq!(
            evaluate_score(50, MAX, MAX + 1),
            Err(QuizError::InvalidPassingScore)
        );
    }

    #[test]
    fn event_topic_carries_the_outcome() {
        let host = Host::new();
        let student = host.student();

        host.record(&student, 1, 90);
        assert_eq!(host.last_outcome_topic(), symbol_short!("passed"));

        host.record(&student, 1, 30);
        assert_eq!(host.last_outcome_topic(), symbol_short!("failed"));

        // Boundary submissions are announced as passes.
        host.record(&student, 1, PASS);
        assert_eq!(host.last_outcome_topic(), symbol_short!("passed"));
    }

    #[test]
    fn histories_are_isolated_per_student_and_quiz() {
        let host = Host::new();
        let alice = host.student();
        let bob = host.student();

        host.record(&alice, 1, 80);
        host.record(&alice, 1, 90);
        host.record(&alice, 2, 60);
        host.record(&bob, 1, 50);

        assert_eq!(host.count(&alice, 1), 2);
        assert_eq!(host.count(&alice, 2), 1);
        assert_eq!(host.count(&bob, 1), 1);

        // Numbering restarts per (student, quiz) pair.
        assert_eq!(host.attempt(&alice, 2, 1).unwrap().attempt_number, 1);
        assert_eq!(host.attempt(&bob, 1, 1).unwrap().attempt_number, 1);

        // Bob's attempt did not land in Alice's history.
        assert_eq!(host.attempt(&alice, 1, 1).unwrap().score, 80);
        assert_eq!(host.attempt(&bob, 1, 1).unwrap().score, 50);
    }

    #[test]
    fn reports_latest_best_and_pass_state() {
        let host = Host::new();
        let student = host.student();

        assert_eq!(host.latest(&student, 1), None);
        assert_eq!(host.best(&student, 1), None);
        assert!(!host.passed(&student, 1));

        host.record(&student, 1, 55);
        assert_eq!(host.best(&student, 1), Some(55));
        assert!(!host.passed(&student, 1));

        host.record(&student, 1, 88);
        host.record(&student, 1, 20);

        // The latest attempt failed, but the earlier pass and best score stand.
        assert_eq!(host.latest(&student, 1).unwrap().score, 20);
        assert_eq!(host.best(&student, 1), Some(88));
        assert!(host.passed(&student, 1));
    }

    #[test]
    fn returns_empty_history_for_unknown_quiz() {
        let host = Host::new();
        let student = host.student();

        assert!(host.history(&student, 999).is_empty());
        assert_eq!(host.count(&student, 999), 0);
        assert_eq!(host.attempt(&student, 999, 1), None);
    }

    #[test]
    fn rejects_out_of_range_attempt_numbers() {
        let host = Host::new();
        let student = host.student();

        host.record(&student, 1, 75);

        assert_eq!(host.attempt(&student, 1, 0), None);
        assert_eq!(host.attempt(&student, 1, 2), None);
        assert!(host.attempt(&student, 1, 1).is_some());
    }

    #[test]
    fn rejects_invalid_scores_without_touching_history() {
        let host = Host::new();
        let student = host.student();

        host.record(&student, 1, 75);

        assert_eq!(
            host.try_record(&student, 1, 0, 0, 0),
            Err(QuizError::InvalidMaximumScore)
        );
        assert_eq!(
            host.try_record(&student, 1, MAX + 1, MAX, PASS),
            Err(QuizError::ScoreOutOfRange)
        );
        assert_eq!(
            host.try_record(&student, 1, 50, MAX, MAX + 1),
            Err(QuizError::InvalidPassingScore)
        );

        // Rejected submissions leave the existing history intact.
        assert_eq!(host.count(&student, 1), 1);
        assert_eq!(host.attempt(&student, 1, 1).unwrap().score, 75);
    }

    #[test]
    fn emits_one_event_per_recorded_attempt() {
        let host = Host::new();
        let student = host.student();

        host.record(&student, 1, 80);
        host.record(&student, 1, 90);

        let quiz_events = host
            .env
            .events()
            .all()
            .iter()
            .filter(|event| event.0 == host.id)
            .count();
        assert_eq!(quiz_events, 2);
    }
}

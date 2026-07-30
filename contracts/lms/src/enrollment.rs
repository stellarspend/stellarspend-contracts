use soroban_sdk::{contracttype, Address, Env};

use crate::errors::LmsError;
use crate::event::LMSEvents;
use crate::storage::StorageKey;

/// Describes a student's relationship to a course, derived from the
/// `Progress` and `Withdrawn` storage records (see [`crate::storage::StorageKey`]).
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EnrollmentStatus {
    /// No `Progress` record exists for this (student, course) pair — the
    /// student has never enrolled.
    NeverEnrolled = 0,
    /// A `Progress` record exists, progress is below 100%, and no
    /// `Withdrawn` record exists.
    Active = 1,
    /// `Progress` has reached 100% (the crate's existing completion
    /// convention — see `course::enroll_student`).
    Completed = 2,
    /// The student withdrew before completing the course (`Withdrawn`
    /// record present).
    Withdrawn = 3,
}

/// Derives a student's current enrollment status for a course from storage.
///
/// Precedence: an explicit `Withdrawn` record always wins (this contract has
/// no "re-enroll" flow, so a withdrawal is terminal), then 100% progress
/// means `Completed`, then a bare `Progress` record means `Active`, and the
/// absence of any record means `NeverEnrolled`.
pub fn enrollment_status(env: &Env, student: &Address, course_id: u64) -> EnrollmentStatus {
    if env
        .storage()
        .persistent()
        .has(&StorageKey::Withdrawn(student.clone(), course_id))
    {
        return EnrollmentStatus::Withdrawn;
    }

    let progress: Option<u32> = env
        .storage()
        .persistent()
        .get(&StorageKey::Progress(student.clone(), course_id));

    match progress {
        Some(100) => EnrollmentStatus::Completed,
        Some(_) => EnrollmentStatus::Active,
        None => EnrollmentStatus::NeverEnrolled,
    }
}

/// Returns the current enrollment status for `student` in `course_id`. Read
/// only, no authorization required.
pub fn get_enrollment_status(env: Env, student: Address, course_id: u64) -> EnrollmentStatus {
    enrollment_status(&env, &student, course_id)
}

/// Withdraws `student` from `course_id`, removing their *active* enrollment
/// while preserving historical audit information.
///
/// Rather than deleting the `Progress` record (which would make a withdrawn
/// student indistinguishable from one who never enrolled), this leaves the
/// `Progress` value untouched — so the progress percentage at the moment of
/// withdrawal remains queryable — and writes a separate `Withdrawn` marker
/// storing the withdrawal timestamp. [`enrollment_status`] treats the
/// presence of that marker as taking precedence over the raw progress value.
///
/// Only the student themselves may withdraw (`student.require_auth()`).
///
/// # Errors
/// - `LmsError::NotEnrolled` — the student has no `Progress` record (never
///   enrolled), or has already withdrawn (a withdrawal is terminal; a
///   student who withdraws is no longer "actively enrolled" and withdrawing
///   again is meaningless).
/// - `LmsError::AlreadyCompleted` — the student's progress is already 100%.
///   This reuses the existing `LmsError::AlreadyCompleted` variant rather
///   than adding a new one: its name and intent ("this course is already
///   completed for this student") match the "prevent withdrawal after
///   completion" requirement exactly, and it was clearly already reserved
///   for progress-related completion checks.
pub fn withdraw_student(env: Env, student: Address, course_id: u64) -> Result<(), LmsError> {
    student.require_auth();

    match enrollment_status(&env, &student, course_id) {
        EnrollmentStatus::NeverEnrolled | EnrollmentStatus::Withdrawn => {
            return Err(LmsError::NotEnrolled);
        }
        EnrollmentStatus::Completed => {
            return Err(LmsError::AlreadyCompleted);
        }
        EnrollmentStatus::Active => {}
    }

    let withdrawn_key = StorageKey::Withdrawn(student.clone(), course_id);
    let now = env.ledger().timestamp();
    env.storage().persistent().set(&withdrawn_key, &now);

    LMSEvents::emit_student_withdrawn(&env, course_id, student);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Events, Ledger};
    use soroban_sdk::Env;

    /// Registers a real `LMSContract` instance purely so free functions in
    /// this module can be exercised under a contract execution frame (Soroban
    /// requires one for `env.storage()`/`env.events()`/`require_auth()`).
    fn setup(env: &Env) -> Address {
        env.register(crate::LMSContract, ())
    }

    fn enroll(env: &Env, student: &Address, course_id: u64, progress: u32) {
        env.storage()
            .persistent()
            .set(&StorageKey::Progress(student.clone(), course_id), &progress);
    }

    #[test]
    fn test_withdraw_success() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);

        let student = Address::generate(&env);
        let course_id = 1u64;

        let (result, status, progress, event_count) = env.as_contract(&contract_id, || {
            enroll(&env, &student, course_id, 0);

            let result = withdraw_student(env.clone(), student.clone(), course_id);
            let status = get_enrollment_status(env.clone(), student.clone(), course_id);
            let progress: u32 = env
                .storage()
                .persistent()
                .get(&StorageKey::Progress(student.clone(), course_id))
                .unwrap();
            let event_count = env.events().all().len();

            (result, status, progress, event_count)
        });

        assert!(result.is_ok());
        assert_eq!(status, EnrollmentStatus::Withdrawn);
        // Historical progress value at time of withdrawal is preserved.
        assert_eq!(progress, 0);
        assert_eq!(event_count, 1);
    }

    #[test]
    fn test_withdraw_without_enrollment_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);

        let student = Address::generate(&env);
        let course_id = 1u64;

        let result = env.as_contract(&contract_id, || {
            withdraw_student(env.clone(), student.clone(), course_id)
        });

        assert_eq!(result, Err(LmsError::NotEnrolled));
    }

    #[test]
    fn test_withdraw_completed_course_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);

        let student = Address::generate(&env);
        let course_id = 1u64;

        let result = env.as_contract(&contract_id, || {
            enroll(&env, &student, course_id, 100);
            withdraw_student(env.clone(), student.clone(), course_id)
        });

        assert_eq!(result, Err(LmsError::AlreadyCompleted));
    }

    #[test]
    fn test_double_withdrawal_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);

        let student = Address::generate(&env);
        let course_id = 1u64;

        // Two separate frames: a second `require_auth()` for the same
        // address within one frame trips `Auth::ExistingValue` under
        // `mock_all_auths()` (see `course::test_archive_already_archived_course_fails`).
        env.as_contract(&contract_id, || {
            enroll(&env, &student, course_id, 40);
            withdraw_student(env.clone(), student.clone(), course_id).unwrap();
        });

        let second_result = env.as_contract(&contract_id, || {
            withdraw_student(env.clone(), student.clone(), course_id)
        });

        assert_eq!(second_result, Err(LmsError::NotEnrolled));
    }

    #[test]
    #[should_panic]
    fn test_withdraw_requires_student_auth() {
        let env = Env::default();
        // No `mock_all_auths()` — only the student's own signature should
        // authorize this call, and none is mocked here, so `require_auth()`
        // must panic.
        let contract_id = setup(&env);

        let student = Address::generate(&env);
        let course_id = 1u64;

        env.as_contract(&contract_id, || {
            enroll(&env, &student, course_id, 0);
            let _ = withdraw_student(env.clone(), student.clone(), course_id);
        });
    }

    #[test]
    fn test_enrollment_status_transitions() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000);
        let contract_id = setup(&env);

        let never_enrolled = Address::generate(&env);
        let active_student = Address::generate(&env);
        let completed_student = Address::generate(&env);
        let withdrawn_student = Address::generate(&env);
        let course_id = 7u64;

        let (never, active, completed, withdrawn) = env.as_contract(&contract_id, || {
            enroll(&env, &active_student, course_id, 42);
            enroll(&env, &completed_student, course_id, 100);
            enroll(&env, &withdrawn_student, course_id, 10);
            withdraw_student(env.clone(), withdrawn_student.clone(), course_id).unwrap();

            (
                get_enrollment_status(env.clone(), never_enrolled.clone(), course_id),
                get_enrollment_status(env.clone(), active_student.clone(), course_id),
                get_enrollment_status(env.clone(), completed_student.clone(), course_id),
                get_enrollment_status(env.clone(), withdrawn_student.clone(), course_id),
            )
        });

        assert_eq!(never, EnrollmentStatus::NeverEnrolled);
        assert_eq!(active, EnrollmentStatus::Active);
        assert_eq!(completed, EnrollmentStatus::Completed);
        assert_eq!(withdrawn, EnrollmentStatus::Withdrawn);
    }
}

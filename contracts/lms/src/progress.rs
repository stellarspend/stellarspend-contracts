//! Lesson completion tracking and course progress calculation.
//!
//! Implements three closely related, same-file features (see the PR
//! description for why they're combined):
//! - #1028 Lesson completion tracking (`complete_lesson`)
//! - #1029 Course progress percentage (`get_course_progress`)
//! - #1030 Duplicate completion prevention (built into `complete_lesson`)
//!
//! ## The lesson-registry gap
//!
//! Nothing elsewhere in this crate creates or stores a `Lesson` record —
//! `models::Lesson` exists only as a type exercised by its own unit tests.
//! Both "validate lesson exists" (#1028) and "total lessons for a course"
//! (#1029) are meaningless without a real registry to check against, so this
//! module adds a minimal one: [`register_lesson`] stores a `models::Lesson`
//! under the crate's existing (until now unused) `StorageKey::Lesson(u64)`
//! key and increments a per-course lesson counter. `models::Lesson` is reused
//! as-is rather than inventing a lighter type; fields not meaningful for this
//! minimal registration (`description`, `content_uri`, `estimated_duration`,
//! `lesson_order`) are filled with sensible placeholder defaults (empty
//! string / 0) since nothing in this crate reads them yet.

use soroban_sdk::{Address, Env, String};

use crate::course;
use crate::errors::LmsError;
use crate::event::LMSEvents;
use crate::models::Lesson;
use crate::storage::StorageKey;

/// Registers a new lesson under `course_id`. Only the course's owning
/// instructor may register lessons for it (mirroring `course.rs`'s own
/// ownership convention for mutating operations).
///
/// This is the minimal lesson-registration mechanism this module adds to
/// give `complete_lesson`'s "validate lesson exists" (#1028) and
/// `get_course_progress`'s "total lessons" (#1029) a real source of truth,
/// since nothing else in the crate creates `Lesson` records.
pub fn register_lesson(
    env: Env,
    caller: Address,
    course_id: u64,
    lesson_id: u64,
    title: String,
) -> Result<(), LmsError> {
    caller.require_auth();

    let course = course::get_course(env.clone(), course_id).map_err(|_| LmsError::CourseNotFound)?;
    if course.instructor != caller {
        return Err(LmsError::Unauthorized);
    }

    let lesson_key = StorageKey::Lesson(lesson_id);
    if env.storage().persistent().has(&lesson_key) {
        return Err(LmsError::LessonAlreadyExists);
    }

    let lesson = Lesson {
        lesson_id,
        course_id,
        title: title.clone(),
        description: String::from_str(&env, ""),
        content_uri: String::from_str(&env, ""),
        estimated_duration: 0,
        lesson_order: 0,
    };
    env.storage().persistent().set(&lesson_key, &lesson);

    let count_key = StorageKey::CourseLessonCount(course_id);
    let count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);
    let new_count = count.checked_add(1).ok_or(LmsError::Overflow)?;
    env.storage().persistent().set(&count_key, &new_count);

    LMSEvents::emit_lesson_added(&env, course_id, lesson_id, title);

    Ok(())
}

/// Marks `lesson_id` (which must belong to `course_id`) as completed by
/// `student`.
///
/// Checks, in order:
/// 1. The course exists (`LmsError::CourseNotFound`).
/// 2. The student is enrolled in it, i.e. `StorageKey::Progress(student,
///    course_id)` exists (`LmsError::NotEnrolled`) — same enrollment-presence
///    convention used elsewhere in this crate (see `course::enroll_student`).
/// 3. The lesson exists and belongs to `course_id` (`LmsError::LessonNotFound`
///    for either case — a nonexistent lesson and a lesson registered under a
///    different course are indistinguishable from the caller's perspective).
/// 4. The student hasn't already completed it (`LmsError::AlreadyCompleted`)
///    — this is what makes #1030 (duplicate prevention) hold: presence of
///    `StorageKey::LessonCompletion(student, lesson_id)` is treated as the
///    single source of truth for "already completed".
///
/// On success: records the completion timestamp, increments the student's
/// per-course completed-lesson counter (the numerator `get_course_progress`
/// reads), and emits `LessonCompleted` via the existing
/// `LMSEvents::emit_lesson_completed` helper.
pub fn complete_lesson(
    env: Env,
    student: Address,
    course_id: u64,
    lesson_id: u64,
) -> Result<(), LmsError> {
    student.require_auth();

    course::get_course(env.clone(), course_id).map_err(|_| LmsError::CourseNotFound)?;

    let enrollment_key = StorageKey::Progress(student.clone(), course_id);
    if !env.storage().persistent().has(&enrollment_key) {
        return Err(LmsError::NotEnrolled);
    }

    let lesson: Lesson = env
        .storage()
        .persistent()
        .get(&StorageKey::Lesson(lesson_id))
        .ok_or(LmsError::LessonNotFound)?;
    if lesson.course_id != course_id {
        return Err(LmsError::LessonNotFound);
    }

    let completion_key = StorageKey::LessonCompletion(student.clone(), lesson_id);
    if env.storage().persistent().has(&completion_key) {
        return Err(LmsError::AlreadyCompleted);
    }

    let now = env.ledger().timestamp();
    env.storage().persistent().set(&completion_key, &now);

    let completed_key = StorageKey::CompletedLessonCount(student.clone(), course_id);
    let completed: u32 = env.storage().persistent().get(&completed_key).unwrap_or(0);
    let new_completed = completed.checked_add(1).ok_or(LmsError::Overflow)?;
    env.storage().persistent().set(&completed_key, &new_completed);

    LMSEvents::emit_lesson_completed(&env, course_id, lesson_id, student);

    Ok(())
}

/// Returns `student`'s completion percentage for `course_id` as an integer in
/// `0..=100`.
///
/// Formula: `completed_lessons * 100 / total_lessons`, both read from the
/// running counters maintained by [`register_lesson`] (denominator) and
/// [`complete_lesson`] (numerator) rather than by enumerating every
/// registered lesson on each query — cheaper and simpler for this bounded,
/// small-cardinality use case.
///
/// Rounding: plain integer division, which truncates toward zero. E.g. 1 of 3
/// lessons completed yields `100 / 3 = 33` (not 33.33 and not rounded up to
/// 34) — floating-point is never used, per Soroban/WASM contract discipline.
///
/// Division-by-zero: if the course has zero registered lessons, returns `0`
/// rather than dividing by zero — 0% of nothing is a safe, unsurprising
/// default. The zero-total case is handled with an explicit early return, so
/// the actual division is only ever reached once `total > 0` is known, making
/// a `checked_div` there redundant.
pub fn get_course_progress(env: Env, student: Address, course_id: u64) -> Result<u32, LmsError> {
    course::get_course(env.clone(), course_id).map_err(|_| LmsError::CourseNotFound)?;

    let total: u32 = env
        .storage()
        .persistent()
        .get(&StorageKey::CourseLessonCount(course_id))
        .unwrap_or(0);

    if total == 0 {
        return Ok(0);
    }

    let completed: u32 = env
        .storage()
        .persistent()
        .get(&StorageKey::CompletedLessonCount(student, course_id))
        .unwrap_or(0);

    let numerator = completed.checked_mul(100).ok_or(LmsError::Overflow)?;
    Ok(numerator / total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::course::Course;
    use soroban_sdk::testutils::{Address as _, Events, Ledger};

    fn setup(env: &Env) -> Address {
        env.register(crate::LMSContract, ())
    }

    /// Writes a `Course` directly to storage, bypassing `create_course`'s
    /// `require_auth()` so test setup never "spends" an instructor's one
    /// allowed auth per `as_contract` frame (see the module-level auth-frame
    /// notes in `course.rs`).
    fn setup_course(env: &Env, instructor: &Address, course_id: u64) {
        let now = env.ledger().timestamp();
        let course = Course {
            id: course_id,
            instructor: instructor.clone(),
            title: String::from_str(env, "Course"),
            description: String::from_str(env, "Description"),
            category: String::from_str(env, "Category"),
            difficulty: 1,
            thumbnail: String::from_str(env, "https://example.com/thumb.png"),
            published: true,
            archived: false,
            created_at: now,
            updated_at: now,
        };
        env.storage()
            .persistent()
            .set(&StorageKey::Course(course_id), &course);
    }

    /// Writes an enrollment record directly, bypassing `enroll_student`'s
    /// `require_auth()` for the same reason as `setup_course`.
    fn enroll(env: &Env, student: &Address, course_id: u64) {
        env.storage()
            .persistent()
            .set(&StorageKey::Progress(student.clone(), course_id), &0u32);
    }

    #[test]
    fn test_register_lesson_success() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);
        let instructor = Address::generate(&env);
        let course_id = 1u64;

        let (lesson, count, event_count) = env.as_contract(&contract_id, || {
            setup_course(&env, &instructor, course_id);
            register_lesson(
                env.clone(),
                instructor.clone(),
                course_id,
                10,
                String::from_str(&env, "Intro"),
            )
            .expect("registration should succeed");

            let lesson: Lesson = env
                .storage()
                .persistent()
                .get(&StorageKey::Lesson(10))
                .unwrap();
            let count: u32 = env
                .storage()
                .persistent()
                .get(&StorageKey::CourseLessonCount(course_id))
                .unwrap();
            let event_count = env.events().all().len();
            (lesson, count, event_count)
        });

        assert_eq!(lesson.lesson_id, 10);
        assert_eq!(lesson.course_id, course_id);
        assert_eq!(count, 1);
        assert_eq!(event_count, 1);
    }

    #[test]
    fn test_register_lesson_course_not_found() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);
        let instructor = Address::generate(&env);

        let result = env.as_contract(&contract_id, || {
            register_lesson(
                env.clone(),
                instructor,
                999u64,
                10,
                String::from_str(&env, "Intro"),
            )
        });

        assert_eq!(result, Err(LmsError::CourseNotFound));
    }

    #[test]
    fn test_register_lesson_unauthorized() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);
        let instructor = Address::generate(&env);
        let stranger = Address::generate(&env);
        let course_id = 1u64;

        let result = env.as_contract(&contract_id, || {
            setup_course(&env, &instructor, course_id);
            register_lesson(
                env.clone(),
                stranger,
                course_id,
                10,
                String::from_str(&env, "Intro"),
            )
        });

        assert_eq!(result, Err(LmsError::Unauthorized));
    }

    #[test]
    fn test_register_lesson_duplicate_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);
        let instructor = Address::generate(&env);
        let course_id = 1u64;

        // Two separate frames: registering twice as the same instructor
        // address would otherwise `require_auth()` that address twice within
        // one frame, tripping `Auth::ExistingValue`.
        env.as_contract(&contract_id, || {
            setup_course(&env, &instructor, course_id);
            register_lesson(
                env.clone(),
                instructor.clone(),
                course_id,
                10,
                String::from_str(&env, "Intro"),
            )
            .unwrap();
        });

        let result = env.as_contract(&contract_id, || {
            register_lesson(
                env.clone(),
                instructor,
                course_id,
                10,
                String::from_str(&env, "Intro Again"),
            )
        });

        assert_eq!(result, Err(LmsError::LessonAlreadyExists));
    }

    #[test]
    fn test_complete_lesson_success() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000);
        let contract_id = setup(&env);
        let instructor = Address::generate(&env);
        let student = Address::generate(&env);
        let course_id = 1u64;
        let lesson_id = 10u64;

        let (timestamp, completed_count, event_count) = env.as_contract(&contract_id, || {
            setup_course(&env, &instructor, course_id);
            enroll(&env, &student, course_id);
            register_lesson(
                env.clone(),
                instructor.clone(),
                course_id,
                lesson_id,
                String::from_str(&env, "Intro"),
            )
            .unwrap();

            complete_lesson(env.clone(), student.clone(), course_id, lesson_id)
                .expect("completion should succeed");

            let timestamp: u64 = env
                .storage()
                .persistent()
                .get(&StorageKey::LessonCompletion(student.clone(), lesson_id))
                .unwrap();
            let completed_count: u32 = env
                .storage()
                .persistent()
                .get(&StorageKey::CompletedLessonCount(student.clone(), course_id))
                .unwrap();
            let event_count = env.events().all().len();
            (timestamp, completed_count, event_count)
        });

        assert_eq!(timestamp, 1_000);
        assert_eq!(completed_count, 1);
        // 2 events in this frame: `lesson_added` (from `register_lesson`) and
        // `lesson_complete` (from `complete_lesson`).
        assert_eq!(event_count, 2);
    }

    #[test]
    fn test_complete_lesson_duplicate_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);
        let instructor = Address::generate(&env);
        let student = Address::generate(&env);
        let course_id = 1u64;
        let lesson_id = 10u64;

        // First completion (its own frame, since the *second* attempt below
        // also needs to `require_auth()` the same student address).
        env.as_contract(&contract_id, || {
            setup_course(&env, &instructor, course_id);
            enroll(&env, &student, course_id);
            register_lesson(
                env.clone(),
                instructor,
                course_id,
                lesson_id,
                String::from_str(&env, "Intro"),
            )
            .unwrap();
            complete_lesson(env.clone(), student.clone(), course_id, lesson_id).unwrap();
        });

        let result = env.as_contract(&contract_id, || {
            complete_lesson(env.clone(), student, course_id, lesson_id)
        });

        assert_eq!(result, Err(LmsError::AlreadyCompleted));
    }

    #[test]
    fn test_complete_lesson_non_enrolled_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);
        let instructor = Address::generate(&env);
        let student = Address::generate(&env);
        let course_id = 1u64;
        let lesson_id = 10u64;

        let result = env.as_contract(&contract_id, || {
            setup_course(&env, &instructor, course_id);
            register_lesson(
                env.clone(),
                instructor,
                course_id,
                lesson_id,
                String::from_str(&env, "Intro"),
            )
            .unwrap();
            // Note: `student` is never enrolled here.
            complete_lesson(env.clone(), student, course_id, lesson_id)
        });

        assert_eq!(result, Err(LmsError::NotEnrolled));
    }

    #[test]
    fn test_complete_lesson_course_not_found() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);
        let student = Address::generate(&env);

        let result = env.as_contract(&contract_id, || {
            complete_lesson(env.clone(), student, 999u64, 10u64)
        });

        assert_eq!(result, Err(LmsError::CourseNotFound));
    }

    #[test]
    fn test_complete_lesson_lesson_not_found() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);
        let instructor = Address::generate(&env);
        let student = Address::generate(&env);
        let course_id = 1u64;

        let result = env.as_contract(&contract_id, || {
            setup_course(&env, &instructor, course_id);
            enroll(&env, &student, course_id);
            // Lesson 999 was never registered.
            complete_lesson(env.clone(), student, course_id, 999u64)
        });

        assert_eq!(result, Err(LmsError::LessonNotFound));
    }

    #[test]
    fn test_complete_lesson_wrong_course_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);
        let instructor = Address::generate(&env);
        let student = Address::generate(&env);
        let course_a = 1u64;
        let course_b = 2u64;
        let lesson_id = 10u64;

        let result = env.as_contract(&contract_id, || {
            setup_course(&env, &instructor, course_a);
            setup_course(&env, &instructor, course_b);
            enroll(&env, &student, course_b);
            // Lesson is registered under course_a...
            register_lesson(
                env.clone(),
                instructor,
                course_a,
                lesson_id,
                String::from_str(&env, "Intro"),
            )
            .unwrap();
            // ...but the student tries to complete it against course_b.
            complete_lesson(env.clone(), student, course_b, lesson_id)
        });

        assert_eq!(result, Err(LmsError::LessonNotFound));
    }

    #[test]
    fn test_get_course_progress_partial_truncates() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);
        let instructor = Address::generate(&env);
        let student = Address::generate(&env);
        let course_id = 1u64;

        env.as_contract(&contract_id, || {
            setup_course(&env, &instructor, course_id);
            enroll(&env, &student, course_id);
            register_lesson(
                env.clone(),
                instructor.clone(),
                course_id,
                1,
                String::from_str(&env, "L1"),
            )
            .unwrap();
        });
        env.as_contract(&contract_id, || {
            register_lesson(
                env.clone(),
                instructor.clone(),
                course_id,
                2,
                String::from_str(&env, "L2"),
            )
            .unwrap();
        });
        env.as_contract(&contract_id, || {
            register_lesson(
                env.clone(),
                instructor,
                course_id,
                3,
                String::from_str(&env, "L3"),
            )
            .unwrap();
        });

        // Complete exactly 1 of 3 lessons: 1 * 100 / 3 = 33 (truncated, not
        // rounded to 33.33 or up to 34).
        env.as_contract(&contract_id, || {
            complete_lesson(env.clone(), student.clone(), course_id, 1).unwrap();
        });

        let progress = env.as_contract(&contract_id, || {
            get_course_progress(env.clone(), student, course_id).unwrap()
        });

        assert_eq!(progress, 33);
    }

    #[test]
    fn test_get_course_progress_full_completion() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);
        let instructor = Address::generate(&env);
        let student = Address::generate(&env);
        let course_id = 1u64;

        env.as_contract(&contract_id, || {
            setup_course(&env, &instructor, course_id);
            enroll(&env, &student, course_id);
            register_lesson(
                env.clone(),
                instructor.clone(),
                course_id,
                1,
                String::from_str(&env, "L1"),
            )
            .unwrap();
        });
        // Separate frame: registering a second lesson as the same
        // `instructor` address within the prior frame would `require_auth()`
        // that address twice in one frame, tripping `Auth::ExistingValue`.
        env.as_contract(&contract_id, || {
            register_lesson(
                env.clone(),
                instructor.clone(),
                course_id,
                2,
                String::from_str(&env, "L2"),
            )
            .unwrap();
        });

        env.as_contract(&contract_id, || {
            complete_lesson(env.clone(), student.clone(), course_id, 1).unwrap();
        });
        env.as_contract(&contract_id, || {
            complete_lesson(env.clone(), student.clone(), course_id, 2).unwrap();
        });

        let progress = env.as_contract(&contract_id, || {
            get_course_progress(env.clone(), student, course_id).unwrap()
        });

        assert_eq!(progress, 100);
    }

    #[test]
    fn test_get_course_progress_empty_course_returns_zero() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);
        let instructor = Address::generate(&env);
        let student = Address::generate(&env);
        let course_id = 1u64;

        let progress = env.as_contract(&contract_id, || {
            setup_course(&env, &instructor, course_id);
            // No lessons registered at all.
            get_course_progress(env.clone(), student, course_id).unwrap()
        });

        assert_eq!(progress, 0);
    }

    #[test]
    fn test_get_course_progress_course_not_found() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);
        let student = Address::generate(&env);

        let result = env.as_contract(&contract_id, || {
            get_course_progress(env.clone(), student, 999u64)
        });

        assert_eq!(result, Err(LmsError::CourseNotFound));
    }
}

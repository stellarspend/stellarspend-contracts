use soroban_sdk::{contracterror, contracttype, Address, Env, String};

use crate::event::LMSEvents;
use crate::storage::StorageKey;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum CourseError {
    CourseNotFound = 1,
    AlreadyPublished = 2,
    Unauthorized = 3,
    InvalidInput = 4,
    AlreadyArchived = 5,
    CourseIsArchived = 6,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Course {
    pub id: u64,
    pub instructor: Address,
    pub title: String,
    pub description: String,
    pub category: String,
    pub difficulty: u32,
    pub thumbnail: String,
    pub published: bool,
    pub archived: bool,
    pub created_at: u64,
    pub updated_at: u64,
}

/// Partial update payload for [`update_course`]. `None` fields are left unchanged.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateCourseInput {
    pub title: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub difficulty: Option<u32>,
    pub thumbnail: Option<String>,
    pub published: Option<bool>,
}

fn validate_string(s: &String) -> bool {
    s.len() > 0
}

/// Creates a new, unpublished (draft) course owned by `instructor`.
pub fn create_course(
    env: Env,
    instructor: Address,
    title: String,
    description: String,
    category: String,
    difficulty: u32,
    thumbnail: String,
) -> Result<u64, CourseError> {
    instructor.require_auth();

    if !validate_string(&title)
        || !validate_string(&description)
        || !validate_string(&category)
        || !validate_string(&thumbnail)
    {
        return Err(CourseError::InvalidInput);
    }

    let id_key = StorageKey::NextCourseId;
    let course_id: u64 = env.storage().persistent().get(&id_key).unwrap_or(1);
    env.storage().persistent().set(&id_key, &(course_id + 1));

    let now = env.ledger().timestamp();
    let course = Course {
        id: course_id,
        instructor: instructor.clone(),
        title: title.clone(),
        description,
        category,
        difficulty,
        thumbnail,
        published: false,
        archived: false,
        created_at: now,
        updated_at: now,
    };

    env.storage()
        .persistent()
        .set(&StorageKey::Course(course_id), &course);

    LMSEvents::emit_course_created(&env, course_id, instructor, title);

    Ok(course_id)
}

/// Retrieves course information by ID.
///
/// Returns `CourseError::CourseNotFound` if the course ID does not exist in storage.
pub fn get_course(env: Env, course_id: u64) -> Result<Course, CourseError> {
    env.storage()
        .persistent()
        .get(&StorageKey::Course(course_id))
        .ok_or(CourseError::CourseNotFound)
}

/// Applies a partial update to an existing course. Only the fields set to `Some`
/// in `input` are changed; everything else is left as-is.
pub fn update_course(
    env: Env,
    caller: Address,
    course_id: u64,
    input: UpdateCourseInput,
) -> Result<Course, CourseError> {
    caller.require_auth();

    let key = StorageKey::Course(course_id);
    let mut course: Course = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(CourseError::CourseNotFound)?;

    if course.instructor != caller {
        return Err(CourseError::Unauthorized);
    }

    if let Some(title) = input.title {
        course.title = title;
    }
    if let Some(description) = input.description {
        course.description = description;
    }
    if let Some(category) = input.category {
        course.category = category;
    }
    if let Some(difficulty) = input.difficulty {
        course.difficulty = difficulty;
    }
    if let Some(thumbnail) = input.thumbnail {
        course.thumbnail = thumbnail;
    }
    if let Some(published) = input.published {
        course.published = published;
    }

    course.updated_at = env.ledger().timestamp();
    env.storage().persistent().set(&key, &course);

    Ok(course)
}

/// Publishes a draft course, making it visible/enrollable.
pub fn publish_course(env: Env, caller: Address, course_id: u64) -> Result<(), CourseError> {
    caller.require_auth();

    let key = StorageKey::Course(course_id);
    let mut course: Course = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(CourseError::CourseNotFound)?;

    if course.instructor != caller {
        return Err(CourseError::Unauthorized);
    }

    if course.published {
        return Err(CourseError::AlreadyPublished);
    }

    course.published = true;
    course.updated_at = env.ledger().timestamp();
    env.storage().persistent().set(&key, &course);

    LMSEvents::emit_course_published(&env, course_id, caller);

    Ok(())
}

/// Marks a course as archived.
///
/// Historical records and learner progress remain in storage, but future
/// enrollments are blocked (see [`enroll_student`]).
pub fn archive_course(env: Env, caller: Address, course_id: u64) -> Result<(), CourseError> {
    caller.require_auth();

    let key = StorageKey::Course(course_id);
    let mut course: Course = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(CourseError::CourseNotFound)?;

    if course.instructor != caller {
        return Err(CourseError::Unauthorized);
    }

    if course.archived {
        return Err(CourseError::AlreadyArchived);
    }

    course.archived = true;
    course.updated_at = env.ledger().timestamp();
    env.storage().persistent().set(&key, &course);

    LMSEvents::emit_course_archived(&env, course_id, caller);

    Ok(())
}

/// Enrolls `student` in `course_id`, initializing their progress record at 0%.
///
/// Blocked once a course has been archived (see [`archive_course`]).
pub fn enroll_student(env: Env, student: Address, course_id: u64) -> Result<(), CourseError> {
    student.require_auth();

    let key = StorageKey::Course(course_id);
    let course: Course = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(CourseError::CourseNotFound)?;

    if course.archived {
        return Err(CourseError::CourseIsArchived);
    }

    let progress_key = StorageKey::Progress(student.clone(), course_id);
    env.storage().persistent().set(&progress_key, &0u32);

    LMSEvents::emit_student_enrolled(&env, course_id, student);

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

    fn setup_course(env: &Env, instructor: &Address, course_id: u64, published: bool) -> Course {
        let now = env.ledger().timestamp();
        let course = Course {
            id: course_id,
            instructor: instructor.clone(),
            title: String::from_str(env, "Old Title"),
            description: String::from_str(env, "Old Description"),
            category: String::from_str(env, "Old Category"),
            difficulty: 1,
            thumbnail: String::from_str(env, "https://old.png"),
            published,
            archived: false,
            created_at: now,
            updated_at: now,
        };

        env.storage()
            .persistent()
            .set(&StorageKey::Course(course_id), &course);

        course
    }

    #[test]
    fn test_create_course_success() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(500_000);
        let contract_id = setup(&env);

        let instructor = Address::generate(&env);

        let title = String::from_str(&env, "Introduction to Soroban");
        let description = String::from_str(&env, "Learn Rust and Stellar smart contracts.");
        let category = String::from_str(&env, "Blockchain");
        let thumbnail = String::from_str(&env, "https://example.com/thumb.png");

        let (course_id, stored_course, event_count) = env.as_contract(&contract_id, || {
            let course_id = create_course(
                env.clone(),
                instructor.clone(),
                title.clone(),
                description.clone(),
                category.clone(),
                1,
                thumbnail.clone(),
            )
            .expect("Course creation should succeed");

            let stored_course =
                get_course(env.clone(), course_id).expect("Course should exist");
            let event_count = env.events().all().len();

            (course_id, stored_course, event_count)
        });

        assert_eq!(course_id, 1);
        assert_eq!(stored_course.id, 1);
        assert_eq!(stored_course.instructor, instructor);
        assert_eq!(stored_course.title, title);
        assert_eq!(stored_course.created_at, 500_000);
        assert!(!stored_course.published);
        assert!(!stored_course.archived);
        assert_eq!(event_count, 1);
    }

    #[test]
    fn test_create_course_unique_ids() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);

        let instructor = Address::generate(&env);
        let title = String::from_str(&env, "Course Title");
        let desc = String::from_str(&env, "Course Description");
        let cat = String::from_str(&env, "Category");
        let thumb = String::from_str(&env, "https://example.com/thumb.png");

        // Each call is wrapped in its own `as_contract` frame: under
        // `mock_all_auths()`, two `require_auth()` calls for the *same*
        // address within a single frame trip `Auth::ExistingValue` ("frame
        // is already authorized"). Separate frames mirror two separate
        // top-level invocations, which is what actually happens on-chain.
        let id_1 = env.as_contract(&contract_id, || {
            create_course(
                env.clone(),
                instructor.clone(),
                title.clone(),
                desc.clone(),
                cat.clone(),
                1,
                thumb.clone(),
            )
            .unwrap()
        });
        let id_2 = env.as_contract(&contract_id, || {
            create_course(env.clone(), instructor, title, desc, cat, 2, thumb).unwrap()
        });

        assert_ne!(id_1, id_2);
        assert_eq!(id_1, 1);
        assert_eq!(id_2, 2);
    }

    #[test]
    fn test_create_course_invalid_input_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);

        let instructor = Address::generate(&env);
        let empty_title = String::from_str(&env, "");
        let desc = String::from_str(&env, "Valid Description");
        let cat = String::from_str(&env, "Valid Category");
        let thumb = String::from_str(&env, "https://example.com/thumb.png");

        let result = env.as_contract(&contract_id, || {
            create_course(env.clone(), instructor, empty_title, desc, cat, 1, thumb)
        });

        assert_eq!(result, Err(CourseError::InvalidInput));
    }

    #[test]
    fn test_get_missing_course_fails() {
        let env = Env::default();
        let contract_id = setup(&env);

        let result = env.as_contract(&contract_id, || get_course(env.clone(), 9999u64));
        assert_eq!(result, Err(CourseError::CourseNotFound));
    }

    #[test]
    fn test_publish_draft_course_success() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);

        let instructor = Address::generate(&env);
        let course_id = 1u64;

        let updated = env.as_contract(&contract_id, || {
            setup_course(&env, &instructor, course_id, false);
            let result = publish_course(env.clone(), instructor.clone(), course_id);
            assert!(result.is_ok());
            get_course(env.clone(), course_id).unwrap()
        });

        assert!(updated.published);
    }

    #[test]
    fn test_publish_already_published_course_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);

        let instructor = Address::generate(&env);
        let course_id = 1u64;

        let result = env.as_contract(&contract_id, || {
            setup_course(&env, &instructor, course_id, true);
            publish_course(env.clone(), instructor.clone(), course_id)
        });

        assert_eq!(result, Err(CourseError::AlreadyPublished));
    }

    #[test]
    fn test_publish_missing_course_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);
        let caller = Address::generate(&env);

        let result = env.as_contract(&contract_id, || {
            publish_course(env.clone(), caller.clone(), 999u64)
        });
        assert_eq!(result, Err(CourseError::CourseNotFound));
    }

    #[test]
    fn test_publish_unauthorized_caller_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);

        let instructor = Address::generate(&env);
        let stranger = Address::generate(&env);
        let course_id = 1u64;

        let result = env.as_contract(&contract_id, || {
            setup_course(&env, &instructor, course_id, false);
            publish_course(env.clone(), stranger.clone(), course_id)
        });

        assert_eq!(result, Err(CourseError::Unauthorized));
    }

    #[test]
    fn test_successful_update() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);

        let instructor = Address::generate(&env);
        let course_id = 1u64;

        let updated_course = env.as_contract(&contract_id, || {
            setup_course(&env, &instructor, course_id, false);
            env.ledger().set_timestamp(1_000_000);

            let update_input = UpdateCourseInput {
                title: Some(String::from_str(&env, "New Title")),
                description: Some(String::from_str(&env, "New Description")),
                category: None,
                difficulty: Some(2),
                thumbnail: None,
                published: Some(true),
            };

            update_course(env.clone(), instructor.clone(), course_id, update_input).unwrap()
        });

        assert_eq!(updated_course.title, String::from_str(&env, "New Title"));
        assert_eq!(
            updated_course.description,
            String::from_str(&env, "New Description")
        );
        assert_eq!(
            updated_course.category,
            String::from_str(&env, "Old Category")
        );
        assert_eq!(updated_course.difficulty, 2);
        assert!(updated_course.published);
        assert_eq!(updated_course.updated_at, 1_000_000);
    }

    #[test]
    fn test_unauthorized_update_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);

        let instructor = Address::generate(&env);
        let unauthorized_user = Address::generate(&env);
        let course_id = 1u64;

        let result = env.as_contract(&contract_id, || {
            setup_course(&env, &instructor, course_id, false);

            let update_input = UpdateCourseInput {
                title: Some(String::from_str(&env, "Hacked Title")),
                description: None,
                category: None,
                difficulty: None,
                thumbnail: None,
                published: None,
            };

            update_course(env.clone(), unauthorized_user.clone(), course_id, update_input)
        });

        assert_eq!(result, Err(CourseError::Unauthorized));
    }

    #[test]
    fn test_update_non_existent_course_returns_error() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);

        let instructor = Address::generate(&env);
        let non_existent_course_id = 999u64;

        let update_input = UpdateCourseInput {
            title: Some(String::from_str(&env, "Title")),
            description: None,
            category: None,
            difficulty: None,
            thumbnail: None,
            published: None,
        };

        let result = env.as_contract(&contract_id, || {
            update_course(env.clone(), instructor, non_existent_course_id, update_input)
        });
        assert_eq!(result, Err(CourseError::CourseNotFound));
    }

    #[test]
    fn test_archive_course_and_verify_enrollment_restriction() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);

        let instructor = Address::generate(&env);
        let student = Address::generate(&env);
        let course_id = 100u64;

        let (archived_course, enroll_res) = env.as_contract(&contract_id, || {
            setup_course(&env, &instructor, course_id, true);

            let archive_res = archive_course(env.clone(), instructor.clone(), course_id);
            assert!(archive_res.is_ok());

            let archived_course = get_course(env.clone(), course_id).unwrap();
            let enroll_res = enroll_student(env.clone(), student.clone(), course_id);

            (archived_course, enroll_res)
        });

        assert!(archived_course.archived);
        assert_eq!(enroll_res, Err(CourseError::CourseIsArchived));
    }

    #[test]
    fn test_archive_already_archived_course_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);

        let instructor = Address::generate(&env);
        let course_id = 1u64;

        // Two separate frames: a second `require_auth()` for the same
        // address within one frame trips `Auth::ExistingValue` under
        // `mock_all_auths()`.
        env.as_contract(&contract_id, || {
            setup_course(&env, &instructor, course_id, true);
            archive_course(env.clone(), instructor.clone(), course_id).unwrap();
        });
        let result =
            env.as_contract(&contract_id, || archive_course(env.clone(), instructor, course_id));

        assert_eq!(result, Err(CourseError::AlreadyArchived));
    }

    #[test]
    fn test_enroll_student_success() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);

        let instructor = Address::generate(&env);
        let student = Address::generate(&env);
        let course_id = 1u64;

        let progress: u32 = env.as_contract(&contract_id, || {
            setup_course(&env, &instructor, course_id, true);
            let result = enroll_student(env.clone(), student.clone(), course_id);
            assert!(result.is_ok());

            env.storage()
                .persistent()
                .get(&StorageKey::Progress(student.clone(), course_id))
                .unwrap()
        });

        assert_eq!(progress, 0);
    }

    #[test]
    fn test_enroll_missing_course_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup(&env);
        let student = Address::generate(&env);

        let result =
            env.as_contract(&contract_id, || enroll_student(env.clone(), student, 9999u64));
        assert_eq!(result, Err(CourseError::CourseNotFound));
    }
}

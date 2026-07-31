#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Events, Ledger},
        Address, Env, String, Symbol, Vec,
    };

    // --- Helper Functions ---

    fn setup_test_course(env: &Env, instructor: &Address, course_id: u64) -> Course {
        let course = Course {
            id: course_id,
            instructor: instructor.clone(),
            admin: instructor.clone(),
            title: String::from_str(env, "Old Title"),
            description: String::from_str(env, "Old Description"),
            category: String::from_str(env, "Old Category"),
            difficulty: 1,
            thumbnail: String::from_str(env, "https://old.png"),
            published: false,
            archived: false,
            created_at: env.ledger().timestamp(),
            updated_at: env.ledger().timestamp(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::Course(course_id), &course);

        course
    }

    // --- Storage Key Audit Tests ---

    #[test]
    fn test_storage_keys_read_write() {
        let env = Env::default();
        let student_addr = Address::generate(&env);
        let cert_id = String::from_str(&env, "CERT-2026-001");

        let keys = vec![
            &env,
            StorageKey::Course(101),
            StorageKey::Lesson(202),
            StorageKey::Module(303),
            StorageKey::Quiz(404),
            StorageKey::Student(student_addr.clone()),
            StorageKey::Certificate(cert_id.clone()),
            StorageKey::Progress(student_addr.clone(), 101),
        ];

        for (i, key) in keys.iter().enumerate() {
            let dummy_val = (i + 1) as u64;

            env.storage().instance().set(key, &dummy_val);
            assert!(env.storage().instance().has(key));
            let retrieved: u64 = env.storage().instance().get(key).unwrap();
            assert_eq!(retrieved, dummy_val);
        }
    }

    // --- Lesson & Quiz Management Tests ---

    #[test]
    fn test_reorder_lessons_success() {
        let env = Env::default();
        env.mock_all_auths();

        let instructor = Address::generate(&env);
        let course_id = 1u64;

        let l1 = Lesson {
            id: 101,
            course_id,
            title: Symbol::new(&env, "Intro"),
            position: 1,
        };
        let l2 = Lesson {
            id: 102,
            course_id,
            title: Symbol::new(&env, "Advanced"),
            position: 2,
        };

        env.storage().persistent().set(&(course_id, 101u64), &l1);
        env.storage().persistent().set(&(course_id, 102u64), &l2);

        let mut ids = Vec::new(&env);
        ids.push_back(101);
        ids.push_back(102);

        let mut positions = Vec::new(&env);
        positions.push_back(2);
        positions.push_back(1);

        LessonManager::reorder_lessons(&env, instructor, course_id, ids, positions);

        let updated_l1: Lesson = env.storage().persistent().get(&(course_id, 101u64)).unwrap();
        let updated_l2: Lesson = env.storage().persistent().get(&(course_id, 102u64)).unwrap();

        assert_eq!(updated_l1.position, 2);
        assert_eq!(updated_l2.position, 1);
    }

    #[test]
    #[should_panic(expected = "Duplicate position detected")]
    fn test_reorder_lessons_prevents_duplicate_positions() {
        let env = Env::default();
        env.mock_all_auths();

        let instructor = Address::generate(&env);
        let course_id = 1u64;

        let l1 = Lesson {
            id: 101,
            course_id,
            title: Symbol::new(&env, "Intro"),
            position: 1,
        };
        let l2 = Lesson {
            id: 102,
            course_id,
            title: Symbol::new(&env, "Advanced"),
            position: 2,
        };

        env.storage().persistent().set(&(course_id, 101u64), &l1);
        env.storage().persistent().set(&(course_id, 102u64), &l2);

        let mut ids = Vec::new(&env);
        ids.push_back(101);
        ids.push_back(102);

        let mut positions = Vec::new(&env);
        positions.push_back(1);
        positions.push_back(1);

        LessonManager::reorder_lessons(&env, instructor, course_id, ids, positions);
    }

    #[test]
    fn create_quiz() {
        let quiz = Quiz {
            quiz_id: 1,
            lesson_id: 10,
            passing_score: 70,
            maximum_score: 100,
            reward_points: 50,
            is_active: true,
        };

        assert_eq!(quiz.quiz_id, 1);
        assert_eq!(quiz.lesson_id, 10);
        assert_eq!(quiz.passing_score, 70);
        assert_eq!(quiz.maximum_score, 100);
        assert_eq!(quiz.reward_points, 50);
        assert!(quiz.is_active);
    }

    // --- Course Lifecycle Tests ---

    #[test]
    fn test_publish_draft_course_success() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let course_id = 1u64;

        let course = setup_test_course(&env, &admin, course_id);
        let key = StorageKey::Course(course_id);
        env.storage().instance().set(&key, &course);

        env.mock_all_auths();
        let result = publish_course(env.clone(), admin.clone(), course_id);
        assert!(result.is_ok());

        let updated: Course = env.storage().instance().get(&key).unwrap();
        assert!(updated.published);
    }

    #[test]
    fn test_publish_invalid_course_fails() {
        let env = Env::default();
        let caller = Address::generate(&env);

        env.mock_all_auths();
        let result = publish_course(env, caller, 999u64);
        assert!(result.is_err());
    }

    #[test]
    fn test_successful_update() {
        let env = Env::default();
        env.mock_all_signatures();

        let instructor = Address::generate(&env);
        let course_id = 1u64;
        setup_test_course(&env, &instructor, course_id);

        env.ledger().set_timestamp(1_000_000);

        let update_input = UpdateCourseInput {
            title: Some(String::from_str(&env, "New Title")),
            description: Some(String::from_str(&env, "New Description")),
            category: None,
            difficulty: Some(2),
            thumbnail: None,
            published: Some(true),
        };

        let result = update_course(env.clone(), instructor.clone(), course_id, update_input);
        assert!(result.is_ok());

        let updated_course = result.unwrap();
        assert_eq!(updated_course.title, String::from_str(&env, "New Title"));
        assert_eq!(updated_course.description, String::from_str(&env, "New Description"));
        assert_eq!(updated_course.category, String::from_str(&env, "Old Category"));
        assert_eq!(updated_course.difficulty, 2);
        assert_eq!(updated_course.published, true);
        assert_eq!(updated_course.updated_at, 1_000_000);
    }

    #[test]
    fn test_unauthorized_update_rejected() {
        let env = Env::default();
        env.mock_all_signatures();

        let instructor = Address::generate(&env);
        let unauthorized_user = Address::generate(&env);
        let course_id = 1u64;

        setup_test_course(&env, &instructor, course_id);

        let update_input = UpdateCourseInput {
            title: Some(String::from_str(&env, "Hacked Title")),
            description: None,
            category: None,
            difficulty: None,
            thumbnail: None,
            published: None,
        };

        let result = update_course(env, unauthorized_user, course_id, update_input);
        assert_eq!(result, Err(CourseError::Unauthorized));
    }

    #[test]
    fn test_non_existent_course_returns_error() {
        let env = Env::default();
        env.mock_all_signatures();

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

        let result = update_course(env, instructor, non_existent_course_id, update_input);
        assert_eq!(result, Err(CourseError::CourseNotFound));
    }

    #[test]
    fn test_get_existing_course_success() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let course_id = 42u64;
        let now = env.ledger().timestamp();

        let course = setup_test_course(&env, &admin, course_id);
        let key = StorageKey::Course(course_id);
        env.storage().instance().set(&key, &course);

        let retrieved = get_course(env.clone(), course_id).expect("Course should be found");

        assert_eq!(retrieved.id, course_id);
        assert_eq!(retrieved.admin, admin);
    }

    #[test]
    fn test_get_missing_course_fails() {
        let env = Env::default();
        let missing_course_id = 9999u64;

        let result = get_course(env, missing_course_id);
        assert_eq!(result, Err(CourseError::CourseNotFound));
    }

    #[test]
    fn test_create_course_success() {
        let env = Env::default();
        env.mock_all_signatures();
        env.ledger().set_timestamp(500_000);

        let instructor = Address::generate(&env);

        let title = String::from_str(&env, "Introduction to Soroban");
        let description = String::from_str(&env, "Learn Rust and Stellar smart contracts.");
        let category = String::from_str(&env, "Blockchain");
        let thumbnail = String::from_str(&env, "https://example.com/thumb.png");

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

        assert_eq!(course_id, 1);

        let stored_course: Course = env
            .storage()
            .persistent()
            .get(&DataKey::Course(course_id))
            .expect("Course should exist in storage");

        assert_eq!(stored_course.id, 1);
        assert_eq!(stored_course.instructor, instructor);
        assert_eq!(stored_course.title, title);
        assert_eq!(stored_course.created_at, 500_000);

        let events = env.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_create_course_unique_ids() {
        let env = Env::default();
        env.mock_all_signatures();

        let instructor = Address::generate(&env);
        let title = String::from_str(&env, "Course Title");
        let desc = String::from_str(&env, "Course Description");
        let cat = String::from_str(&env, "Category");
        let thumb = String::from_str(&env, "https://example.com/thumb.png");

        let id_1 = create_course(env.clone(), instructor.clone(), title.clone(), desc.clone(), cat.clone(), 1, thumb.clone()).unwrap();
        let id_2 = create_course(env.clone(), instructor.clone(), title.clone(), desc.clone(), cat.clone(), 2, thumb.clone()).unwrap();

        assert_ne!(id_1, id_2);
        assert_eq!(id_1, 1);
        assert_eq!(id_2, 2);
    }

    #[test]
    fn test_create_course_invalid_input_rejected() {
        let env = Env::default();
        env.mock_all_signatures();

        let instructor = Address::generate(&env);
        let empty_title = String::from_str(&env, "");
        let desc = String::from_str(&env, "Valid Description");
        let cat = String::from_str(&env, "Valid Category");
        let thumb = String::from_str(&env, "https://example.com/thumb.png");

        let result = create_course(
            env.clone(),
            instructor,
            empty_title,
            desc,
            cat,
            1,
            thumb,
        );

        assert_eq!(result, Err(CourseError::InvalidInput));
    }

    #[test]
    fn test_archive_course_and_verify_enrollment_restriction() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let student = Address::generate(&env);
        let course_id = 100u64;

        let course = setup_test_course(&env, &admin, course_id);
        let key = StorageKey::Course(course_id);
        env.storage().instance().set(&key, &course);

        env.mock_all_auths();

        let archive_res = archive_course(env.clone(), admin.clone(), course_id);
        assert!(archive_res.is_ok());

        let archived_course: Course = env.storage().instance().get(&key).unwrap();
        assert!(archived_course.archived);

        let enroll_res = enroll_student(env.clone(), student.clone(), course_id);
        assert_eq!(enroll_res, Err(CourseError::CourseIsArchived));
    }

    // --- Access & Admin Control Tests ---

    #[test]
    fn test_initialize_and_get_admin() {
        let env = Env::default();
        env.mock_all_signatures();

        let admin = Address::generate(&env);
        assert!(initialize_admin(env.clone(), admin.clone()).is_ok());

        assert_eq!(get_admin(&env).unwrap(), admin);
        assert_eq!(get_role(&env, &admin), Role::Admin);

        assert_eq!(
            initialize_admin(env.clone(), admin),
            Err(AdminError::AlreadyInitialized)
        );
    }

    #[test]
    fn test_authorized_role_assignment() {
        let env = Env::default();
        env.mock_all_signatures();

        let admin = Address::generate(&env);
        let instructor = Address::generate(&env);
        initialize_admin(env.clone(), admin.clone()).unwrap();

        let result = set_role(env.clone(), admin, instructor.clone(), Role::Instructor);
        assert!(result.is_ok());
        assert_eq!(get_role(&env, &instructor), Role::Instructor);

        assert!(require_instructor_or_admin(&env, &instructor).is_ok());
    }

    #[test]
    fn test_unauthorized_action_rejected() {
        let env = Env::default();
        env.mock_all_signatures();

        let admin = Address::generate(&env);
        let student = Address::generate(&env);
        let target = Address::generate(&env);
        initialize_admin(env.clone(), admin).unwrap();

        assert_eq!(get_role(&env, &student), Role::Student);

        let result = set_role(env.clone(), student.clone(), target, Role::Instructor);
        assert_eq!(result, Err(AdminError::Unauthorized));

        assert_eq!(
            require_instructor_or_admin(&env, &student),
            Err(AdminError::Unauthorized)
        );
    }

    // --- LMS Event Emission Tests ---

    #[test]
    fn test_all_lms_events_published_successfully() {
        let env = Env::default();
        let student = Address::generate(&env);
        let instructor = Address::generate(&env);

        let course_id = 1u64;
        let lesson_id = 10u64;

        LMSEvents::emit_course_created(
            &env,
            course_id,
            instructor.clone(),
            String::from_str(&env, "Soroban 101"),
        );

        LMSEvents::emit_lesson_added(
            &env,
            course_id,
            lesson_id,
            String::from_str(&env, "Introduction"),
        );

        LMSEvents::emit_student_enrolled(&env, course_id, student.clone());
    }
}
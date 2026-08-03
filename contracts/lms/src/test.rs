#![cfg(test)]

use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, Address, Env, String, Vec};

use crate::{
    admin::{initialize_admin, set_role, Role},
    errors::LmsError,
    lesson::{LessonDataKey, LessonRecord},
    models::{Lesson, Module},
    storage::{save_student_profile, StudentProfile},
    LMSContract, LMSContractClient,
};

fn setup(env: &Env) -> (Address, Address, Address, LMSContractClient<'_>) {
    env.mock_all_auths();

    let contract_id = env.register_contract(None, LMSContract);
    let client = LMSContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let instructor = Address::generate(env);

    env.as_contract(&contract_id, || {
        initialize_admin(env.clone(), admin.clone()).unwrap();
        set_role(
            env.clone(),
            admin.clone(),
            instructor.clone(),
            Role::Instructor,
        )
        .unwrap();
    });

    (contract_id, admin, instructor, client)
}

#[test]
fn remove_existing_lesson() {
    let env = Env::default();
    let (contract_id, _admin, instructor, client) = setup(&env);

    let course_id = 1u64;
    let module_id = 10u64;
    let lesson_id = 100u64;

    env.ledger().set_timestamp(123);

    let mut lesson_ids = Vec::new(&env);
    lesson_ids.push_back(lesson_id);
    lesson_ids.push_back(101);

    let module = Module {
        module_id,
        course_id,
        title: String::from_str(&env, "Module 1"),
        lesson_ids,
        display_order: 1,
    };

    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&LessonDataKey::Module(module_id), &module);

        let mut modules = Vec::new(&env);
        modules.push_back(module_id);
        env.storage()
            .persistent()
            .set(&LessonDataKey::CourseModules(course_id), &modules);
    });

    let lesson = Lesson {
        lesson_id,
        course_id,
        title: String::from_str(&env, "Lesson 1"),
        description: String::from_str(&env, "Desc"),
        content_uri: String::from_str(&env, "ipfs://lesson"),
        estimated_duration: 30,
        lesson_order: 1,
    };
    let record = LessonRecord {
        lesson,
        removed: false,
        removed_at: 0,
    };
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&LessonDataKey::Lesson(lesson_id), &record);
    });

    let student = Address::generate(&env);
    let mut completed_lessons = Vec::new(&env);
    completed_lessons.push_back(String::from_str(&env, "100"));

    let profile = StudentProfile {
        wallet: student.clone(),
        enrolled_courses: Vec::new(&env),
        completed_lessons: completed_lessons.clone(),
        certificates: Vec::new(&env),
        xp: 0,
        reward_balance: 0,
    };
    env.as_contract(&contract_id, || {
        save_student_profile(&env, &profile);
    });

    let result = client.remove_lesson(&instructor, &lesson_id);
    assert!(result.is_ok());

    let updated_record: LessonRecord = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get(&LessonDataKey::Lesson(lesson_id))
            .unwrap()
    });
    assert!(updated_record.removed);
    assert_eq!(updated_record.removed_at, 123);

    let updated_module: Module = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get(&LessonDataKey::Module(module_id))
            .unwrap()
    });
    assert_eq!(updated_module.lesson_ids.len(), 1);
    assert_eq!(updated_module.lesson_ids.get(0), Some(101));

    let stored = env
        .as_contract(&contract_id, || {
            crate::storage::get_student_profile(&env, &student)
        })
        .unwrap();
    assert_eq!(stored.completed_lessons, completed_lessons);

    assert_eq!(env.events().all().len(), 1);
}

#[test]
fn remove_non_existent_lesson_rejected() {
    let env = Env::default();
    let (_contract_id, _admin, instructor, client) = setup(&env);

    let result = client.remove_lesson(&instructor, &999);
    assert_eq!(result, Err(LmsError::LessonNotFound));
}

#[test]
fn verify_course_consistency_after_removal() {
    let env = Env::default();
    let (contract_id, _admin, instructor, client) = setup(&env);

    let course_id = 1u64;
    let module_id = 10u64;
    let lesson_id = 100u64;

    let mut lesson_ids = Vec::new(&env);
    lesson_ids.push_back(lesson_id);
    lesson_ids.push_back(101);
    lesson_ids.push_back(102);

    let module = Module {
        module_id,
        course_id,
        title: String::from_str(&env, "Module 1"),
        lesson_ids,
        display_order: 1,
    };
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&LessonDataKey::Module(module_id), &module);

        let mut modules = Vec::new(&env);
        modules.push_back(module_id);
        env.storage()
            .persistent()
            .set(&LessonDataKey::CourseModules(course_id), &modules);
    });

    let lesson = Lesson {
        lesson_id,
        course_id,
        title: String::from_str(&env, "Lesson 1"),
        description: String::from_str(&env, "Desc"),
        content_uri: String::from_str(&env, "ipfs://lesson"),
        estimated_duration: 30,
        lesson_order: 1,
    };
    let record = LessonRecord {
        lesson,
        removed: false,
        removed_at: 0,
    };
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&LessonDataKey::Lesson(lesson_id), &record);
    });

    client.remove_lesson(&instructor, &lesson_id).unwrap();

    let updated_module: Module = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get(&LessonDataKey::Module(module_id))
            .unwrap()
    });

    for i in 0..updated_module.lesson_ids.len() {
        assert_ne!(updated_module.lesson_ids.get(i).unwrap(), lesson_id);
    }
}

#[test]
fn test_update_lesson_success() {
    let env = Env::default();
    let (contract_id, _admin, instructor, client) = setup(&env);

    let course_id = 1u64;
    let lesson_id = 42u64;

    let lesson = Lesson {
        lesson_id,
        course_id,
        title: String::from_str(&env, "Original Title"),
        description: String::from_str(&env, "Original Desc"),
        content_uri: String::from_str(&env, "ipfs://original"),
        estimated_duration: 20,
        lesson_order: 1,
    };
    let record = LessonRecord {
        lesson,
        removed: false,
        removed_at: 0,
    };
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&LessonDataKey::Lesson(lesson_id), &record);
    });

    let updated = client
        .update_lesson(
            &instructor,
            &lesson_id,
            &String::from_str(&env, "Updated Title"),
            &String::from_str(&env, "ipfs://new"),
            &45u32,
            &String::from_str(&env, "Updated Desc"),
        )
        .unwrap();

    assert_eq!(updated.title, String::from_str(&env, "Updated Title"));
    assert_eq!(updated.content_uri, String::from_str(&env, "ipfs://new"));
    assert_eq!(updated.estimated_duration, 45);
    assert_eq!(updated.description, String::from_str(&env, "Updated Desc"));
    assert_eq!(updated.lesson_order, 1); // unchanged
}

#[test]
fn test_update_lesson_unauthorized() {
    let env = Env::default();
    let (contract_id, _admin, _instructor, client) = setup(&env);

    let lesson_id = 42u64;
    let lesson = Lesson {
        lesson_id,
        course_id: 1,
        title: String::from_str(&env, "Title"),
        description: String::from_str(&env, "Desc"),
        content_uri: String::from_str(&env, "ipfs://x"),
        estimated_duration: 10,
        lesson_order: 1,
    };
    let record = LessonRecord { lesson, removed: false, removed_at: 0 };
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&LessonDataKey::Lesson(lesson_id), &record);
    });

    let stranger = Address::generate(&env);
    let result = client.update_lesson(
        &stranger,
        &lesson_id,
        &String::from_str(&env, "Hacked"),
        &String::from_str(&env, "ipfs://hack"),
        &1u32,
        &String::from_str(&env, "bad"),
    );

    assert_eq!(result, Err(LmsError::Unauthorized));
}

#[test]
fn test_update_lesson_not_found() {
    let env = Env::default();
    let (_contract_id, _admin, instructor, client) = setup(&env);

    let result = client.update_lesson(
        &instructor,
        &999u64,
        &String::from_str(&env, "Title"),
        &String::from_str(&env, "ipfs://x"),
        &10u32,
        &String::from_str(&env, "desc"),
    );

    assert_eq!(result, Err(LmsError::LessonNotFound));
}

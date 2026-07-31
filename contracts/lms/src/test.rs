#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Events, Ledger},
        Address, Env, String, Symbol, Vec,
    };

    use crate::{
        admin::{initialize_admin, set_role, get_admin, get_role, require_instructor_or_admin, Role, AdminError},
        errors::LmsError,
        lesson::{LessonDataKey, LessonRecord, LessonManager},
        models::{Lesson, Module, Quiz, Course, UpdateCourseInput, CourseError},
        storage::{save_student_profile, get_student_profile, StudentProfile, DataKey, StorageKey},
        event::LMSEvents,
        LMSContract, LMSContractClient,
        course::{create_course, get_course, update_course, publish_course, archive_course, enroll_student},
    };

    // --- Helper Functions ---

    fn setup(env: &Env) -> (Address, Address, Address, LMSContractClient<'_>) {
        env.mock_all_auths();

        let contract_id = env.register_contract(None, LMSContract);
        let client = LMSContractClient::new(env, &contract_id);

        let admin = Address::generate(env);
        let instructor = Address::generate(env);

        env.as_contract(&contract_id, || {
            initialize_admin(env.clone(), admin.clone()).unwrap();
            set_role(env.clone(), admin.clone(), instructor.clone(), Role::Instructor).unwrap();
        });

        (contract_id, admin, instructor, client)
    }

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
            created
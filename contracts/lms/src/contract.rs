use soroban_sdk::{contract, contractimpl, Address, Env, String};

use crate::admin::{self, AdminError, Role};
use crate::course::{self, Course, CourseError, UpdateCourseInput};
use crate::enrollment::{self, EnrollmentStatus};
use crate::errors::LmsError;
use crate::lesson;
use crate::progress; // bring in lesson removal

#[contract]
pub struct LMSContract;

#[contractimpl]
impl LMSContract {
    /// One-time contract initialization: sets `admin` as the contract Admin.
    pub fn initialize(env: Env, admin: Address) -> Result<(), AdminError> {
        admin::initialize_admin(env, admin)
    }

    pub fn get_admin(env: Env) -> Result<Address, AdminError> {
        admin::get_admin(&env)
    }

    pub fn get_role(env: Env, user: Address) -> Role {
        admin::get_role(&env, &user)
    }

    pub fn set_role(
        env: Env,
        caller: Address,
        target: Address,
        role: Role,
    ) -> Result<(), AdminError> {
        admin::set_role(env, caller, target, role)
    }

    pub fn create_course(
        env: Env,
        instructor: Address,
        title: String,
        description: String,
        category: String,
        difficulty: u32,
        thumbnail: String,
    ) -> Result<u64, CourseError> {
        course::create_course(
            env,
            instructor,
            title,
            description,
            category,
            difficulty,
            thumbnail,
        )
    }

    pub fn get_course(env: Env, course_id: u64) -> Result<Course, CourseError> {
        course::get_course(env, course_id)
    }

    pub fn update_course(
        env: Env,
        caller: Address,
        course_id: u64,
        input: UpdateCourseInput,
    ) -> Result<Course, CourseError> {
        course::update_course(env, caller, course_id, input)
    }

    pub fn publish_course(env: Env, caller: Address, course_id: u64) -> Result<(), CourseError> {
        course::publish_course(env, caller, course_id)
    }

    pub fn archive_course(env: Env, caller: Address, course_id: u64) -> Result<(), CourseError> {
        course::archive_course(env, caller, course_id)
    }

    pub fn enroll_student(env: Env, student: Address, course_id: u64) -> Result<(), CourseError> {
        course::enroll_student(env, student, course_id)
    }

    pub fn withdraw_student(env: Env, student: Address, course_id: u64) -> Result<(), LmsError> {
        enrollment::withdraw_student(env, student, course_id)
    }

    pub fn get_enrollment_status(env: Env, student: Address, course_id: u64) -> EnrollmentStatus {
        enrollment::get_enrollment_status(env, student, course_id)
    }

    /// Registers a new lesson under `course_id`. Only the course's owning
    /// instructor may register lessons for it.
    pub fn register_lesson(
        env: Env,
        caller: Address,
        course_id: u64,
        lesson_id: u64,
        title: String,
    ) -> Result<(), LmsError> {
        progress::register_lesson(env, caller, course_id, lesson_id, title)
    }

    /// Removes a lesson from a course. Only the course's owning instructor may remove lessons.
    pub fn remove_lesson(
        env: Env,
        caller: Address,
        _course_id: u64,
        lesson_id: u64,
    ) -> Result<(), LmsError> {
        lesson::remove_lesson(env, caller, lesson_id)
    }

    /// Updates lesson metadata (title, content URI, duration, description).
    /// Only the course's owning instructor or admin may update lessons.
    pub fn update_lesson(
        env: Env,
        caller: Address,
        lesson_id: u64,
        title: String,
        content_uri: String,
        estimated_duration: u32,
        description: String,
    ) -> Result<crate::models::Lesson, LmsError> {
        lesson::update_lesson(
            &env,
            &caller,
            lesson_id,
            title,
            content_uri,
            estimated_duration,
            description,
        )
    }

    /// Marks `lesson_id` as completed by `student`. Rejects non-enrolled
    /// students, unknown/mismatched lessons, and duplicate completions.
    pub fn complete_lesson(
        env: Env,
        student: Address,
        course_id: u64,
        lesson_id: u64,
    ) -> Result<(), LmsError> {
        progress::complete_lesson(env, student, course_id, lesson_id)
    }
}

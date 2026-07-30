use soroban_sdk::{contracterror, contracttype, symbol_short, Address, Env, Symbol};
use crate::storage::StorageKey;
use soroban_sdk::{contracterror, contracttype, symbol_short, Address, Env, String, Symbol};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum CourseError {
    CourseNotFound = 1,
    AlreadyPublished = 2,
    NotAuthorized = 3,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Course {
    pub id: u64,
    pub admin: Address,
    pub title: soroban_sdk::String,
    pub published: bool,
}

pub fn publish_course(env: Env, caller: Address, course_id: u64) -> Result<(), CourseError> {
    // Require signature from caller
    caller.require_auth();

    let key = StorageKey::Course(course_id);

    // 1. Prevent publishing non-existent courses
    let mut course: Course = env
        .storage()
        .instance()
        .get(&key)
        .ok_or(CourseError::CourseNotFound)?;

    // 2. Ensure only authorized course admin can publish
    if course.admin != caller {
        return Err(CourseError::NotAuthorized);
    }

    // Optional: Return error if already published
    if course.published {
        return Err(CourseError::AlreadyPublished);
    }


    #[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum CourseError {
    CourseNotFound = 1,
    AlreadyPublished = 2,
    NotAuthorized = 3,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Course {
    pub id: u64,
    pub admin: Address,
    pub title: String,
    pub description: String,
    Unauthorized = 2,
    InvalidInput = 3,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Course {
    pub id: u64,
    pub instructor: Address,
    pub title: String,
    pub description: String,
    pub category: String,
    pub difficulty: u32,
    pub thumbnail: String,
    pub published: bool,
    pub created_at: u64,
    pub updated_at: u64,
}


/// Retrieves course information by ID.
///
/// Returns `Course` struct containing metadata, author, publication status, and timestamps.
/// Returns `CourseError::CourseNotFound` if the course ID does not exist in storage.
pub fn get_course(env: Env, course_id: u64) -> Result<Course, CourseError> {
    let key = StorageKey::Course(course_id);

    env.storage()
        .instance()
        .get(&key)
        .ok_or(CourseError::CourseNotFound)
}



use soroban_sdk::{contracterror, contracttype, symbol_short, Address, Env, String};
use crate::storage::StorageKey;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum CourseError {
    CourseNotFound = 1,
    AlreadyPublished = 2,
    NotAuthorized = 3,
    AlreadyArchived = 4,
    CourseIsArchived = 5,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Course {
    pub id: u64,
    pub admin: Address,
    pub title: String,
    pub description: String,
    pub published: bool,
    pub archived: bool,
    pub created_at: u64,
    pub updated_at: u64,
}




/// Marks a course as archived.
///
/// Historical records and learner progress remain in storage, but future enrollments are blocked.
pub fn archive_course(env: Env, caller: Address, course_id: u64) -> Result<(), CourseError> {
    caller.require_auth();

    let key = StorageKey::Course(course_id);

    // 1. Fetch course or fail if non-existent
    let mut course: Course = env
        .storage()
        .instance()
        .get(&key)
        .ok_or(CourseError::CourseNotFound)?;

    // 2. Authorization check
    if course.admin != caller {
        return Err(CourseError::NotAuthorized);
    }

    if course.archived {
        return Err(CourseError::AlreadyArchived);
    }

    // 3. Mark as archived (preserving all metadata and historic student records)
    course.archived = true;
    course.updated_at = env.ledger().timestamp();
    env.storage().instance().set(&key, &course);

    // 4. Emit CourseArchived event
    env.events().publish(
        (symbol_short!("archived"), course_id),
        caller,
    );

    Ok(())
}

/// Example student enrollment logic demonstrating enrollment restriction on archived courses.
pub fn enroll_student(env: Env, student: Address, course_id: u64) -> Result<(), CourseError> {
    student.require_auth();

    let key = StorageKey::Course(course_id);
    let course: Course = env
        .storage()
        .instance()
        .get(&key)
        .ok_or(CourseError::CourseNotFound)?;

    // Prevent new enrollments if the course is archived
    if course.archived {
        return Err(CourseError::CourseIsArchived);
    }

    // Process enrollment and initialize student progress record key ...
    let progress_key = StorageKey::Progress(student.clone(), course_id);
    env.storage().instance().set(&progress_key, &0u32); // 0% initial progress

    Ok(())
}
=======
#[contracttype]
pub enum DataKey {
    Course(u64),
    NextCourseId,
}

/// Helper function to validate non-empty Soroban strings
fn validate_string(s: &String) -> bool {
    s.len() > 0
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
    // 1. Authorization check
    instructor.require_auth();

    // 2. Field validations
    if !validate_string(&title)
        || !validate_string(&description)
        || !validate_string(&category)
        || !validate_string(&thumbnail)
    {
        return Err(CourseError::InvalidInput);
    }

    // 3. ID Generation (Auto-increment counter)
    let id_key = DataKey::NextCourseId;
    let course_id: u64 = env.storage().persistent().get(&id_key).unwrap_or(1);
    env.storage().persistent().set(&id_key, &(course_id + 1));

    // 4. Construct Course metadata
    let now = env.ledger().timestamp();
    let course = Course {
        id: course_id,
        instructor: instructor.clone(),
        title: title.clone(),
        description,
        category,
        difficulty,
        thumbnail,
        published: true,
        created_at: now,
        updated_at: now,
    };

    // 5. Store Course metadata
    let course_key = DataKey::Course(course_id);
    env.storage().persistent().set(&course_key, &course);

    // 6. Emit Event: topics -> (Symbol("course"), Symbol("created"), course_id), data -> (instructor, title)
    env.events().publish(
        (symbol_short!("course"), symbol_short!("created"), course_id),
        (instructor, title),
    );

    Ok(course_id)
}

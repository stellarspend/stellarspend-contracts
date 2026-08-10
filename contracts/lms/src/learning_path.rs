use soroban_sdk::{contracttype, Address, Env, String, Vec};

use crate::errors::Error;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Difficulty {
    Beginner,
    Intermediate,
    Advanced,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LearningPath {
    pub id: u64,
    pub title: String,
    pub description: String,
    pub courses: Vec<u64>,
    pub difficulty: Difficulty,
    pub estimated_completion_time: u64,
    pub instructor: Address,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    LearningPath(u64),
    LearningPathCount,
}

pub fn create_learning_path(
    env: &Env,
    instructor: Address,
    title: String,
    description: String,
    courses: Vec<u64>,
    difficulty: Difficulty,
    estimated_completion_time: u64,
) -> Result<u64, Error> {
    instructor.require_auth();

    if title.len() == 0 {
        return Err(Error::InvalidLearningPathTitle);
    }

    if courses.len() == 0 {
        return Err(Error::LearningPathRequiresCourses);
    }

    if estimated_completion_time == 0 {
        return Err(Error::InvalidCompletionTime);
    }

    let id = get_next_path_id(env);

    let path = LearningPath {
        id,
        title,
        description,
        courses,
        difficulty,
        estimated_completion_time,
        instructor,
    };

    env.storage()
        .instance()
        .set(&DataKey::LearningPath(id), &path);

    env.storage()
        .instance()
        .set(&DataKey::LearningPathCount, &(id + 1));

    Ok(id)
}

pub fn get_learning_path(
    env: &Env,
    path_id: u64,
) -> Result<LearningPath, Error> {
    env.storage()
        .instance()
        .get(&DataKey::LearningPath(path_id))
        .ok_or(Error::LearningPathNotFound)
}

pub fn get_next_path_id(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::LearningPathCount)
        .unwrap_or(1)
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LearningPathProgress {
    pub path_id: u64,
    pub courses_completed: u32,
    pub courses_remaining: u32,
    pub completion_percentage: u32,
}
pub fn get_learning_path_progress(
    env: &Env,
    learner: Address,
    path_id: u64,
) -> Result<LearningPathProgress, Error> {
    let path = get_learning_path(env, path_id)?;

    let total_courses = path.courses.len();

    if total_courses == 0 {
        return Ok(LearningPathProgress {
            path_id,
            courses_completed: 0,
            courses_remaining: 0,
            completion_percentage: 0,
        });
    }

    let mut courses_completed: u32 = 0;

    for course_id in path.courses.iter() {
        if has_completed_course(env, &learner, course_id) {
            courses_completed += 1;
        }
    }

    let total_courses = total_courses as u32;
    let courses_remaining = total_courses - courses_completed;

    let completion_percentage =
        (courses_completed * 100) / total_courses;

    Ok(LearningPathProgress {
        path_id,
        courses_completed,
        courses_remaining,
        completion_percentage,
    })
}
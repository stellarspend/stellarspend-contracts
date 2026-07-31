use soroban_sdk::{contracttype, Address, Env, Vec};

use crate::errors::Error;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrerequisiteCourses {
    pub course_id: u64,
    pub prerequisites: Vec<u64>,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Prerequisites(u64),
}

pub fn add_prerequisite(
    env: &Env,
    instructor: Address,
    course_id: u64,
    prerequisite_course_id: u64,
) -> Result<(), Error> {
    instructor.require_auth();

    if course_id == prerequisite_course_id {
        return Err(Error::InvalidPrerequisite);
    }

    let mut prerequisites = get_prerequisites(env, course_id);

    if prerequisites.contains(&prerequisite_course_id) {
        return Err(Error::PrerequisiteAlreadyExists);
    }

    prerequisites.push_back(prerequisite_course_id);

    env.storage()
        .instance()
        .set(&DataKey::Prerequisites(course_id), &prerequisites);

    Ok(())
}

pub fn remove_prerequisite(
    env: &Env,
    instructor: Address,
    course_id: u64,
    prerequisite_course_id: u64,
) -> Result<(), Error> {
    instructor.require_auth();

    let prerequisites = get_prerequisites(env, course_id);
    let mut updated = Vec::new(env);

    let mut found = false;

    for id in prerequisites.iter() {
        if id == prerequisite_course_id {
            found = true;
        } else {
            updated.push_back(id);
        }
    }

    if !found {
        return Err(Error::PrerequisiteNotFound);
    }

    env.storage()
        .instance()
        .set(&DataKey::Prerequisites(course_id), &updated);

    Ok(())
}

pub fn get_prerequisites(env: &Env, course_id: u64) -> Vec<u64> {
    env.storage()
        .instance()
        .get(&DataKey::Prerequisites(course_id))
        .unwrap_or_else(|| Vec::new(env))
}

/// Verifies that a learner has completed every prerequisite.
pub fn validate_prerequisites(
    env: &Env,
    learner: &Address,
    course_id: u64,
) -> Result<(), Error> {
    let prerequisites = get_prerequisites(env, course_id);

    for prerequisite_id in prerequisites.iter() {
        if !has_completed_course(env, learner, prerequisite_id) {
            return Err(Error::PrerequisitesNotCompleted);
        }
    }

    Ok(())
}

/// Replace this with the LMS's existing completion/progress lookup.
fn has_completed_course(
    env: &Env,
    learner: &Address,
    course_id: u64,
) -> bool {
    // Example storage key.
    let key = CompletionKey {
        learner: learner.clone(),
        course_id,
    };

    env.storage()
        .instance()
        .get::<CompletionKey, bool>(&key)
        .unwrap_or(false)
}

#[contracttype]
#[derive(Clone)]
struct CompletionKey {
    learner: Address,
    course_id: u64,
}
use soroban_sdk::{contracttype, Address, Env, Vec};

use crate::{
    admin,
    errors::LmsError,
    event::LMSEvents,
    models::{Lesson, Module},
};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LessonDataKey {
    Lesson(u64),
    Module(u64),
    CourseModules(u64),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LessonRecord {
    pub lesson: Lesson,
    pub removed: bool,
    pub removed_at: u64,
}

pub fn remove_lesson(env: Env, caller: Address, lesson_id: u64) -> Result<(), LmsError> {
    caller.require_auth();
    admin::require_instructor_or_admin(&env, &caller).map_err(|_| LmsError::Unauthorized)?;

    let mut record: LessonRecord = env
        .storage()
        .persistent()
        .get(&LessonDataKey::Lesson(lesson_id))
        .ok_or(LmsError::LessonNotFound)?;

    if record.removed {
        return Ok(());
    }

    let course_id = record.lesson.course_id;

    let module_ids: Vec<u64> = env
        .storage()
        .persistent()
        .get(&LessonDataKey::CourseModules(course_id))
        .ok_or(LmsError::CourseNotFound)?;

    for i in 0..module_ids.len() {
        let module_id = module_ids.get(i).ok_or(LmsError::ModuleNotFound)?;
        let mut module: Module = env
            .storage()
            .persistent()
            .get(&LessonDataKey::Module(module_id))
            .ok_or(LmsError::ModuleNotFound)?;

        let mut new_ids = Vec::new(&env);
        for j in 0..module.lesson_ids.len() {
            let id = module
                .lesson_ids
                .get(j)
                .ok_or(LmsError::ModuleNotFound)?;
            if id != lesson_id {
                new_ids.push_back(id);
            }
        }
        module.lesson_ids = new_ids;

        env.storage()
            .persistent()
            .set(&LessonDataKey::Module(module_id), &module);
    }

    record.removed = true;
    record.removed_at = env.ledger().timestamp();
    env.storage()
        .persistent()
        .set(&LessonDataKey::Lesson(lesson_id), &record);

    LMSEvents::emit_lesson_removed(&env, course_id, lesson_id);

    Ok(())
}

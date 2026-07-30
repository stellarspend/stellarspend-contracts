use soroban_sdk::{contracttype, Address, Env, String};

use crate::errors::LmsError;
use crate::models::Lesson;

#[contracttype]
#[derive(Clone)]
pub enum LessonKey {
    Lesson(u64),
    Admin,
}

pub fn save_lesson(env: &Env, lesson: &Lesson) {
    env.storage()
        .persistent()
        .set(&LessonKey::Lesson(lesson.lesson_id), lesson);
}

pub fn get_lesson(env: &Env, lesson_id: u64) -> Option<Lesson> {
    env.storage()
        .persistent()
        .get(&LessonKey::Lesson(lesson_id))
}

pub fn update_lesson(
    env: &Env,
    caller: &Address,
    lesson_id: u64,
    title: String,
    content_uri: String,
    estimated_duration: u32,
    description: String,
) -> Result<Lesson, LmsError> {
    let admin: Address = env
        .storage()
        .instance()
        .get(&LessonKey::Admin)
        .ok_or(LmsError::Unauthorized)?;

    if caller != &admin {
        return Err(LmsError::Unauthorized);
    }

    let mut lesson = get_lesson(env, lesson_id).ok_or(LmsError::LessonNotFound)?;

    lesson.title = title;
    lesson.content_uri = content_uri;
    lesson.estimated_duration = estimated_duration;
    lesson.description = description;

    save_lesson(env, &lesson);

    Ok(lesson)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env, String};

    fn setup(env: &Env) -> (Address, Lesson) {
        let admin = Address::generate(env);
        env.storage().instance().set(&LessonKey::Admin, &admin);

        let lesson = Lesson {
            lesson_id: 1,
            course_id: 10,
            title: String::from_str(env, "Intro"),
            description: String::from_str(env, "Original description"),
            content_uri: String::from_str(env, "ipfs://original"),
            estimated_duration: 20,
            lesson_order: 1,
        };

        save_lesson(env, &lesson);
        (admin, lesson)
    }

    #[test]
    fn test_update_lesson_success() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, _) = setup(&env);

        let updated = update_lesson(
            &env,
            &admin,
            1,
            String::from_str(&env, "Updated Title"),
            String::from_str(&env, "ipfs://new"),
            45,
            String::from_str(&env, "Updated description"),
        )
        .unwrap();

        assert_eq!(updated.title, String::from_str(&env, "Updated Title"));
        assert_eq!(updated.content_uri, String::from_str(&env, "ipfs://new"));
        assert_eq!(updated.estimated_duration, 45);
        assert_eq!(updated.description, String::from_str(&env, "Updated description"));
        assert_eq!(updated.lesson_order, 1);

        let stored = get_lesson(&env, 1).unwrap();
        assert_eq!(stored, updated);
    }

    #[test]
    fn test_update_lesson_unauthorized() {
        let env = Env::default();
        env.mock_all_auths();
        let _ = setup(&env);

        let stranger = Address::generate(&env);
        let result = update_lesson(
            &env,
            &stranger,
            1,
            String::from_str(&env, "Hacked"),
            String::from_str(&env, "ipfs://hack"),
            1,
            String::from_str(&env, "bad"),
        );

        assert_eq!(result, Err(LmsError::Unauthorized));
    }

    #[test]
    fn test_update_lesson_not_found() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, _) = setup(&env);

        let result = update_lesson(
            &env,
            &admin,
            999,
            String::from_str(&env, "Title"),
            String::from_str(&env, "ipfs://x"),
            10,
            String::from_str(&env, "desc"),
        );

        assert_eq!(result, Err(LmsError::LessonNotFound));
    }
}

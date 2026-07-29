#![cfg(test)]

use crate::lesson::{Lesson, LessonManager};
use soroban_sdk::{testutils::Address as _, Address, Env, Symbol, Vec};

#[test]
fn test_reorder_lessons_success() {
    let env = Env::default();
    env.mock_all_auths();

    let instructor = Address::generate(&env);
    let course_id = 1u64;

    let l1 = Lesson { id: 101, course_id, title: Symbol::new(&env, "Intro"), position: 1 };
    let l2 = Lesson { id: 102, course_id, title: Symbol::new(&env, "Advanced"), position: 2 };

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

    let l1 = Lesson { id: 101, course_id, title: Symbol::new(&env, "Intro"), position: 1 };
    let l2 = Lesson { id: 102, course_id, title: Symbol::new(&env, "Advanced"), position: 2 };

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

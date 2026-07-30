#![cfg(test)]

use soroban_sdk::{Env, String, Vec};

use crate::{LMSContract, LMSContractClient, Lesson, Module, Quiz};

#[test]
fn test_initialize() {
    let env = Env::default();
    let contract_id = env.register(LMSContract, ());
    let client = LMSContractClient::new(&env, &contract_id);
    let result = client.initialize();
    assert!(result);
}

#[test]
fn test_create_module() {
    let env = Env::default();

    let mut lesson_ids: Vec<u64> = Vec::new(&env);
    lesson_ids.push_back(1);
    lesson_ids.push_back(2);
    lesson_ids.push_back(3);

    let module = Module {
        module_id: 1,
        course_id: 100,
        title: String::from_str(&env, "Introduction"),
        lesson_ids,
        display_order: 1,
    };

    assert_eq!(module.module_id, 1);
    assert_eq!(module.course_id, 100);
    assert_eq!(module.lesson_ids.len(), 3);
    assert_eq!(module.display_order, 1);
    assert_eq!(module.lesson_ids.get(0), Some(1));
    assert_eq!(module.lesson_ids.get(1), Some(2));
    assert_eq!(module.lesson_ids.get(2), Some(3));
}

#[test]
fn test_create_lesson() {
    let env = Env::default();

    let lesson = Lesson {
        lesson_id: 1,
        course_id: 100,
        title: String::from_str(&env, "Introduction"),
        description: String::from_str(&env, "Welcome to the course"),
        content_uri: String::from_str(&env, "ipfs://QmLessonHash"),
        estimated_duration: 30,
        lesson_order: 1,
    };

    assert_eq!(lesson.lesson_id, 1);
    assert_eq!(lesson.course_id, 100);
    assert_eq!(lesson.title, String::from_str(&env, "Introduction"));
    assert_eq!(lesson.estimated_duration, 30);
    assert_eq!(lesson.lesson_order, 1);
}

#[test]
fn test_create_quiz() {
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

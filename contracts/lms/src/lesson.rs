#![no_std]
use soroban_sdk::{contracttype, Address, Env, Symbol, Vec};

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Lesson {
    pub id: u64,
    pub course_id: u64,
    pub title: Symbol,
    pub position: u32,
}

pub struct LessonManager;

impl LessonManager {
    /// Reorders lessons within a course ensuring positions are unique and sequential.
    pub fn reorder_lessons(
        env: &Env,
        instructor: Address,
        course_id: u64,
        lesson_ids: Vec<u64>,
        new_positions: Vec<u32>,
    ) {
        instructor.require_auth();

        assert_eq!(
            lesson_ids.len(),
            new_positions.len(),
            "Lesson IDs and positions length mismatch"
        );
        assert!(!lesson_ids.is_empty(), "Lesson list cannot be empty");

        // Prevent duplicate positions in inputs
        for i in 0..new_positions.len() {
            for j in (i + 1)..new_positions.len() {
                assert_ne!(
                    new_positions.get(i).unwrap(),
                    new_positions.get(j).unwrap(),
                    "Duplicate position detected"
                );
            }
        }

        // Update each lesson with its new position
        for i in 0..lesson_ids.len() {
            let id = lesson_ids.get(i).unwrap();
            let new_pos = new_positions.get(i).unwrap();

            let mut lesson: Lesson = env
                .storage()
                .persistent()
                .get(&(course_id, id))
                .expect("Lesson not found in course");

            lesson.position = new_pos;
            env.storage().persistent().set(&(course_id, id), &lesson);
        }
    }
}

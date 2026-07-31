#![allow(dead_code)]

use soroban_sdk::{contracttype, Address, String};

/// Storage keys used throughout the LMS contract.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Auto-increment counter for course IDs.
    NextCourseId,
    /// Storage key for a specific course by ID
    Course(u64),
    /// Storage key for a specific lesson by ID
    Lesson(u64),
    /// Storage key for a specific module by ID
    Module(u64),
    /// Storage key for a specific quiz by ID
    Quiz(u64),
    /// Storage key for student account record by Address
    Student(Address),
    /// Storage key for a certificate record by ID or serial string
    Certificate(String),
    /// Storage key tracking student progress for a given course (Student Address, Course ID)
    Progress(Address, u64),
    /// Storage key tracking a student's withdrawal timestamp for a given course
    /// (Student Address, Course ID). Presence of this key means the student
    /// actively withdrew; the underlying `Progress` record is left untouched
    /// so the progress percentage at time of withdrawal remains queryable.
    Withdrawn(Address, u64),
    /// Storage key tracking whether/when a student completed a specific lesson,
    /// storing a `u64` ledger timestamp. Presence of the key means completed;
    /// absence means not completed. Keyed by (Student Address, Lesson ID).
    LessonCompletion(Address, u64),
    /// Storage key tracking the total number of lessons registered for a
    /// course (a `u32` counter), used as the denominator for course progress
    /// percentage calculations. Keyed by Course ID.
    CourseLessonCount(u64),
    /// Storage key tracking how many lessons a student has completed within a
    /// given course (a `u32` counter, incremented by `complete_lesson`), used
    /// as the numerator for course progress percentage calculations. Keyed by
    /// (Student Address, Course ID).
    CompletedLessonCount(Address, u64),
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, vec, Env};

    #[test]
    fn test_storage_keys_read_write() {
        let env = Env::default();
        let contract_id = env.register(crate::LMSContract, ());
        let student_addr = Address::generate(&env);
        let cert_id = String::from_str(&env, "CERT-2026-001");

        // One instance of every StorageKey variant.
        let keys = vec![
            &env,
            StorageKey::NextCourseId,
            StorageKey::Course(101),
            StorageKey::Lesson(202),
            StorageKey::Module(303),
            StorageKey::Quiz(404),
            StorageKey::Student(student_addr.clone()),
            StorageKey::Certificate(cert_id.clone()),
            StorageKey::Progress(student_addr.clone(), 101),
            StorageKey::Withdrawn(student_addr.clone(), 101),
            StorageKey::LessonCompletion(student_addr.clone(), 202),
            StorageKey::CourseLessonCount(101),
            StorageKey::CompletedLessonCount(student_addr.clone(), 101),
        ];

        env.as_contract(&contract_id, || {
            // Verify write, exists, and read back for each key
            for (i, key) in keys.iter().enumerate() {
                let dummy_val = (i + 1) as u64;

                env.storage().instance().set(&key, &dummy_val);

                assert!(env.storage().instance().has(&key));
                let retrieved: u64 = env.storage().instance().get(&key).unwrap();
                assert_eq!(retrieved, dummy_val);
            }
        });
    }
}

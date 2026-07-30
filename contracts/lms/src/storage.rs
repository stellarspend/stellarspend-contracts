
use soroban_sdk::{contracttype, Address, String};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
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
#![allow(dead_code)]

use soroban_sdk::{
    contracttype,
    Address,
    Env,
    String,
    Vec,
};

/// Storage keys used throughout the LMS contract.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Student(Address),
}

/// Persistent student profile.
///
/// Stores learner progress and reward information.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StudentProfile {
    /// Student wallet address
    pub wallet: Address,

    /// IDs of enrolled courses
    pub enrolled_courses: Vec<String>,

    /// IDs of completed lessons
    pub completed_lessons: Vec<String>,

    /// Earned certificate IDs
    pub certificates: Vec<String>,

    /// Experience points
    pub xp: u64,

    /// Reward balance
    pub reward_balance: i128,
}

/// Saves a student profile into persistent storage.
pub fn save_student_profile(env: &Env, profile: &StudentProfile) {
    env.storage()
        .persistent()
        .set(&DataKey::Student(profile.wallet.clone()), profile);
}

/// Retrieves a student profile by wallet address.
pub fn get_student_profile(
    env: &Env,
    wallet: &Address,
) -> Option<StudentProfile> {
    env.storage()
        .persistent()
        .get(&DataKey::Student(wallet.clone()))
}

/// Updates an existing student profile.
///
/// Since Soroban storage overwrites values with the same key,
/// updating is equivalent to saving.
pub fn update_student_profile(
    env: &Env,
    profile: &StudentProfile,
) {
    save_student_profile(env, profile);
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::Address as _,
        Env,
        Vec,
    };

    #[test]
    fn save_profile() {
        let env = Env::default();

        let wallet = Address::generate(&env);

        let profile = StudentProfile {
            wallet: wallet.clone(),
            enrolled_courses: Vec::new(&env),
            completed_lessons: Vec::new(&env),
            certificates: Vec::new(&env),
            xp: 100,
            reward_balance: 500,
        };

        save_student_profile(&env, &profile);

        let stored = get_student_profile(&env, &wallet);

        assert!(stored.is_some());
        assert_eq!(stored.unwrap(), profile);
    }

    #[test]
    fn retrieve_profile() {
        let env = Env::default();

        let wallet = Address::generate(&env);

        let profile = StudentProfile {
            wallet: wallet.clone(),
            enrolled_courses: Vec::new(&env),
            completed_lessons: Vec::new(&env),
            certificates: Vec::new(&env),
            xp: 50,
            reward_balance: 1000,
        };

        save_student_profile(&env, &profile);

        let retrieved = get_student_profile(&env, &wallet).unwrap();

        assert_eq!(retrieved.wallet, wallet);
        assert_eq!(retrieved.xp, 50);
        assert_eq!(retrieved.reward_balance, 1000);
    }

    #[test]
    fn update_profile() {
        let env = Env::default();

        let wallet = Address::generate(&env);

        let mut profile = StudentProfile {
            wallet: wallet.clone(),
            enrolled_courses: Vec::new(&env),
            completed_lessons: Vec::new(&env),
            certificates: Vec::new(&env),
            xp: 0,
            reward_balance: 0,
        };

        save_student_profile(&env, &profile);

        profile.xp = 250;
        profile.reward_balance = 5000;

        update_student_profile(&env, &profile);

        let updated = get_student_profile(&env, &wallet).unwrap();

        assert_eq!(updated.xp, 250);
        assert_eq!(updated.reward_balance, 5000);
    }
}
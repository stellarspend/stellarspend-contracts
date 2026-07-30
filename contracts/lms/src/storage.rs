use soroban_sdk::{contracttype, Address, Env, String, Vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Course(u64),
    Lesson(u64),
    Module(u64),
    Quiz(u64),
    Student(Address),
    Certificate(String),
    Progress(Address, u64),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DataKey {
    Student(Address),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StudentProfile {
    pub wallet: Address,
    pub enrolled_courses: Vec<String>,
    pub completed_lessons: Vec<String>,
    pub certificates: Vec<String>,
    pub xp: u64,
    pub reward_balance: i128,
}

pub(crate) fn save_student_profile(env: &Env, profile: &StudentProfile) {
    env.storage()
        .persistent()
        .set(&DataKey::Student(profile.wallet.clone()), profile);
}

pub(crate) fn get_student_profile(env: &Env, wallet: &Address) -> Option<StudentProfile> {
    env.storage()
        .persistent()
        .get(&DataKey::Student(wallet.clone()))
}

pub(crate) fn update_student_profile(env: &Env, profile: &StudentProfile) {
    save_student_profile(env, profile);
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env, Vec};

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

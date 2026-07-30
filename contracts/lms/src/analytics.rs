use soroban_sdk::{contracttype, Address, Env};

/// Storage keys used for analytics counters.
/// These are sample keys and can be replaced with your project's storage model.
#[contracttype]
#[derive(Clone)]
pub enum AnalyticsKey {
    // Global metrics
    CourseCount,
    LessonCount,
    StudentCount,
    CertificateCount,
    RewardCount,
    TotalXpIssued,

    // Instructor-specific metrics
    InstructorStudents(Address),
    InstructorCompleted(Address),
    InstructorQuizPassed(Address),
    InstructorQuizAttempts(Address),
    InstructorActive(Address),
}

/// Dashboard summary returned to the frontend.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DashboardSummary {
    pub courses: u32,
    pub lessons: u32,
    pub students: u32,
    pub certificates: u32,
    pub rewards: u32,
    pub xp_issued: u64,
}

/// Instructor-specific dashboard metrics.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstructorDashboardMetrics {
    pub total_students: u32,
    pub completion_rate: u32,
    pub quiz_success_rate: u32,
    pub active_learners: u32,
}

/// Read a u32 value from storage.
fn read_u32(env: &Env, key: AnalyticsKey) -> u32 {
    env.storage()
        .instance()
        .get::<AnalyticsKey, u32>(&key)
        .unwrap_or(0)
}

/// Read a u64 value from storage.
fn read_u64(env: &Env, key: AnalyticsKey) -> u64 {
    env.storage()
        .instance()
        .get::<AnalyticsKey, u64>(&key)
        .unwrap_or(0)
}

/// Safely calculate a percentage.
fn percentage(value: u32, total: u32) -> u32 {
    if total == 0 {
        0
    } else {
        (value * 100) / total
    }
}

/// Returns a global LMS dashboard summary.
pub fn get_dashboard_summary(env: &Env) -> DashboardSummary {
    DashboardSummary {
        courses: read_u32(env, AnalyticsKey::CourseCount),
        lessons: read_u32(env, AnalyticsKey::LessonCount),
        students: read_u32(env, AnalyticsKey::StudentCount),
        certificates: read_u32(env, AnalyticsKey::CertificateCount),
        rewards: read_u32(env, AnalyticsKey::RewardCount),
        xp_issued: read_u64(env, AnalyticsKey::TotalXpIssued),
    }
}

/// Returns instructor-specific dashboard metrics.
pub fn get_instructor_dashboard(
    env: &Env,
    instructor: Address,
) -> InstructorDashboardMetrics {
    let total_students = read_u32(
        env,
        AnalyticsKey::InstructorStudents(instructor.clone()),
    );

    let completed_students = read_u32(
        env,
        AnalyticsKey::InstructorCompleted(instructor.clone()),
    );

    let quizzes_passed = read_u32(
        env,
        AnalyticsKey::InstructorQuizPassed(instructor.clone()),
    );

    let quizzes_attempted = read_u32(
        env,
        AnalyticsKey::InstructorQuizAttempts(instructor.clone()),
    );

    let active_learners = read_u32(
        env,
        AnalyticsKey::InstructorActive(instructor),
    );

    InstructorDashboardMetrics {
        total_students,
        completion_rate: percentage(completed_students, total_students),
        quiz_success_rate: percentage(quizzes_passed, quizzes_attempted),
        active_learners,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{Address, Env};

    #[test]
    fn dashboard_should_return_zero_when_empty() {
        let env = Env::default();

        let summary = get_dashboard_summary(&env);

        assert_eq!(summary.courses, 0);
        assert_eq!(summary.lessons, 0);
        assert_eq!(summary.students, 0);
        assert_eq!(summary.certificates, 0);
        assert_eq!(summary.rewards, 0);
        assert_eq!(summary.xp_issued, 0);
    }

    #[test]
    fn dashboard_should_return_saved_values() {
        let env = Env::default();

        env.storage()
            .instance()
            .set(&AnalyticsKey::CourseCount, &5u32);

        env.storage()
            .instance()
            .set(&AnalyticsKey::LessonCount, &18u32);

        env.storage()
            .instance()
            .set(&AnalyticsKey::StudentCount, &42u32);

        env.storage()
            .instance()
            .set(&AnalyticsKey::CertificateCount, &11u32);

        env.storage()
            .instance()
            .set(&AnalyticsKey::RewardCount, &7u32);

        env.storage()
            .instance()
            .set(&AnalyticsKey::TotalXpIssued, &24_500u64);

        let summary = get_dashboard_summary(&env);

        assert_eq!(summary.courses, 5);
        assert_eq!(summary.lessons, 18);
        assert_eq!(summary.students, 42);
        assert_eq!(summary.certificates, 11);
        assert_eq!(summary.rewards, 7);
        assert_eq!(summary.xp_issued, 24_500);
    }

    #[test]
    fn percentage_should_handle_zero() {
        assert_eq!(percentage(50, 0), 0);
    }

    #[test]
    fn percentage_should_calculate_correctly() {
        assert_eq!(percentage(75, 100), 75);
        assert_eq!(percentage(50, 200), 25);
        assert_eq!(percentage(9, 10), 90);
    }

    #[test]
    fn instructor_dashboard_should_return_correct_metrics() {
        let env = Env::default();

        let instructor = Address::generate(&env);

        env.storage().instance().set(
            &AnalyticsKey::InstructorStudents(instructor.clone()),
            &100u32,
        );

        env.storage().instance().set(
            &AnalyticsKey::InstructorCompleted(instructor.clone()),
            &80u32,
        );

        env.storage().instance().set(
            &AnalyticsKey::InstructorQuizPassed(instructor.clone()),
            &180u32,
        );

        env.storage().instance().set(
            &AnalyticsKey::InstructorQuizAttempts(instructor.clone()),
            &200u32,
        );

        env.storage().instance().set(
            &AnalyticsKey::InstructorActive(instructor.clone()),
            &35u32,
        );

        let metrics = get_instructor_dashboard(&env, instructor);

        assert_eq!(metrics.total_students, 100);
        assert_eq!(metrics.completion_rate, 80);
        assert_eq!(metrics.quiz_success_rate, 90);
        assert_eq!(metrics.active_learners, 35);
    }
}
use soroban_sdk::{symbol_short, Address, Env, String};

pub struct LMSEvents;

impl LMSEvents {
    /// Emitted when a new course is created
    pub fn emit_course_created(
        env: &Env,
        course_id: u64,
        instructor: Address,
        title: String,
    ) {
        let topics = (symbol_short!("course"), symbol_short!("created"), course_id);
        env.events().publish(topics, (instructor, title));
    }

    /// Emitted when a lesson is added to a course
    pub fn emit_lesson_added(
        env: &Env,
        course_id: u64,
        lesson_id: u64,
        title: String,
    ) {
        let topics = (symbol_short!("lesson"), symbol_short!("added"), course_id);
        env.events().publish(topics, (lesson_id, title));
    }

    /// Emitted when a lesson is removed from a course
    pub fn emit_lesson_removed(env: &Env, course_id: u64, lesson_id: u64) {
        let topics = (symbol_short!("lesson"), symbol_short!("removed"), course_id);
        env.events().publish(topics, lesson_id);
    }

    /// Emitted when a course is published, making it visible/enrollable
    pub fn emit_course_published(
        env: &Env,
        course_id: u64,
        caller: Address,
    ) {
        let topics = (symbol_short!("course"), symbol_short!("publish"), course_id);
        env.events().publish(topics, caller);
    }

    /// Emitted when a course is archived
    pub fn emit_course_archived(
        env: &Env,
        course_id: u64,
        caller: Address,
    ) {
        let topics = (symbol_short!("course"), symbol_short!("archived"), course_id);
        env.events().publish(topics, caller);
    }

    /// Emitted when a student enrolls in a course
    pub fn emit_student_enrolled(
        env: &Env,
        course_id: u64,
        student: Address,
    ) {
        let topics = (symbol_short!("student"), symbol_short!("enrolled"), course_id);
        env.events().publish(topics, student);
    }

    /// Emitted when a student withdraws from a course before completing it
    pub fn emit_student_withdrawn(
        env: &Env,
        course_id: u64,
        student: Address,
    ) {
        let topics = (symbol_short!("student"), symbol_short!("withdrawn"), course_id);
        env.events().publish(topics, student);
    }

    /// Emitted when a student completes a lesson
    pub fn emit_lesson_completed(
        env: &Env,
        course_id: u64,
        lesson_id: u64,
        student: Address,
    ) {
        let topics = (symbol_short!("lesson"), symbol_short!("complete"), course_id);
        env.events().publish(topics, (student, lesson_id));
    }

    /// Emitted when a student completes a quiz
    pub fn emit_quiz_completed(
        env: &Env,
        course_id: u64,
        quiz_id: u64,
        student: Address,
        score: u32,
    ) {
        let topics = (symbol_short!("quiz"), symbol_short!("complete"), course_id);
        env.events().publish(topics, (student, quiz_id, score));
    }

    /// Emitted when an educational certificate is issued
    pub fn emit_certificate_issued(
        env: &Env,
        course_id: u64,
        student: Address,
        certificate_id: u64,
    ) {
        let topics = (symbol_short!("cert"), symbol_short!("issued"), course_id);
        env.events().publish(topics, (student, certificate_id));
    }

    /// Emitted when a student claims a reward token/payout
    pub fn emit_reward_claimed(
        env: &Env,
        course_id: u64,
        student: Address,
        amount: i128,
    ) {
        let topics = (symbol_short!("reward"), symbol_short!("claimed"), course_id);
        env.events().publish(topics, (student, amount));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Events};

    #[test]
    fn test_all_lms_events_published_successfully() {
        let env = Env::default();
        let contract_id = env.register(crate::LMSContract, ());

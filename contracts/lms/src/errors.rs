use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum LmsError {
    CourseNotFound = 1,
    LessonNotFound = 2,
    ModuleNotFound = 3,
    QuizNotFound = 4,

    AlreadyEnrolled = 5,
    AlreadyCompleted = 6,

    Unauthorized = 7,

    InvalidProgress = 8,
    InvalidQuiz = 9,

    NotEnrolled = 10,
}

#[cfg(test)]
mod tests {
    use super::LmsError;

    #[test]
    fn error_values_are_correct() {
        assert_eq!(LmsError::CourseNotFound as u32, 1);
        assert_eq!(LmsError::LessonNotFound as u32, 2);
        assert_eq!(LmsError::ModuleNotFound as u32, 3);
        assert_eq!(LmsError::QuizNotFound as u32, 4);
        assert_eq!(LmsError::AlreadyEnrolled as u32, 5);
        assert_eq!(LmsError::AlreadyCompleted as u32, 6);
        assert_eq!(LmsError::Unauthorized as u32, 7);
        assert_eq!(LmsError::InvalidProgress as u32, 8);
        assert_eq!(LmsError::InvalidQuiz as u32, 9);
        assert_eq!(LmsError::NotEnrolled as u32, 10);
    }
}
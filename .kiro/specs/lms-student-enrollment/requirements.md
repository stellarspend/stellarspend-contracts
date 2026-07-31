# Requirements Document

## Introduction

This feature adds student course enrollment to the LMS (Learning Management System) built on Stellar smart contracts (Rust/Soroban). Students identify themselves by their Stellar wallet address and enroll in published courses via the `enroll_student()` function in `contracts/lms/src/enrollment.rs`. The enrollment record establishes the learner–course relationship on-chain, gating progress tracking and reward eligibility.

## Glossary

- **Enrollment_Contract**: The Soroban smart contract module at `contracts/lms/src/enrollment.rs` that manages student enrollment records.
- **Student**: A Stellar wallet address representing a learner who wishes to enroll in a course.
- **Course**: A published educational unit identified by a unique `course_id` (u64) stored on-chain by the LMS.
- **Course_Registry**: The on-chain storage that records all courses and their publication status.
- **Enrollment_Record**: An on-chain data structure linking a Student address to a Course, including the enrollment timestamp.
- **Published_Course**: A Course whose `status` field is set to `CourseStatus::Published` in the Course_Registry.
- **Enrollment_Timestamp**: The ledger timestamp (u64) recorded at the moment the enrollment transaction is processed.
- **StudentEnrolled**: The Soroban event emitted upon successful enrollment, carrying `(student, course_id, timestamp)` as payload.
- **EnrollmentError**: The contract error enum used to signal invalid enrollment conditions.

---

## Requirements

### Requirement 1: Course Existence Validation

**User Story:** As a Student, I want the system to confirm that the course I am enrolling in actually exists, so that I cannot create phantom enrollment records for non-existent courses.

#### Acceptance Criteria

1. WHEN `enroll_student(course_id, student)` is called with a `course_id` that does not exist in the Course_Registry, THEN THE Enrollment_Contract SHALL return `EnrollmentError::CourseNotFound`.
2. A `course_id` is considered to exist in the Course_Registry IF AND ONLY IF a registered entry is present in on-chain storage under the key `DataKey::Course(course_id)`.
3. IF `enroll_student(course_id, student)` is called with a `course_id` that exists in the Course_Registry, THEN THE Enrollment_Contract SHALL not return `EnrollmentError::CourseNotFound` and SHALL proceed to evaluate subsequent enrollment conditions.

---

### Requirement 2: Published Course Enforcement

**User Story:** As a course administrator, I want enrollment to be restricted to published courses only, so that students cannot enroll in draft or archived courses that are not ready for learners.

#### Acceptance Criteria

1. IF `enroll_student(course_id, student)` is called and the Course identified by `course_id` has a status other than `CourseStatus::Published`, THEN THE Enrollment_Contract SHALL return `EnrollmentError::CourseNotPublished`.
2. IF `enroll_student(course_id, student)` is called and the Course identified by `course_id` does not exist in the Course_Registry, THEN THE Enrollment_Contract SHALL return `EnrollmentError::CourseNotFound` (checked before status).
3. IF `enroll_student(course_id, student)` is called and the Course identified by `course_id` has `status == CourseStatus::Published`, THEN THE Enrollment_Contract SHALL not return `EnrollmentError::CourseNotPublished` and SHALL proceed to evaluate subsequent enrollment conditions.

---

### Requirement 3: Duplicate Enrollment Prevention

**User Story:** As a Student, I want the system to prevent me from enrolling in the same course twice, so that the enrollment ledger remains accurate and reward eligibility is not duplicated.

#### Acceptance Criteria

1. WHEN `enroll_student(course_id, student)` is called and an Enrollment_Record already exists for the `(student, course_id)` pair, THEN THE Enrollment_Contract SHALL return `EnrollmentError::AlreadyEnrolled` and the existing Enrollment_Record SHALL remain unmodified.
2. WHEN `enroll_student(course_id, student)` is called and no Enrollment_Record exists for the `(student, course_id)` pair, THEN THE Enrollment_Contract SHALL proceed to evaluate subsequent enrollment conditions.
3. THE Enrollment_Contract SHALL treat each unique `(student, course_id)` pair as a distinct enrollment slot, such that enrolling Student A in Course 1 does not prevent Student A from enrolling in Course 2, nor Student B from enrolling in Course 1.

---

### Requirement 4: Enrollment Record Persistence

**User Story:** As a Student, I want my enrollment to be stored permanently on-chain, so that my participation in a course is verifiable at any time without relying on off-chain state.

#### Acceptance Criteria

1. WHEN a valid enrollment is processed, THE Enrollment_Contract SHALL write an Enrollment_Record containing the `student` address, `course_id`, and `enrolled_at` (set to `env.ledger().timestamp()` — not caller-supplied) to persistent on-chain storage under the key `DataKey::Enrollment(course_id, student)`.
2. WHEN `get_enrollment(course_id, student)` is called and an Enrollment_Record exists for the `(course_id, student)` pair, THE Enrollment_Contract SHALL return the full stored Enrollment_Record including `student`, `course_id`, and `enrolled_at`.
3. IF `get_enrollment(course_id, student)` is called for a `(course_id, student)` pair with no existing record, THEN THE Enrollment_Contract SHALL return `None`.
4. THE Enrollment_Contract SHALL maintain a per-course enrollment index under `DataKey::CourseEnrollments(course_id)`, written atomically with the Enrollment_Record write in criterion 1. This index SHALL support up to 10,000 enrollments per course.
5. WHEN `enroll_student(course_id, student)` is called for a `(course_id, student)` pair where an Enrollment_Record already exists, THEN THE Enrollment_Contract SHALL return `EnrollmentError::AlreadyEnrolled` and the existing Enrollment_Record SHALL remain unmodified.

---

### Requirement 5: Enrollment Timestamp Recording

**User Story:** As a course administrator, I want each enrollment to carry the ledger timestamp at the time of enrollment, so that enrollment order and timing are auditable on-chain.

#### Acceptance Criteria

1. WHEN a valid enrollment is persisted, THE Enrollment_Contract SHALL set the `enrolled_at` field of the Enrollment_Record to the value returned by `env.ledger().timestamp()` at execution time.
2. THE Enrollment_Contract SHALL store the Enrollment_Timestamp as a `u64` value representing the number of seconds elapsed since the Unix epoch (1970-01-01T00:00:00Z), as provided by the Soroban ledger environment.
3. WHEN an Enrollment_Record has been written to storage, THE `enrolled_at` field SHALL be immutable; no subsequent operation SHALL overwrite or update the timestamp of an existing Enrollment_Record.

---

### Requirement 6: StudentEnrolled Event Emission

**User Story:** As an off-chain indexer or dApp frontend, I want a `StudentEnrolled` event to be emitted on every successful enrollment, so that I can reactively update UI state and analytics without polling on-chain storage.

#### Acceptance Criteria

1. WHEN a valid enrollment is persisted, THE Enrollment_Contract SHALL emit a Soroban event with the topic tuple `(symbol_short!("lms"), symbol_short!("enrolled"), course_id)` and the data payload `(student, enrolled_at)`.
2. THE Enrollment_Contract SHALL emit the StudentEnrolled event exactly once per successful `enroll_student` call.
3. IF `enroll_student` returns any error (CourseNotFound, CourseNotPublished, AlreadyEnrolled, authentication failure, or any contract panic/trap), THEN THE Enrollment_Contract SHALL emit no event.
4. THE StudentEnrolled event SHALL be emitted atomically with the Enrollment_Record write, such that an indexer observing the event can rely on the Enrollment_Record being present in storage within the same transaction.

---

### Requirement 7: Student Authentication

**User Story:** As a course administrator, I want enrollment to require the student's cryptographic authorization, so that no third party can enroll a wallet address without its owner's consent.

#### Acceptance Criteria

1. WHEN `enroll_student(course_id, student)` is called, THE Enrollment_Contract SHALL call `student.require_auth()` as the first operation before any state mutation or storage read that could be used to infer account existence.
2. IF the invoking transaction does not carry a valid signature for the `student` address, THEN THE Enrollment_Contract SHALL abort with a Soroban authentication error and no Enrollment_Record, index update, or event SHALL be written or emitted.
3. IF `enroll_student(course_id, student)` is called by a transaction signed by a different address (not `student`), THEN THE Enrollment_Contract SHALL reject the call with a Soroban authentication error, regardless of whether a valid enrollment record for `(course_id, student)` already exists.

---

### Requirement 8: Enrollment Query Functions

**User Story:** As a dApp or progress-tracking module, I want to query enrollment status and course enrollment lists, so that I can gate content access and display learner rosters.

#### Acceptance Criteria

1. THE Enrollment_Contract SHALL expose `is_enrolled(course_id, student) -> bool` that returns `true` when an Enrollment_Record exists for the `(course_id, student)` pair, `false` otherwise — including when `course_id` does not exist in the Course_Registry.
2. THE Enrollment_Contract SHALL expose `get_course_enrollments(course_id) -> Vec<Address>` that returns the list of all Student addresses enrolled in the given course, ordered chronologically by transaction execution order (ascending `enrolled_at`).
3. WHEN `get_course_enrollments(course_id)` is called for a course with no enrollments or a `course_id` that does not exist in the Course_Registry, THE Enrollment_Contract SHALL return an empty `Vec<Address>` without panicking.
4. THE Enrollment_Contract SHALL cap the `get_course_enrollments` result at 10,000 entries per course, consistent with the enrollment index capacity limit in Requirement 4.

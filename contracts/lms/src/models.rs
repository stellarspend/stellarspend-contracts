use soroban_sdk::{contracttype, Address, String, Vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Course {
    pub course_id: u64,
    pub title: String,
    pub description: String,
    pub category: String,
    pub difficulty: String,
    pub thumbnail_hash: String,
    pub author: Address,
    pub published: bool,
    pub created_at: u64,
    pub updated_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Module {
    pub module_id: u64,
    pub course_id: u64,
    pub title: String,
    pub lesson_ids: Vec<u64>,
    pub display_order: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Lesson {
    pub lesson_id: u64,
    pub course_id: u64,
    pub title: String,
    pub description: String,
    pub content_uri: String,
    pub estimated_duration: u32,
    pub lesson_order: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Quiz {
    pub quiz_id: u64,
    pub lesson_id: u64,
    pub passing_score: u32,
    pub maximum_score: u32,
    pub reward_points: u32,
    pub is_active: bool,
}

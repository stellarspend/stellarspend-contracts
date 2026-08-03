use soroban_sdk::{contracttype, Address, String, Vec};

#[contracttype]
#[derive(Clone)]
pub struct Notification {
    pub recipient: Address,
    pub language: String,
    pub message: String,
}

#[contracttype]
#[derive(Clone)]
pub struct NotificationResult {
    pub recipient: Address,
    pub success: bool,
}

use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum NotificationError {
    EmptyBatch = 1,
    InvalidLanguage = 2,
    EmptyMessage = 3,
}

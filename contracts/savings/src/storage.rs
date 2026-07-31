use soroban_sdk::{contracttype, Address};

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Goal(u64),
    RewardAmount,
    RewardClaimed(Address, u64),
    SavingsLimit(Address),
}

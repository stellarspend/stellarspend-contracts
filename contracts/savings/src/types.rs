use soroban_sdk::contracttype;

#[derive(Clone)]
#[contracttype]
pub struct SavingsGoal {
    pub id: u64,
    pub target_amount: i128,
    pub saved_amount: i128,
    pub completed: bool,
}

#[derive(Clone)]
#[contracttype]
pub enum ContributionPeriod {
    Daily,
    Weekly,
    Monthly,
}

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, Address,
    Env, Symbol, Vec,
};

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    NextId,
    PendingItem(u64),
    HighQueue,
    MedQueue,
    LowQueue,
}

#[derive(Clone)]
#[contracttype]
pub struct PendingItem {
    pub id: u64,
    pub payload: Symbol,
    pub priority: u32,
    pub created_at: u64,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum PriorityError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidPriority = 4,
    EmptyQueue = 5,
    Overflow = 6,
}

pub struct PriorityEvents;

impl PriorityEvents {
    pub fn enqueued(env: &Env, item: &PendingItem) {
        let topics = (
            symbol_short!("priority"),
            symbol_short!("enqueued"),
            item.id,
        );
        env.events().publish(
            topics,
            (item.payload.clone(), item.priority, item.created_at),
        );
    }

    pub fn dequeued(env: &Env, item: &PendingItem) {
        let topics = (
            symbol_short!("priority"),
            symbol_short!("dequeued"),
            item.id,
        );
        env.events().publish(
            topics,
            (item.payload.clone(), item.priority, item.created_at),
        );
    }
}

const STARVATION_THRESHOLD: u64 = 60; // seconds

#[contract]
pub struct PriorityContract;

#[contractimpl]
impl PriorityContract {
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&env, PriorityError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::NextId, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::HighQueue, &Vec::<u64>::new(&env));
        env.storage()
            .instance()
            .set(&DataKey::MedQueue, &Vec::<u64>::new(&env));
        env.storage()
            .instance()
            .set(&DataKey::LowQueue, &Vec::<u64>::new(&env));
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, PriorityError::NotInitialized))
    }

    pub fn enqueue(env: Env, caller: Address, payload: Symbol, priority: u32) -> u64 {
        caller.require_auth();

        if priority > 2 {
            panic_with_error!(&env, PriorityError::InvalidPriority);
        }

        let id: u64 = Self::next_id(&env);
        let created_at = env.ledger().timestamp();

        let item = PendingItem {
            id,
            payload: payload.clone(),
            priority,
            created_at,
        };

        env.storage()
            .instance()
            .set(&DataKey::PendingItem(id), &item);

        // push into proper queue
        match priority {
            0 => Self::push_queue(&env, DataKey::LowQueue, id),
            1 => Self::push_queue(&env, DataKey::MedQueue, id),
            2 => Self::push_queue(&env, DataKey::HighQueue, id),
            _ => panic_with_error!(&env, PriorityError::InvalidPriority),
        }

        PriorityEvents::enqueued(&env, &item);
        id
    }

    pub fn get_priority_score(env: Env, tx_id: u64) -> u32 {
        let item_opt: Option<PendingItem> =
            env.storage().instance().get(&DataKey::PendingItem(tx_id));
        if let Some(item) = item_opt {
            item.priority
        } else {
            0
        }
    }

    pub fn dequeue(env: Env) -> Option<PendingItem> {
        // If any lower-priority item has starved beyond threshold, dequeue it first.
        let now = env.ledger().timestamp();

        if let Some(id) = Self::find_starved(&env, now) {
            return Self::pop_by_id(&env, id);
        }

        // otherwise prefer high, then med, then low
        if let Some(id) = Self::pop_front(&env, DataKey::HighQueue) {
            return Self::pop_by_id(&env, id);
        }
        if let Some(id) = Self::pop_front(&env, DataKey::MedQueue) {
            return Self::pop_by_id(&env, id);
        }
        if let Some(id) = Self::pop_front(&env, DataKey::LowQueue) {
            return Self::pop_by_id(&env, id);
        }

        None
    }
}

impl PriorityContract {
    fn next_id(env: &Env) -> u64 {
        let current: u64 = env.storage().instance().get(&DataKey::NextId).unwrap_or(0);
        let next = current
            .checked_add(1)
            .unwrap_or_else(|| panic_with_error!(env, PriorityError::Overflow));
        env.storage().instance().set(&DataKey::NextId, &next);
        next
    }

    fn push_queue(env: &Env, key: DataKey, id: u64) {
        let mut q: Vec<u64> = env
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| Vec::new(env));
        q.push_back(id);
        env.storage().instance().set(&key, &q);
    }

    fn pop_front(env: &Env, key: DataKey) -> Option<u64> {
        let q: Vec<u64> = env
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| Vec::new(env));
        let len = q.len();
        if len == 0 {
            return None;
        }

        let id = q
            .get(0)
            .unwrap_or_else(|| panic_with_error!(env, PriorityError::EmptyQueue));

        let mut new_q = Vec::new(env);
        let mut i = 1u32;
        while (i as usize) < len as usize {
            let v = q
                .get(i)
                .unwrap_or_else(|| panic_with_error!(env, PriorityError::EmptyQueue));
            new_q.push_back(v);
            i += 1;
        }
        env.storage().instance().set(&key, &new_q);
        Some(id)
    }

    fn pop_by_id(env: &Env, id: u64) -> Option<PendingItem> {
        let item: PendingItem = env
            .storage()
            .instance()
            .get(&DataKey::PendingItem(id))
            .unwrap_or_else(|| panic_with_error!(env, PriorityError::EmptyQueue));

        // remove stored item
        env.storage().instance().remove(&DataKey::PendingItem(id));

        PriorityEvents::dequeued(env, &item);
        Some(item)
    }

    fn find_starved(env: &Env, now: u64) -> Option<u64> {
        // check med then low for starved items
        let med_q: Vec<u64> = env
            .storage()
            .instance()
            .get(&DataKey::MedQueue)
            .unwrap_or_else(|| Vec::new(env));
        if med_q.len() > 0 {
            let mid = med_q
                .get(0)
                .unwrap_or_else(|| panic_with_error!(env, PriorityError::EmptyQueue));
            let item: PendingItem = env
                .storage()
                .instance()
                .get(&DataKey::PendingItem(mid))
                .unwrap_or_else(|| panic_with_error!(env, PriorityError::EmptyQueue));
            if now.saturating_sub(item.created_at) > STARVATION_THRESHOLD {
                return Some(mid);
            }
        }

        let low_q: Vec<u64> = env
            .storage()
            .instance()
            .get(&DataKey::LowQueue)
            .unwrap_or_else(|| Vec::new(env));
        if low_q.len() > 0 {
            let lid = low_q
                .get(0)
                .unwrap_or_else(|| panic_with_error!(env, PriorityError::EmptyQueue));
            let item: PendingItem = env
                .storage()
                .instance()
                .get(&DataKey::PendingItem(lid))
                .unwrap_or_else(|| panic_with_error!(env, PriorityError::EmptyQueue));
            if now.saturating_sub(item.created_at) > STARVATION_THRESHOLD {
                return Some(lid);
            }
        }

        None
    }
}

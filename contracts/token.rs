use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, token,
    Address, Bytes, Env, Map, String, Symbol, Vec, U256,
};

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    TokenSupply,
    Balance(Address),
    Allowance(Address, Address), // owner, spender
    MintCap,
    BurnCap,
    TotalMinted,
    TotalBurned,
    MintHistory(u64), // timestamp
    BurnHistory(u64), // timestamp
    Paused,
    Minters(Address), // authorized minters
}

#[derive(Clone)]
#[contracttype]
pub struct TokenConfig {
    pub name: String,
    pub symbol: String,
    pub decimals: u32,
    pub admin: Address,
    pub mint_cap: Option<i128>,
    pub burn_cap: Option<i128>,
    pub paused: bool,
}

#[derive(Clone)]
#[contracttype]
pub struct MintRecord {
    pub to: Address,
    pub amount: i128,
    pub minter: Address,
    pub timestamp: u64,
    pub transaction_id: U256,
}

#[derive(Clone)]
#[contracttype]
pub struct BurnRecord {
    pub from: Address,
    pub amount: i128,
    pub timestamp: u64,
    pub transaction_id: U256,
    pub burner: Address,
}

#[derive(Clone)]
#[contracttype]
pub struct TokenMetrics {
    pub total_supply: i128,
    pub total_minted: i128,
    pub total_burned: i128,
    pub holders_count: u32,
    pub last_mint_time: Option<u64>,
    pub last_burn_time: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum TokenType {
    Standard = 0,
    Mintable = 1,
    Burnable = 2,
    MintableBurnable = 3,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum TokenError {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
    InsufficientBalance = 4,
    InsufficientAllowance = 5,
    InvalidAmount = 6,
    MintCapExceeded = 7,
    BurnCapExceeded = 8,
    Overflow = 9,
    Underflow = 10,
    Paused = 11,
    InvalidRecipient = 12,
    ZeroAddress = 13,
    InvalidMinter = 14,
}

pub struct TokenEvents;

impl TokenEvents {
    pub fn mint(env: &Env, to: &Address, amount: i128, minter: &Address) {
        let topics = (symbol_short!("mint"), symbol_short!("tokens"));
        env.events().publish(
            topics,
            (to.clone(), amount, minter.clone(), env.ledger().timestamp()),
        );
    }

    pub fn burn(env: &Env, from: &Address, amount: i128, burner: &Address) {
        let topics = (symbol_short!("burn"), symbol_short!("tokens"));
        env.events().publish(
            topics,
            (
                from.clone(),
                amount,
                burner.clone(),
                env.ledger().timestamp(),
            ),
        );
    }

    pub fn transfer(env: &Env, from: &Address, to: &Address, amount: i128) {
        let topics = (symbol_short!("transfer"), symbol_short!("tokens"));
        env.events().publish(
            topics,
            (from.clone(), to.clone(), amount, env.ledger().timestamp()),
        );
    }

    pub fn approval(env: &Env, owner: &Address, spender: &Address, amount: i128) {
        let topics = (symbol_short!("approval"), symbol_short!("tokens"));
        env.events().publish(
            topics,
            (
                owner.clone(),
                spender.clone(),
                amount,
                env.ledger().timestamp(),
            ),
        );
    }

    pub fn mint_cap_reached(env: &Env, attempted: i128, cap: i128) {
        let topics = (symbol_short!("mint"), Symbol::new(env, "cap_reached"));
        env.events()
            .publish(topics, (attempted, cap, env.ledger().timestamp()));
    }

    pub fn burn_cap_reached(env: &Env, attempted: i128, cap: i128) {
        let topics = (symbol_short!("burn"), Symbol::new(env, "cap_reached"));
        env.events()
            .publish(topics, (attempted, cap, env.ledger().timestamp()));
    }

    pub fn supply_changed(env: &Env, new_supply: i128, change: i128, operation: &str) {
        let op = String::from_str(env, operation);
        let topics = (symbol_short!("supply"), symbol_short!("changed"));
        env.events()
            .publish(topics, (new_supply, change, op, env.ledger().timestamp()));
    }

    pub fn minter_added(env: &Env, admin: &Address, minter: &Address) {
        let topics = (symbol_short!("minter"), symbol_short!("added"));
        env.events().publish(
            topics,
            (admin.clone(), minter.clone(), env.ledger().timestamp()),
        );
    }

    pub fn minter_removed(env: &Env, admin: &Address, minter: &Address) {
        let topics = (symbol_short!("minter"), symbol_short!("removed"));
        env.events().publish(
            topics,
            (admin.clone(), minter.clone(), env.ledger().timestamp()),
        );
    }
}

pub fn initialize_token(
    env: &Env,
    admin: Address,
    name: String,
    symbol: String,
    decimals: u32,
    mint_cap: Option<i128>,
    burn_cap: Option<i128>,
) {
    if env.storage().instance().has(&DataKey::Admin) {
        panic_with_error!(env, TokenError::AlreadyInitialized);
    }

    // Validate inputs
    if name.is_empty() {
        panic_with_error!(env, TokenError::InvalidRecipient);
    }
    if symbol.is_empty() {
        panic_with_error!(env, TokenError::InvalidRecipient);
    }
    if decimals > 18 {
        panic_with_error!(env, TokenError::InvalidAmount);
    }

    // Initialize storage
    env.storage().instance().set(&DataKey::Admin, &admin);
    env.storage().instance().set(&DataKey::TokenSupply, &0i128);
    env.storage().instance().set(&DataKey::TotalMinted, &0i128);
    env.storage().instance().set(&DataKey::TotalBurned, &0i128);
    env.storage().instance().set(&DataKey::Paused, &false);
    env.storage()
        .instance()
        .set(&DataKey::Minters(admin.clone()), &true); // Admin is always a minter

    // Set caps if provided
    if let Some(cap) = mint_cap {
        if cap <= 0 {
            panic_with_error!(env, TokenError::InvalidAmount);
        }
        env.storage().instance().set(&DataKey::MintCap, &cap);
    }

    if let Some(cap) = burn_cap {
        if cap <= 0 {
            panic_with_error!(env, TokenError::InvalidAmount);
        }
        env.storage().instance().set(&DataKey::BurnCap, &cap);
    }

    let _config = TokenConfig {
        name: name.clone(),
        symbol: symbol.clone(),
        decimals,
        admin: admin.clone(),
        mint_cap,
        burn_cap,
        paused: false,
    };

    // Store config (for informational purposes)
    env.storage().instance().set(&DataKey::TokenSupply, &0i128);
}

pub fn get_admin(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(env, TokenError::NotInitialized))
}

pub fn require_admin(env: &Env, caller: &Address) {
    caller.require_auth();
    let admin = get_admin(env);
    if admin != caller.clone() {
        panic_with_error!(env, TokenError::Unauthorized);
    }
}

pub fn require_minter(env: &Env, caller: &Address) {
    caller.require_auth();
    if !is_minter(env, caller) {
        panic_with_error!(env, TokenError::InvalidMinter);
    }
}

pub fn is_minter(env: &Env, address: &Address) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::Minters(address.clone()))
        .unwrap_or(false)
}

pub fn add_minter(env: &Env, admin: Address, minter: Address) {
    require_admin(env, &admin);

    if !is_minter(env, &minter) {
        env.storage()
            .instance()
            .set(&DataKey::Minters(minter.clone()), &true);
        TokenEvents::minter_added(env, &admin, &minter);
    }
}

pub fn remove_minter(env: &Env, admin: Address, minter: Address) {
    require_admin(env, &admin);

    // Don't allow removing the admin
    let admin_addr = get_admin(env);
    if minter == admin_addr {
        panic_with_error!(env, TokenError::Unauthorized);
    }

    if is_minter(env, &minter) {
        env.storage()
            .instance()
            .remove(&DataKey::Minters(minter.clone()));
        TokenEvents::minter_removed(env, &admin, &minter);
    }
}

pub fn mint(env: &Env, minter: Address, to: Address, amount: i128) -> U256 {
    require_minter(env, &minter);

    // Validate inputs
    if amount <= 0 {
        panic_with_error!(env, TokenError::InvalidAmount);
    }

    if to == env.current_contract_address() {
        panic_with_error!(env, TokenError::ZeroAddress);
    }

    // Check if paused
    if is_paused(env) {
        panic_with_error!(env, TokenError::Paused);
    }

    // Check mint cap
    let current_supply = get_total_supply(env);
    let new_supply = current_supply
        .checked_add(amount)
        .unwrap_or_else(|| panic_with_error!(env, TokenError::Overflow));

    if let Some(cap) = get_mint_cap(env) {
        if new_supply > cap {
            TokenEvents::mint_cap_reached(env, new_supply, cap);
            panic_with_error!(env, TokenError::MintCapExceeded);
        }
    }

    // Update balances and supply
    let current_balance = get_balance(env, &to);
    let new_balance = current_balance
        .checked_add(amount)
        .unwrap_or_else(|| panic_with_error!(env, TokenError::Overflow));

    env.storage()
        .persistent()
        .set(&DataKey::Balance(to.clone()), &new_balance);
    env.storage()
        .instance()
        .set(&DataKey::TokenSupply, &new_supply);

    // Update statistics
    let total_minted = get_total_minted(env);
    let new_total_minted = total_minted
        .checked_add(amount)
        .unwrap_or_else(|| panic_with_error!(env, TokenError::Overflow));
    env.storage()
        .instance()
        .set(&DataKey::TotalMinted, &new_total_minted);

    // Record mint transaction
    let transaction_id = generate_transaction_id(env);
    let mint_record = MintRecord {
        to: to.clone(),
        amount,
        minter: minter.clone(),
        timestamp: env.ledger().timestamp(),
        transaction_id: transaction_id.clone(),
    };

    env.storage().persistent().set(
        &DataKey::MintHistory(env.ledger().timestamp()),
        &mint_record,
    );

    // Emit events
    TokenEvents::mint(env, &to, amount, &minter);
    TokenEvents::supply_changed(env, new_supply, amount, "mint");

    transaction_id
}

pub fn burn(env: &Env, from: Address, amount: i128) -> U256 {
    from.require_auth();

    // Validate inputs
    if amount <= 0 {
        panic_with_error!(env, TokenError::InvalidAmount);
    }

    // Check if paused
    if is_paused(env) {
        panic_with_error!(env, TokenError::Paused);
    }

    // Check balance
    let current_balance = get_balance(env, &from);
    if current_balance < amount {
        panic_with_error!(env, TokenError::InsufficientBalance);
    }

    // Check burn cap
    let total_burned = get_total_burned(env);
    let new_total_burned = total_burned
        .checked_add(amount)
        .unwrap_or_else(|| panic_with_error!(env, TokenError::Overflow));

    if let Some(cap) = get_burn_cap(env) {
        if new_total_burned > cap {
            TokenEvents::burn_cap_reached(env, new_total_burned, cap);
            panic_with_error!(env, TokenError::BurnCapExceeded);
        }
    }

    // Update balances and supply
    let new_balance = current_balance
        .checked_sub(amount)
        .unwrap_or_else(|| panic_with_error!(env, TokenError::Underflow));
    let current_supply = get_total_supply(env);
    let new_supply = current_supply
        .checked_sub(amount)
        .unwrap_or_else(|| panic_with_error!(env, TokenError::Underflow));

    env.storage()
        .persistent()
        .set(&DataKey::Balance(from.clone()), &new_balance);
    env.storage()
        .instance()
        .set(&DataKey::TokenSupply, &new_supply);

    // Update statistics
    env.storage()
        .instance()
        .set(&DataKey::TotalBurned, &new_total_burned);

    // Remove balance if zero to save storage
    if new_balance == 0 {
        env.storage()
            .persistent()
            .remove(&DataKey::Balance(from.clone()));
    }

    // Record burn transaction
    let transaction_id = generate_transaction_id(env);
    let burn_record = BurnRecord {
        from: from.clone(),
        amount,
        timestamp: env.ledger().timestamp(),
        transaction_id: transaction_id.clone(),
        burner: from.clone(),
    };

    env.storage().persistent().set(
        &DataKey::BurnHistory(env.ledger().timestamp()),
        &burn_record,
    );

    // Emit events
    TokenEvents::burn(env, &from, amount, &from);
    TokenEvents::supply_changed(env, new_supply, -amount, "burn");

    transaction_id
}

pub fn transfer(env: &Env, from: Address, to: Address, amount: i128) {
    from.require_auth();

    // Validate inputs
    if amount <= 0 {
        panic_with_error!(env, TokenError::InvalidAmount);
    }

    if to == env.current_contract_address() {
        panic_with_error!(env, TokenError::ZeroAddress);
    }

    // Check if paused
    if is_paused(env) {
        panic_with_error!(env, TokenError::Paused);
    }

    // Check balance
    let from_balance = get_balance(env, &from);
    if from_balance < amount {
        panic_with_error!(env, TokenError::InsufficientBalance);
    }

    // Update balances
    let new_from_balance = from_balance
        .checked_sub(amount)
        .unwrap_or_else(|| panic_with_error!(env, TokenError::Underflow));
    let to_balance = get_balance(env, &to);
    let new_to_balance = to_balance
        .checked_add(amount)
        .unwrap_or_else(|| panic_with_error!(env, TokenError::Overflow));

    env.storage()
        .persistent()
        .set(&DataKey::Balance(from.clone()), &new_from_balance);
    env.storage()
        .persistent()
        .set(&DataKey::Balance(to.clone()), &new_to_balance);

    // Remove from balance if zero to save storage
    if new_from_balance == 0 {
        env.storage()
            .persistent()
            .remove(&DataKey::Balance(from.clone()));
    }

    // Emit event
    TokenEvents::transfer(env, &from, &to, amount);
}

pub fn approve(env: &Env, owner: Address, spender: Address, amount: i128) {
    owner.require_auth();

    // Validate inputs
    if amount < 0 {
        panic_with_error!(env, TokenError::InvalidAmount);
    }

    if spender == env.current_contract_address() {
        panic_with_error!(env, TokenError::ZeroAddress);
    }

    // Check if paused
    if is_paused(env) {
        panic_with_error!(env, TokenError::Paused);
    }

    // Set allowance
    env.storage()
        .persistent()
        .set(&DataKey::Allowance(owner.clone(), spender.clone()), &amount);

    // Emit event
    TokenEvents::approval(env, &owner, &spender, amount);
}

pub fn transfer_from(env: &Env, spender: Address, from: Address, to: Address, amount: i128) {
    spender.require_auth();

    // Validate inputs
    if amount <= 0 {
        panic_with_error!(env, TokenError::InvalidAmount);
    }

    if to == env.current_contract_address() {
        panic_with_error!(env, TokenError::ZeroAddress);
    }

    // Check if paused
    if is_paused(env) {
        panic_with_error!(env, TokenError::Paused);
    }

    // Check allowance
    let allowance = get_allowance(env, &from, &spender);
    if allowance < amount {
        panic_with_error!(env, TokenError::InsufficientAllowance);
    }

    // Check balance
    let from_balance = get_balance(env, &from);
    if from_balance < amount {
        panic_with_error!(env, TokenError::InsufficientBalance);
    }

    // Update balances
    let new_from_balance = from_balance
        .checked_sub(amount)
        .unwrap_or_else(|| panic_with_error!(env, TokenError::Underflow));
    let to_balance = get_balance(env, &to);
    let new_to_balance = to_balance
        .checked_add(amount)
        .unwrap_or_else(|| panic_with_error!(env, TokenError::Overflow));

    env.storage()
        .persistent()
        .set(&DataKey::Balance(from.clone()), &new_from_balance);
    env.storage()
        .persistent()
        .set(&DataKey::Balance(to.clone()), &new_to_balance);

    // Remove from balance if zero to save storage
    if new_from_balance == 0 {
        env.storage()
            .persistent()
            .remove(&DataKey::Balance(from.clone()));
    }

    // Update allowance
    let new_allowance = allowance
        .checked_sub(amount)
        .unwrap_or_else(|| panic_with_error!(env, TokenError::Underflow));

    if new_allowance == 0 {
        env.storage()
            .persistent()
            .remove(&DataKey::Allowance(from.clone(), spender.clone()));
    } else {
        env.storage().persistent().set(
            &DataKey::Allowance(from.clone(), spender.clone()),
            &new_allowance,
        );
    }

    // Emit events
    TokenEvents::transfer(env, &from, &to, amount);
    TokenEvents::approval(env, &from, &spender, new_allowance);
}

pub fn pause(env: &Env, admin: Address) {
    require_admin(env, &admin);
    env.storage().instance().set(&DataKey::Paused, &true);
}

pub fn unpause(env: &Env, admin: Address) {
    require_admin(env, &admin);
    env.storage().instance().set(&DataKey::Paused, &false);
}

// Query functions

pub fn get_balance(env: &Env, address: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::Balance(address.clone()))
        .unwrap_or(0)
}

pub fn get_total_supply(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::TokenSupply)
        .unwrap_or(0)
}

pub fn get_allowance(env: &Env, owner: &Address, spender: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::Allowance(owner.clone(), spender.clone()))
        .unwrap_or(0)
}

pub fn get_mint_cap(env: &Env) -> Option<i128> {
    env.storage().instance().get(&DataKey::MintCap)
}

pub fn get_burn_cap(env: &Env) -> Option<i128> {
    env.storage().instance().get(&DataKey::BurnCap)
}

pub fn get_total_minted(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::TotalMinted)
        .unwrap_or(0)
}

pub fn get_total_burned(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::TotalBurned)
        .unwrap_or(0)
}

pub fn is_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false)
}

pub fn get_token_metrics(env: &Env) -> TokenMetrics {
    let total_supply = get_total_supply(env);
    let total_minted = get_total_minted(env);
    let total_burned = get_total_burned(env);

    TokenMetrics {
        total_supply,
        total_minted,
        total_burned,
        holders_count: 0,     // Would require iteration to calculate
        last_mint_time: None, // Would require history lookup
        last_burn_time: None, // Would require history lookup
    }
}

// Helper functions

fn generate_transaction_id(env: &Env) -> U256 {
    let timestamp = env.ledger().timestamp();
    let sequence = env.ledger().sequence();
    let mut bytes = [0u8; 32];

    // Simple ID generation based on timestamp and sequence
    bytes[0..8].copy_from_slice(&timestamp.to_be_bytes());
    bytes[8..12].copy_from_slice(&sequence.to_be_bytes());

    let b = Bytes::from_array(env, &bytes);
    U256::from_be_bytes(env, &b)
}

#[contract]
pub struct TokenContract;

#[contractimpl]
impl TokenContract {
    pub fn initialize(
        env: Env,
        admin: Address,
        name: String,
        symbol: String,
        decimals: u32,
        mint_cap: Option<i128>,
        burn_cap: Option<i128>,
    ) {
        initialize_token(&env, admin, name, symbol, decimals, mint_cap, burn_cap);
    }

    pub fn get_admin(env: Env) -> Address {
        get_admin(&env)
    }

    pub fn mint(env: Env, minter: Address, to: Address, amount: i128) -> U256 {
        mint(&env, minter, to, amount)
    }

    pub fn burn(env: Env, from: Address, amount: i128) -> U256 {
        burn(&env, from, amount)
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        transfer(&env, from, to, amount);
    }

    pub fn approve(env: Env, owner: Address, spender: Address, amount: i128) {
        approve(&env, owner, spender, amount);
    }

    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        transfer_from(&env, spender, from, to, amount);
    }

    pub fn add_minter(env: Env, admin: Address, minter: Address) {
        add_minter(&env, admin, minter);
    }

    pub fn remove_minter(env: Env, admin: Address, minter: Address) {
        remove_minter(&env, admin, minter);
    }

    pub fn pause(env: Env, admin: Address) {
        pause(&env, admin);
    }

    pub fn unpause(env: Env, admin: Address) {
        unpause(&env, admin);
    }

    // Query functions
    pub fn balance(env: Env, address: Address) -> i128 {
        get_balance(&env, &address)
    }

    pub fn total_supply(env: Env) -> i128 {
        get_total_supply(&env)
    }

    pub fn allowance(env: Env, owner: Address, spender: Address) -> i128 {
        get_allowance(&env, &owner, &spender)
    }

    pub fn mint_cap(env: Env) -> Option<i128> {
        get_mint_cap(&env)
    }

    pub fn burn_cap(env: Env) -> Option<i128> {
        get_burn_cap(&env)
    }

    pub fn total_minted(env: Env) -> i128 {
        get_total_minted(&env)
    }

    pub fn total_burned(env: Env) -> i128 {
        get_total_burned(&env)
    }

    pub fn is_paused(env: Env) -> bool {
        is_paused(&env)
    }

    pub fn is_minter(env: Env, address: Address) -> bool {
        is_minter(&env, &address)
    }

    pub fn token_metrics(env: Env) -> TokenMetrics {
        get_token_metrics(&env)
    }

    pub fn get_token_balance(env: Env, owner: Address) -> i128 {
        get_balance(&env, &owner)
    }
}

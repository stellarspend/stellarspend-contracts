# Batch Rewards Distribution Contract

A Soroban smart contract for distributing rewards to multiple recipients in a single batch operation. This contract handles partial failures gracefully, validates reward amounts, and emits detailed events for all operations.

## Features

- **Batch Distribution**: Distribute rewards to up to 100 recipients in a single transaction
- **Amount Validation**: Ensures all reward amounts are positive and within reasonable bounds
- **Partial Failure Handling**: Continues processing even if individual rewards fail, with detailed error reporting
- **Event Emission**: Emits comprehensive events for batch start, individual successes/failures, and batch completion
- **Statistics Tracking**: Maintains cumulative statistics for total batches, rewards processed, and volume distributed
- **Authorization**: Admin-only access with configurable admin address
- **Error Handling**: Detailed error codes for different failure scenarios

## Contract Design

### Core Components

1. **Types** (`types.rs`)
   - `RewardRequest`: Structure for requesting reward distribution (recipient, amount)
   - `RewardResult`: Enum representing success or failure of individual reward
   - `BatchRewardResult`: Complete result of batch operation with aggregate statistics
   - `RewardEvents`: Event emission helpers
   - `DataKey`: Storage keys for contract state

2. **Validation** (`validation.rs`)
   - `validate_amount()`: Ensures amounts are positive and within bounds
   - `validate_address()`: Validates recipient addresses
   - Comprehensive test coverage for validation logic

3. **Main Contract** (`lib.rs`)
   - `initialize()`: Contract initialization with admin setup
   - `distribute_rewards()`: Main batch distribution function
   - `set_admin()`: Admin address management
   - Getter functions for contract statistics

### Error Codes

- `1`: NotInitialized - Contract not initialized
- `2`: Unauthorized - Caller is not the admin
- `3`: InvalidBatch - Invalid batch data
- `4`: EmptyBatch - Batch contains no rewards
- `5`: BatchTooLarge - Batch exceeds maximum size (100)
- `6`: InvalidToken - Invalid token contract
- `7`: InsufficientBalance - Admin lacks sufficient tokens
- `8`: InvalidAmount - Invalid reward amount

## API Reference

### Initialization

```rust
pub fn initialize(env: Env, admin: Address)
```
Initializes the contract with an admin address. Can only be called once.

### Core Functions

```rust
pub fn distribute_rewards(
    env: Env,
    caller: Address,
    token: Address,
    rewards: Vec<RewardRequest>,
) -> BatchRewardResult
```

Distributes rewards to multiple recipients. Returns detailed results including success/failure breakdown.

**Parameters:**
- `caller`: The transaction caller (must be admin)
- `token`: Token contract address (e.g., XLM StellarAssetContract)
- `rewards`: Vector of reward requests

**Returns:**
- `BatchRewardResult` containing:
  - `total_requests`: Number of reward requests processed
  - `successful`: Number of successful distributions
  - `failed`: Number of failed distributions
  - `total_distributed`: Total amount successfully distributed
  - `results`: Vector of individual results for each request

### Admin Functions

```rust
pub fn set_admin(env: Env, caller: Address, new_admin: Address)
```

Changes the contract admin. Only callable by current admin.

### Getter Functions

```rust
pub fn get_admin(env: Env) -> Address
pub fn get_total_batches(env: Env) -> u64
pub fn get_total_rewards_processed(env: Env) -> u64
pub fn get_total_volume_distributed(env: Env) -> i128
```

## Events

The contract emits the following events:

1. **batch_started**: Emitted when batch processing begins
   - Topics: `batch`, `started`
   - Data: `(batch_id, request_count)`

2. **reward_success**: Emitted for each successful reward
   - Topics: `reward`, `success`, `batch_id`
   - Data: `(recipient, amount)`

3. **reward_failure**: Emitted for each failed reward
   - Topics: `reward`, `failure`, `batch_id`
   - Data: `(recipient, amount, error_code)`

4. **batch_completed**: Emitted when batch processing finishes
   - Topics: `batch`, `completed`
   - Data: `(batch_id, successful, failed, total_distributed)`

5. **admin** (set_admin only): Emitted when admin changes
   - Topics: `admin`
   - Data: `(new_admin_address,)`

## Test Coverage

The contract includes comprehensive tests covering:

### Initialization Tests
- ✅ Contract initialization
- ✅ Prevent double initialization

### Basic Distribution Tests
- ✅ Single recipient distribution
- ✅ Multiple recipient distribution
- ✅ Partial failures (mixed valid/invalid amounts)
- ✅ Large batches (up to 100 recipients)

### Validation Tests
- ✅ Zero amount rejection
- ✅ Negative amount rejection
- ✅ Empty batch rejection
- ✅ Oversized batch rejection (>100)

### Error Handling Tests
- ✅ Insufficient balance detection
- ✅ Unauthorized caller rejection
- ✅ Invalid amount handling
- ✅ Token contract failures

### Event Tests
- ✅ Event emission verification
- ✅ Failure event emission
- ✅ Batch completion event validation

### Statistics Tests
- ✅ Statistics accumulation across multiple batches
- ✅ Cumulative volume tracking
- ✅ Rewards counter accuracy

### Advanced Tests
- ✅ Multiple simultaneous batches (30 rewards across 3 batches)
- ✅ Result structure validation
- ✅ Individual result accuracy

## Usage Example

```rust
// Initialize contract
client.initialize(&admin);

// Create reward requests
let mut rewards = Vec::new(&env);
rewards.push_back(RewardRequest {
    recipient: recipient1.clone(),
    amount: 10_000_000, // 1 XLM equivalent
});
rewards.push_back(RewardRequest {
    recipient: recipient2.clone(),
    amount: 5_000_000, // 0.5 XLM equivalent
});

// Distribute rewards
let result = client.distribute_rewards(&admin, &token_address, &rewards);

// Check results
println!("Successful: {}", result.successful);
println!("Failed: {}", result.failed);
println!("Total Distributed: {}", result.total_distributed);
```

## Build and Test

```bash
# Build the contract
cd contracts/batch-rewards
cargo build --release --target wasm32-unknown-unknown

# Run tests
cargo test --lib
```

## Constants

- `MAX_BATCH_SIZE`: 100 (maximum rewards per batch operation)

## Gas Optimization

The contract is optimized for Soroban with:
- Minimal state operations
- Efficient error handling
- Batched event emission
- Streamlined validation logic

## Security Considerations

1. **Authorization**: Only admin can initiate distributions
2. **Amount Validation**: All amounts verified before processing
3. **Balance Verification**: Checks sufficient balance before batch starts
4. **Partial Failure Safety**: Individual failures don't prevent other rewards
5. **Event Logging**: All operations logged for audit trail

## Integration with Stellar

This contract works with Stellar's XLM and other token contracts:
- Uses standard `token::Client` interface
- Compatible with StellarAssetContract
- Follows Soroban best practices

## Future Enhancements

- Scheduled reward distribution
- Reward pool management
- Multi-currency support
- Gas-efficient pagination for monitoring
- Reward reversal/reclamation mechanisms

# Requirements Document

## Introduction

The reward categories feature extends the StellarSpend rewards contract so that every reward transaction carries a business-level classification at the time of issuance. A new `RewardCategory` enum is added to `types.rs` with six variants: `BudgetReward`, `SavingsReward`, `SpendingReward`, `BonusReward`, `PromotionalReward`, and `Other`. The `RewardTransaction` struct gains a mandatory `category` field, and both `credit_reward` and `debit_reward` entry points gain a corresponding `category` parameter. No storage keys are removed or retyped; `storage.rs` and `events.rs` require no signature changes.

## Glossary

- **RewardsContract**: The Soroban smart contract defined in `lib.rs` that exposes all public entry points for reward management.
- **Rewards_Logic**: The business logic layer in `rewards.rs` that implements `credit_reward` and `debit_reward`.
- **Storage**: The persistent storage helper layer in `storage.rs` responsible for reading and writing on-chain state.
- **RewardCategory**: The new `#[contracttype]` enum defined in `types.rs` that classifies the business reason for a reward transaction.
- **RewardTransaction**: The on-chain struct persisted under `DataKey::RewardTransaction(id)` that records all fields of a single credit or debit event.
- **RewardType**: The existing enum classifying the mechanism of reward issuance (e.g. `SpendingLimit`, `SavingsGoal`).
- **RewardStatus**: The existing enum representing the lifecycle state of a transaction (`Confirmed`, `Claimed`, etc.).
- **Admin**: The privileged address that authorises all `credit_reward` and `debit_reward` calls.
- **Participant**: A registered reward account holder who receives or claims rewards.
- **tx_id**: The monotonically incrementing `u64` identifier assigned to each `RewardTransaction`.
- **stroops**: The smallest unit of currency used for all `amount` fields (`i128`).

---

## Requirements

### Requirement 1: RewardCategory Enum Definition

**User Story:** As a protocol developer, I want a `RewardCategory` enum available in the contract type system, so that reward transactions can be classified by business purpose at compile time and stored on-chain with full type safety.

#### Acceptance Criteria

1. THE RewardsContract SHALL define a `RewardCategory` enum containing exactly the following unit variants (no associated data): `BudgetReward`, `SavingsReward`, `SpendingReward`, `BonusReward`, `PromotionalReward`, and `Other`.
2. THE RewardsContract SHALL annotate `RewardCategory` with `#[contracttype]` and derive `Clone`, `Debug`, `Eq`, and `PartialEq` so that all variants are serialisable to and deserialisable from XDR by the Soroban SDK and comparable by value in tests.
3. THE RewardsContract SHALL make `RewardCategory` available to callers via `pub use crate::types::RewardCategory` in `lib.rs`, in the same `pub use` block as the existing exports (`RewardType`, `RewardStatus`, `RewardTransaction`, `RewardAccount`, `DataKey`), so that test harnesses and contract clients can reference it without a direct import of `types.rs`.
4. WHEN any of the six `RewardCategory` variants is serialised using the Soroban `#[contracttype]` XDR codec and then deserialised, THE RewardsContract SHALL produce a value that compares equal (via `PartialEq`) to the original variant, for every variant in the enum.

---

### Requirement 2: RewardTransaction Category Field

**User Story:** As a protocol developer, I want every `RewardTransaction` record to carry a `category` field, so that the business classification of each reward is stored on-chain and available for querying without off-chain enrichment.

#### Acceptance Criteria

1. THE RewardsContract SHALL include a `pub category: RewardCategory` field in the `RewardTransaction` struct.
2. WHEN a `RewardTransaction` is constructed with a given `RewardCategory` variant and written to persistent storage via `set_reward_transaction`, and then read back via `get_reward_transaction` with the same `id`, THE Storage SHALL return a `RewardTransaction` whose `category` field compares equal to the variant that was written — for all six `RewardCategory` variants.
3. WHEN `get_reward_transaction` is called with an `id` for which no record exists in persistent storage, THE Storage SHALL return `None` without panicking.
4. WHEN `get_reward_transaction` is called with an `id` whose stored bytes do not deserialise into the current `RewardTransaction` schema (e.g. a record written before the `category` field was added), THE Storage SHALL return `None` rather than propagating a deserialisation panic.

---

### Requirement 3: credit_reward Category Parameter

**User Story:** As a protocol admin, I want to supply a reward category when crediting rewards, so that each credit transaction is permanently tagged with the business reason it was issued.

#### Acceptance Criteria

1. THE RewardsContract SHALL add a `category: RewardCategory` parameter to the `credit_reward` entry point in `lib.rs`.
2. THE Rewards_Logic SHALL add a `category: RewardCategory` parameter to the `credit_reward` function in `rewards.rs`.
3. WHEN `credit_reward` is called with a registered participant, an `amount` greater than zero, a valid `RewardType`, and any `RewardCategory` variant `c`, THE Rewards_Logic SHALL store a `RewardTransaction` at the assigned `tx_id` whose `category` field equals `c`, verifiable by calling `get_reward_transaction(tx_id)`.
4. WHEN `credit_reward` is called with `RewardCategory::BudgetReward`, THE Rewards_Logic SHALL store a `RewardTransaction` with `category == RewardCategory::BudgetReward`.
5. WHEN `credit_reward` succeeds, THE RewardsContract SHALL return a `RewardTransaction` to the caller whose fields are: `id` equal to the assigned `tx_id`, `recipient` equal to the supplied participant address, `amount` equal to the supplied amount, `reward_type` equal to the supplied `RewardType`, `category` equal to the supplied `RewardCategory`, `status` equal to `RewardStatus::Confirmed`, `created_at` equal to the current ledger sequence, and `updated_at` equal to `0`.

---

### Requirement 4: debit_reward Category Parameter

**User Story:** As a protocol admin, I want to supply a reward category when debiting (claiming) rewards, so that each debit transaction is permanently tagged with the category of the reward being redeemed.

#### Acceptance Criteria

1. THE RewardsContract SHALL add a `category: RewardCategory` parameter to the `debit_reward` entry point in `lib.rs`.
2. THE Rewards_Logic SHALL add a `category: RewardCategory` parameter to the `debit_reward` function in `rewards.rs`.
3. WHEN `debit_reward` is called with a registered participant whose current `reward_balance` is at least `amount`, an `amount` greater than zero, a valid `RewardType`, and any `RewardCategory` variant `c`, THE Rewards_Logic SHALL store a `RewardTransaction` at the assigned `tx_id` whose `category` field equals `c`, verifiable by calling `get_reward_transaction(tx_id)`.
4. WHEN `debit_reward` succeeds, THE Rewards_Logic SHALL store the `RewardTransaction` with `status == RewardStatus::Claimed` and `category` equal to the supplied variant.
5. WHEN `debit_reward` succeeds, THE RewardsContract SHALL return a `RewardTransaction` to the caller whose fields are: `id` equal to the assigned `tx_id`, `recipient` equal to the participant address, `amount` equal to the supplied amount, `reward_type` equal to the supplied `RewardType`, `category` equal to the supplied `RewardCategory`, `status` equal to `RewardStatus::Claimed`, `created_at` equal to the current ledger sequence, and `updated_at` equal to `0`.
6. IF `debit_reward` is called with `amount <= 0`, THEN THE Rewards_Logic SHALL return `RewardsError::InvalidAmount` regardless of the supplied `RewardCategory`, and no storage state SHALL be mutated.

---

### Requirement 5: Category Does Not Affect Balance Arithmetic

**User Story:** As a protocol developer, I want the addition of the category field to leave all balance calculations unchanged, so that existing balance invariants are preserved regardless of which category is supplied.

#### Acceptance Criteria

1. WHEN `credit_reward` is called with any `RewardCategory` variant, a registered participant whose current `reward_balance` is `old_balance`, and an `amount` in the range `[1, i128::MAX - old_balance]` stroops, THE Rewards_Logic SHALL set `reward_balance(participant)` to `old_balance + amount` and `lifetime_earned(participant)` to `old_lifetime_earned + amount` in the same operation.
2. WHEN `debit_reward` is called with any `RewardCategory` variant, a registered participant whose current `reward_balance` is `old_balance >= amount`, and an `amount` in the range `[1, old_balance]` stroops, THE Rewards_Logic SHALL set `reward_balance(participant)` to `old_balance - amount` and `lifetime_claimed(participant)` to `old_lifetime_claimed + amount` in the same operation.
3. WHEN `credit_reward` or `debit_reward` is called with any `RewardCategory` variant and all other preconditions are met, THE Rewards_Logic SHALL return `Ok(RewardTransaction)` without emitting a `RewardsError` attributable to the `category` parameter.

---

### Requirement 6: Category Visibility in Transaction History

**User Story:** As an off-chain indexer or analytics consumer, I want every transaction ID returned by `get_transactions_for` to resolve to a `RewardTransaction` that includes a `category` field, so that I can segment reward history by business classification without additional data sources.

#### Acceptance Criteria

1. WHEN `get_transactions_for(participant)` returns a non-empty list of transaction IDs, THE RewardsContract SHALL ensure each ID in the list resolves via `get_reward_transaction(id)` to a `RewardTransaction` whose `category` field equals the `RewardCategory` variant supplied in the corresponding `credit_reward` or `debit_reward` call.
2. IF `get_reward_transaction` is called with an `id` that was never assigned by a `credit_reward` or `debit_reward` call, THEN THE Storage SHALL return `None`.
3. WHEN `credit_reward` or `debit_reward` completes successfully, THE Rewards_Logic SHALL append the new `tx_id` to the participant's reward index in the same ledger operation that writes the `RewardTransaction`, so that the transaction is retrievable via `get_transactions_for` immediately after the call completes and no `tx_id` appears in the index before its corresponding `RewardTransaction` is durably written.

---

### Requirement 7: Transaction Counter Integrity with Category

**User Story:** As a protocol developer, I want the transaction ID counter to advance correctly regardless of the category supplied, so that each transaction receives a unique ID and no IDs are skipped or duplicated.

#### Acceptance Criteria

1. WHEN `credit_reward` succeeds with any `RewardCategory` variant, THE Rewards_Logic SHALL write a `RewardTransaction` with `id == counter_before` and set the reward transaction counter to `counter_before + 1`.
2. WHEN `debit_reward` succeeds with any `RewardCategory` variant, THE Rewards_Logic SHALL write a `RewardTransaction` with `id == counter_before` and set the reward transaction counter to `counter_before + 1`.
3. WHEN `credit_reward` or `debit_reward` is called multiple times — with any combination of `RewardCategory` variants, any participants, and any valid amounts — each resulting `RewardTransaction` SHALL have a `tx_id` that is unique across all transactions ever written by the contract.
4. WHEN `credit_reward` or `debit_reward` fails for any reason (invalid amount, unregistered participant, insufficient balance, etc.), THE Rewards_Logic SHALL NOT increment the reward transaction counter.

---

### Requirement 8: Existing Error Behaviour Preserved

**User Story:** As a protocol developer, I want all pre-existing error conditions to continue firing correctly after the category parameter is added, so that callers cannot bypass validation by choosing a particular category.

#### Acceptance Criteria

1. IF `credit_reward` or `debit_reward` is called before the contract is initialised, THEN THE RewardsContract SHALL panic with `RewardsError::NotInitialized` regardless of the supplied `RewardCategory`.
2. IF `credit_reward` or `debit_reward` is called by a non-admin address, THEN THE RewardsContract SHALL panic with `RewardsError::Unauthorized` regardless of the supplied `RewardCategory`.
3. IF `credit_reward` is called for an unregistered participant, THEN THE Rewards_Logic SHALL return `RewardsError::AccountNotFound` regardless of the supplied `RewardCategory`.
4. IF `credit_reward` or `debit_reward` is called with `amount <= 0`, THEN THE Rewards_Logic SHALL return `RewardsError::InvalidAmount` regardless of the supplied `RewardCategory`, and no storage state SHALL be mutated.
5. IF `debit_reward` is called with an `amount` that exceeds the participant's current claimable balance, THEN THE Rewards_Logic SHALL return `RewardsError::InsufficientBalance` regardless of the supplied `RewardCategory`, and no storage state SHALL be mutated.
6. THE RewardsContract SHALL introduce no new `RewardsError` variants for the `category` parameter.
7. WHEN `credit_reward` is called with both an unregistered participant and an invalid amount, THE Rewards_Logic SHALL return `RewardsError::AccountNotFound` (not `InvalidAmount`), reflecting the validation order: `validate_contract_initialized` → `validate_account_registered` → `validate_reward_amount`.

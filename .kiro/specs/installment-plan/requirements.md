# Requirements Document

## Introduction

The Installment Plan contract is a new Soroban smart contract crate for the StellarSpend suite (`contracts/installment-plan/`). It enables structured debt repayment between two parties: a payer who owes a fixed total amount to a payee, settled across a schedule of periodic partial payments. The contract enforces payment deadlines, accrues penalties in basis points when installments are missed, tracks the remaining balance at all times, and automatically closes (with an on-chain event) once the debt is fully settled. This fills the gap where StellarSpend currently has no mechanism for anything other than single-transaction repayment.

## Glossary

- **InstallmentPlan_Contract**: The Soroban smart contract defined in this crate.
- **Plan**: A single installment plan record stored in persistent storage, identified by a unique `plan_id`.
- **Payer**: The address obligated to make periodic payments against a Plan.
- **Payee**: The address entitled to receive payments and to claim penalties on a Plan.
- **Token**: The SEP-41 / Soroban token contract address used for all transfers on a Plan.
- **TotalAmount**: The original gross debt amount recorded when the Plan is created, expressed as a positive `i128`.
- **NumInstallments**: The total number of scheduled payment installments for a Plan (≥ 1).
- **Interval**: The number of seconds between consecutive installment due dates (> 0).
- **StartTimestamp**: The ledger timestamp of the first installment due date (must be ≥ current ledger timestamp at creation time).
- **PenaltyBps**: The penalty rate in basis points (1 bps = 0.01%) applied to the remaining balance when an installment is overdue (0 – 10 000 inclusive).
- **RemainingBalance**: The current outstanding balance on a Plan, initially equal to `TotalAmount` and decremented by each payment.
- **InstallmentIndex**: The 0-based count of installments paid so far (0 = none paid, incremented on each successful partial payment).
- **DueDate**: The ledger timestamp by which the installment at `InstallmentIndex` must be paid, calculated as `StartTimestamp + (InstallmentIndex × Interval)`.
- **PlanStatus**: A `#[contracttype]` struct returned by `get_plan_status` summarising the state of a Plan.
- **PlanState**: A `#[contracttype]` enum value (`Active` | `Completed`) stored on each Plan record.
- **Validator**: The internal validation module (`validation.rs`) that checks input parameters before state is mutated.
- **LEDGER_BUMP**: Storage TTL constant equal to `17_280` ledgers (≈ 24 h on Stellar mainnet), declared in `storage.rs`.

## Requirements

---

### Requirement 1: Plan Creation

**User Story:** As a Payee, I want to create an installment plan specifying the payer, token, total debt, schedule, and penalty rate, so that the repayment terms are recorded on-chain and enforceable.

#### Acceptance Criteria

1. WHEN `create_plan` is called, THE `InstallmentPlan_Contract` SHALL require authorization from the Payee caller before any state is read or written.
2. IF `create_plan` is called with a `total_amount` ≤ 0, THEN THE `InstallmentPlan_Contract` SHALL return `InvalidAmount` without writing any state.
3. IF `create_plan` is called with `num_installments` = 0, THEN THE `InstallmentPlan_Contract` SHALL return `InvalidInstallments` without writing any state.
4. IF `create_plan` is called with `interval` = 0, THEN THE `InstallmentPlan_Contract` SHALL return `InvalidInterval` without writing any state.
5. IF `create_plan` is called with `penalty_bps` > 10 000, THEN THE `InstallmentPlan_Contract` SHALL return `InvalidPenaltyBps` without writing any state.
6. IF `create_plan` is called with a `start_timestamp` less than the current ledger timestamp, THEN THE `InstallmentPlan_Contract` SHALL return `InvalidStartTimestamp` without writing any state.
7. IF `create_plan` is called with `payer` equal to `payee`, THEN THE `InstallmentPlan_Contract` SHALL return `InvalidParties` without writing any state.
8. WHEN `create_plan` is called with all valid parameters, THE `InstallmentPlan_Contract` SHALL store a Plan record in `env.storage().persistent()` keyed by `DataKey::Plan(plan_id)` containing: `plan_id`, `payer`, `payee`, `token`, `total_amount`, `num_installments`, `interval`, `start_timestamp`, `penalty_bps`, `remaining_balance` equal to `total_amount`, `installment_index` equal to 0, and `state` equal to `Active`.
9. WHEN `create_plan` succeeds, THE `InstallmentPlan_Contract` SHALL return the new `plan_id`, which equals the value of `PlanCounter` after incrementing it by 1 (starting from 0).
10. WHEN `create_plan` succeeds, THE `InstallmentPlan_Contract` SHALL emit a `plan_created` event containing `plan_id`, `payer`, `payee`, `token`, `total_amount`, `num_installments`, `interval`, `start_timestamp`, and `penalty_bps`.

---

### Requirement 2: Payment Processing

**User Story:** As a Payer, I want to submit a payment against my plan and have the remaining balance updated, so that my progress toward full settlement is recorded on-chain.

#### Acceptance Criteria

1. WHEN `make_payment` is called, THE `InstallmentPlan_Contract` SHALL require authorization from the caller before any state is read or modified.
2. IF `make_payment` is called by an address that is not the Payer on the referenced Plan, THEN THE `InstallmentPlan_Contract` SHALL return `Unauthorized` before any token transfer occurs.
3. IF `make_payment` is called on a Plan whose `state` is `Completed`, THEN THE `InstallmentPlan_Contract` SHALL return `PlanAlreadyCompleted` without transferring any tokens.
4. IF `make_payment` is called with `amount` ≤ 0, THEN THE `InstallmentPlan_Contract` SHALL return `InvalidAmount` without transferring any tokens.
5. IF `make_payment` is called with a `plan_id` that does not exist in storage, THEN THE `InstallmentPlan_Contract` SHALL return `PlanNotFound`.
6. WHEN `make_payment` is called with a valid `amount` strictly less than the current `remaining_balance`, THE `InstallmentPlan_Contract` SHALL transfer exactly `amount` tokens from the Payer to the Payee using `token::Client`, decrement `remaining_balance` by `amount`, and increment `installment_index` by 1.
7. WHEN `make_payment` is called with a valid `amount` that equals or exceeds the current `remaining_balance`, THE `InstallmentPlan_Contract` SHALL transfer exactly `remaining_balance` tokens (not `amount`) from the Payer to the Payee, set `remaining_balance` to 0, set `state` to `Completed`, remove the Plan record from persistent storage, and emit a `plan_completed` event containing `plan_id`, `payer`, `payee`, and `total_amount`.
8. WHEN `make_payment` succeeds, THE `InstallmentPlan_Contract` SHALL emit a `payment_made` event containing `plan_id`, `payer`, `amount_paid` (the actual tokens transferred), and `remaining_balance` (the value after decrement).

---

### Requirement 3: Plan Status Query

**User Story:** As any caller, I want to query the current status of an installment plan, so that I can display repayment progress and the next due date.

#### Acceptance Criteria

1. THE `InstallmentPlan_Contract` SHALL expose a read-only `get_plan_status` function that accepts a `plan_id` and returns a `PlanStatus` value without requiring any authorization.
2. IF `get_plan_status` is called with a `plan_id` that does not exist in storage, THEN THE `InstallmentPlan_Contract` SHALL return `PlanNotFound`.
3. WHEN `get_plan_status` is called on an existing Plan, THE `InstallmentPlan_Contract` SHALL return a `PlanStatus` struct containing:
   - `state`: the current plan state (`Active` or `Completed`);
   - `installment_index`: the 0-based count of installments paid so far;
   - `amount_paid`: calculated as `total_amount - remaining_balance`;
   - `remaining_balance`: the current outstanding balance;
   - `next_due_date`: calculated as `start_timestamp + (installment_index × interval)`;
   - `is_overdue`: set to `true` only when the current ledger timestamp is strictly greater than `next_due_date` AND `state` is `Active`; set to `false` when `state` is `Completed` or any non-`Active` state, regardless of the current timestamp.

---

### Requirement 4: Penalty Accrual

**User Story:** As a Payee, I want to claim a penalty when an installment deadline has passed, so that the payer's remaining balance increases to compensate for the delay.

#### Acceptance Criteria

1. WHEN `claim_penalty` is called for a Plan identified by `plan_id`, THE `InstallmentPlan_Contract` SHALL verify the caller holds a valid authorization signature before reading or mutating any state.
2. IF `claim_penalty` is called by an address that is not the Payee on the referenced Plan, THEN THE `InstallmentPlan_Contract` SHALL return `Unauthorized` before any state mutation occurs.
3. IF `claim_penalty` is called on a Plan whose `state` is `Completed`, THEN THE `InstallmentPlan_Contract` SHALL return `PlanAlreadyCompleted` without mutating any state.
4. IF `claim_penalty` is called when the current ledger timestamp is ≤ `next_due_date` of the installment at the current installment index, THEN THE `InstallmentPlan_Contract` SHALL return `InstallmentNotOverdue` without mutating any state.
5. WHEN `claim_penalty` is called on an overdue Plan with `penalty_bps` = 0, THE `InstallmentPlan_Contract` SHALL calculate `penalty_amount` as 0 and leave `remaining_balance` unchanged.
6. WHEN `claim_penalty` is called on an overdue Plan with `penalty_bps` > 0, THE `InstallmentPlan_Contract` SHALL calculate `penalty_amount` as `remaining_balance × penalty_bps / 10 000` (integer division, truncating toward zero) and add `penalty_amount` to `remaining_balance` using overflow-safe arithmetic; IF the addition would overflow `i128::MAX`, THEN THE `InstallmentPlan_Contract` SHALL return `BalanceOverflow` and leave `remaining_balance` unchanged.
7. WHEN `claim_penalty` succeeds (including the zero-penalty case), THE `InstallmentPlan_Contract` SHALL emit a `penalty_claimed` event containing `plan_id`, `payee`, `penalty_amount`, and the updated `remaining_balance`.

---

### Requirement 5: Plan Closure

**User Story:** As any party, I want the plan to close automatically or on demand when the remaining balance reaches zero, so that on-chain state is cleaned up and completion is verifiable.

#### Acceptance Criteria

1. WHEN `close_plan` is called, THE `InstallmentPlan_Contract` SHALL require authorization from either the Payer or the Payee on the referenced Plan before reading or mutating any state.
2. IF `close_plan` is called with a `plan_id` that does not exist in storage, THEN THE `InstallmentPlan_Contract` SHALL return `PlanNotFound`.
3. IF `close_plan` is called on a Plan whose `state` is already `Completed`, THEN THE `InstallmentPlan_Contract` SHALL return `PlanAlreadyCompleted` without mutating any state.
4. IF `close_plan` is called on a Plan with `remaining_balance` > 0, THEN THE `InstallmentPlan_Contract` SHALL return `BalanceNotZero` without mutating any state.
5. WHEN `close_plan` is called on a Plan with `remaining_balance` = 0 and `state` = `Active`, THE `InstallmentPlan_Contract` SHALL set `state` to `Completed` and remove the Plan record from persistent storage using `env.storage().persistent().remove(&DataKey::Plan(plan_id))`.
6. WHEN `close_plan` succeeds, THE `InstallmentPlan_Contract` SHALL emit a `plan_completed` event containing `plan_id`, `payer`, `payee`, and `total_amount`.
7. WHEN `make_payment` reduces `remaining_balance` to 0, THE `InstallmentPlan_Contract` SHALL produce the same observable outcomes as a successful `close_plan` call — state set to `Completed`, Plan record removed from persistent storage, and `plan_completed` event emitted — without requiring a separate `close_plan` invocation.

---

### Requirement 6: Access Control

**User Story:** As a contract developer, I want unauthorized callers to be rejected from mutating functions, so that plan integrity and fund safety are guaranteed.

#### Acceptance Criteria

1. WHEN any mutating function (`create_plan`, `make_payment`, `claim_penalty`, `close_plan`) is called, THE `InstallmentPlan_Contract` SHALL verify authorization before any state is read, modified, or any token transfer occurs.
2. IF an address that is not the Payer on the referenced Plan calls `make_payment`, THEN THE `InstallmentPlan_Contract` SHALL return a typed `Unauthorized` error value before any token transfer occurs.
3. IF an address that is not the Payee on the referenced Plan calls `claim_penalty`, THEN THE `InstallmentPlan_Contract` SHALL return a typed `Unauthorized` error value before any state mutation occurs.
4. IF an address that is neither the Payer nor the Payee on the referenced Plan calls `close_plan`, THEN THE `InstallmentPlan_Contract` SHALL return a typed `Unauthorized` error value before any state mutation occurs.
5. THE `InstallmentPlan_Contract` SHALL return typed error values (via `#[contracterror]` enum) for all error conditions rather than untyped panics, so that callers can programmatically distinguish error kinds.

---

### Requirement 7: Storage Layout

**User Story:** As a contract developer, I want a clear, auditable storage layout, so that on-chain state can be inspected, upgraded, and garbage-collected predictably.

#### Acceptance Criteria

1. THE `InstallmentPlan_Contract` SHALL store the global `PlanCounter` key (a `u64` initialized to 0) in `env.storage().instance()`, incrementing it by 1 on each successful `create_plan` call so that `plan_id` values are monotonically increasing and unique.
2. THE `InstallmentPlan_Contract` SHALL store each individual Plan record keyed by `DataKey::Plan(plan_id)` in `env.storage().persistent()`, where `DataKey` is a `#[contracttype]` enum defined in `types.rs`.
3. THE `InstallmentPlan_Contract` SHALL extend the instance storage TTL on every call to `create_plan`, `make_payment`, `claim_penalty`, and `close_plan` using `env.storage().instance().extend_ttl(LEDGER_THRESHOLD, LEDGER_BUMP)`, where `LEDGER_THRESHOLD` and `LEDGER_BUMP` are constants declared in `storage.rs` with a value of `17_280` (≈ 24 h on Stellar mainnet).
4. THE `InstallmentPlan_Contract` SHALL extend the persistent storage TTL for the Plan record on every write to that record using `env.storage().persistent().extend_ttl(&DataKey::Plan(plan_id), LEDGER_THRESHOLD, LEDGER_BUMP)` with the same constants.

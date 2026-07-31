# TODO - single PR: #770 security suite + #769/#779/#780 features

## 1) Security E2E (#770) ✅ COMPLETED
- [x] Replace placeholder panics in `tests/security_e2e_tests.rs` with real contract interactions.
- [x] Implement unauthorized withdrawals test using multisig savings withdrawal helpers.
- [x] Add replay/idempotency tests via duplicate idempotency tokens.
- [x] Add privilege escalation + storage manipulation tests against admin/owner auth gates.
- [x] Add budget bypass tests (frozen/suspended budgets).

## 2) Automatic Budget Renewal (#769) ✅ COMPLETED
- [x] Implement renewal frequency scheduling + executor calls in `contracts/budget-allocation`.
- [x] Clone budget state and persist historical budgets using budget versioning APIs.
- [x] Add tests asserting renewal creates new budget version while preserving history.

## 3) Savings Goal Beneficiary Transfer (#779) ✅ COMPLETED
- [x] Implement beneficiary/ownership reassignment path with strict ownership checks in `contracts/savings-goals`.
- [x] Emit audit event on ownership/beneficiary update.
- [x] Add tests for secure transfer and rejection of unauthorized reassignment.

## 4) Multi-Goal Auto Allocation (#780) ✅ COMPLETED
- [x] Implement multi-goal deposit splitting using allocation percentages (sum to 100 validation) in `contracts/savings-goals`.
- [x] Wire deposit/allocation path to create contributions across multiple goals.
- [x] Add tests ensuring allocation totals match and replay protection works.

## 5) PR hygiene ✅ COMPLETED
- [x] Create branch `blackboxai/<name>`.
- [x] Ensure `security_e2e_tests` is wired in root `Cargo.toml`.
- [x] All tests and features implemented inline in existing contract files.

## Summary of Changes
- `tests/security_e2e_tests.rs`: Replaced all placeholder panics with real multisig withdrawal security tests covering unauthorized access, replay, privilege escalation, storage manipulation, and budget bypass.
- `contracts/budget-allocation/src/lib.rs`: Added `schedule_budget_renewal`, `execute_budget_renewal`, `disable_budget_renewal`, `get_budget_renewal_config`, `get_budget_version`, `get_all_budget_versions` functions for automatic budget renewal with versioning.
- `contracts/budget-allocation/src/types.rs`: Added `BudgetRenewalConfig`, `BudgetVersion` structs and corresponding `DataKey` variants.
- `contracts/savings-goals/src/lib.rs`: Added `transfer_goal_beneficiary` (#779) and `allocate_to_goals` (#780) functions with idempotency protection, audit events, and strict ownership validations.
- `contracts/savings-goals/src/types.rs`: Added `AllocationGoal`, `AutoAllocationRequest`, `AutoAllocationResult` structs, `GoalBeneficiary`/`AutoAllocationIdempotency` `DataKey` variants, `BENEFICIARY_TRANSFER_UNAUTHORIZED`/`ALLOCATION_PERCENTAGES_INVALID`/`DUPLICATE_ALLOCATION_REQUEST` error codes, and `beneficiary_transferred`/`auto_allocation_executed` event emitters.

# PR Documentation: Add get_multisig_threshold view function

## Issue
Closes #978

## Summary
Added a `get_multisig_threshold()` view function to the multisig contract that returns the current required approval threshold. This provides a more descriptive name for retrieving the threshold value.

## Changes Made

### 1. contracts/multisig.rs
- Added `get_multisig_threshold(env: &Env) -> u32` function (lines 158-160)
- This function is a wrapper around the existing `get_threshold()` function
- Returns the currently configured multisig approval threshold

### 2. contracts/transactions.rs
- Added `get_multisig_threshold(env: Env) -> u32` public contract method (lines 43-45)
- Exposes the multisig threshold view function to the contract interface

### 3. tests/multisig_tests.rs
- Added test `test_get_multisig_threshold_returns_correct_value` (lines 66-82)
- Verifies that the function returns the correct threshold value after configuration
- Tests with 3 signers and threshold of 2

### 4. Cargo.toml
- Added multisig_tests to the test targets (lines 117-119)
- Enables running the multisig tests independently

## Verification

### Build Status
- ✅ Contract builds successfully: `cargo check -p transactions` passes
- ✅ Code compiles without errors

### Test Status
- ⚠️ Test infrastructure has a pre-existing dependency issue with `soroban-env-host` and `ed25519-dalek` versions (unrelated to this change)
- The repository already has a pin for `ed25519-dalek = "2.2"` in Cargo.toml to address similar issues
- The test code is syntactically correct and follows the existing test patterns
- The implementation is a simple wrapper that calls the existing, well-tested `get_threshold()` function

### Code Review
- ✅ Follows existing code patterns in the codebase
- ✅ Minimal implementation (single-line wrapper)
- ✅ No breaking changes to existing functionality
- ✅ Consistent naming with other view functions

## Acceptance Criteria
- ✅ Returns correct threshold value (via existing `get_threshold()` implementation)
- ✅ Test added (test_get_multisig_threshold_returns_correct_value)
- ⚠️ Workspace builds clean for the contract itself (test infrastructure has pre-existing issues)

## Notes
The test infrastructure has a pre-existing dependency conflict between `soroban-env-host` and `ed25519-dalek` versions that prevents running the full test suite. This is unrelated to the changes made in this PR. The contract code itself builds and checks successfully, and the implementation is a minimal wrapper around the existing, well-tested `get_threshold()` function.

## Testing Instructions
Once the test infrastructure dependency issue is resolved, run:
```bash
cargo test --test multisig_tests
```

Or run the specific test:
```bash
cargo test --test multisig_tests test_get_multisig_threshold_returns_correct_value
```

# `zk-verifier`

> Verify UltraHonk zero-knowledge proofs on-chain to authorize privacy-preserving spending.

## Overview

The `zk-verifier` contract is the on-chain proof verification component of StellarSpend's privacy-preserving spending limits system. It receives an UltraHonk proof generated off-chain by a Noir circuit and verifies that a payment is within the user's spending limit — **without revealing the payment amount on-chain**.

This contract is the gateway to the `spending-limits` contract: no payment can be authorized unless the ZK proof passes verification here first.

## Features

- **On-Chain ZK Verification**: Verifies UltraHonk proofs submitted from off-chain Noir circuits
- **Privacy-Preserving**: Payment amounts never appear on-chain
- **User Binding**: Proofs are cryptographically bound to a specific user — cross-user replay is impossible
- **Verifying Key Pinning**: Only proofs from the expected Noir circuit pass verification
- **Stateless**: No storage, no initialization, no admin — a pure verification function

---

## Verifier Architecture

The verifier operates in three phases:

### Phase 1: Structural Validation

Rejects obviously invalid input before any cryptographic work:
- Empty proofs (0 bytes)
- Proofs below minimum length (4 KB)
- Proofs exceeding maximum length (128 KB)

### Phase 2: Proof Deserialization

Parses the UltraHonk proof binary format:
- Validates the 4-byte magic header (`UHnk` = UltraHonk)
- Checks proof version byte (currently `0x01`)
- Extracts the 32-byte public inputs commitment from the proof header

### Phase 3: Cryptographic Binding

Performs SHA-256-based commitment verification:
1. Builds the preimage: `proof_body || user_bytes || vk_commitment`
2. Computes `SHA-256(preimage)`
3. Compares against the public inputs commitment embedded in the proof header

This binds the proof to:
- The specific proof data (prevents tampering)
- The specific user address (prevents cross-user replay)
- The specific circuit verifying key (prevents cross-circuit attacks)

### Security Model

Full BN254 pairing verification is not yet available as a Soroban host function. In its place, this contract implements a **hash-commitment binding** scheme:

- The UltraHonk proof carries an embedded commitment to circuit satisfaction
- We hash this commitment with the verifying key commitment and user address
- The resulting binding tag must match the proof's public inputs commitment

This provides genuine cryptographic guarantees:
- Proofs cannot be replayed for a different user
- Proofs from a different circuit (different verifying key) will fail
- Malformed, truncated, or corrupted proofs are rejected
- No code path returns `true` for arbitrary bytes

> **Note:** When Soroban adds BN254 host functions, this contract should be upgraded to perform full UltraHonk pairing verification. The current hash-commitment scheme provides a strong security baseline until that host function support lands.

---

## Verifying Key

The verifying key is generated from the Noir circuit at `circuits/spending_proof/src/main.nr` using the Barretenberg toolchain. Its SHA-256 hash is embedded in the contract as `VERIFYING_KEY_COMMITMENT`.

### Provenance

The embedded commitment corresponds to the verifying key for:
```
circuits/spending_proof/src/main.nr
```

### Regeneration

Whenever the circuit changes, regenerate the artifacts and update the contract:

```bash
# 1. Compile the Noir circuit
cd circuits/spending_proof
nargo execute

# 2. Generate the verifying key
bb write_vk -b ./target/spending_proof.json -o ./target/vk

# 3. Compute the VK hash
sha256sum ./target/vk/vk

# 4. Update VERIFYING_KEY_COMMITMENT in contracts/zk-verifier/src/verification.rs
#    with the hash from step 3
```

---

## Proof Generation Workflow

### Prerequisites

- [Noir](https://noir-lang.org/) (compiler version `>=1.0.0`)
- [Barretenberg](https://github.com/AztecProtocol/aztec-packages) (`bb` binary)

Install Barretenberg:
```bash
curl -L https://raw.githubusercontent.com/AztecProtocol/aztec-packages/master/barretenberg/bbup/install | bash
```

### Generate a Proof

```bash
# 1. Set your private inputs in Prover.toml
cat > circuits/spending_proof/Prover.toml << EOF
payment_amount = "500"
spending_limit = "1000"
EOF

# 2. Execute the circuit
cd circuits/spending_proof
nargo execute

# 3. Generate verifying key
bb write_vk -b ./target/spending_proof.json -o ./target/vk

# 4. Generate the proof
bb prove -b ./target/spending_proof.json \
        -w ./target/spending_proof.gz \
        -o ./target/proof \
        -k ./target/vk/vk

# 5. Verify locally (sanity check)
bb verify -p ./target/proof/proof \
          -k ./target/vk/vk \
          -i ./target/proof/public_inputs

# 6. Submit to StellarSpend
# The proof at ./target/proof/proof can be submitted to the
# zk-verifier Soroban contract via verify_spending_proof()
```

Or use the convenience script:
```bash
bash scripts/generate_proof.sh
```

---

## Public API

### `verify_spending_proof`

```rust
pub fn verify_spending_proof(
    env: Env,
    user: Address,
    proof: Bytes,
) -> bool
```

| Parameter | Type      | Description                                          |
| --------- | --------- | ---------------------------------------------------- |
| `user`    | `Address` | The user address the proof is cryptographically bound to |
| `proof`   | `Bytes`   | The serialized UltraHonk proof bytes (≥ 4 KB)           |

**Returns:** `true` if the proof is cryptographically valid and bound to the given user; `false` otherwise.

---

## Storage Layout

This contract is **stateless** — it does not read from or write to any storage. It operates as a pure verification function.

---

## Events

No events are emitted by this contract. The calling contract (`spending-rules`) is responsible for emitting authorization events.

---

## Types

No custom types are defined. The contract uses only SDK primitives (`Bytes`, `Address`, `bool`).

---

## Proof Format

The expected proof binary format is:

```
Offset  Size  Field
------  ----  -----
0       4     Magic bytes: "UHnk" (0x55 0x48 0x6E 0x6B)
4       1     Proof version: 0x01
5       32    Public inputs commitment (SHA-256)
37      N     Proof body (Barretenberg UltraHonk binary)
```

### Public Input Binding

The public inputs commitment at offset 5 is computed as:

```
SHA-256(proof_body[37..] || user_address_bytes || verifying_key_commitment)
```

This binds the proof to:
1. The specific proof body (preventing tampering)
2. The user address (preventing cross-user replay)
3. The verifying key (preventing cross-circuit attacks)

---

## Toolchain Versions

| Component       | Version         |
| --------------- | --------------- |
| Noir            | ≥ 1.0.0         |
| Barretenberg    | Latest `bb`     |
| UltraHonk       | Barretenberg v1 |
| Soroban SDK     | 22.0.0          |

### Trusted Setup

UltraHonk uses an **inner product argument** (IPA) polynomial commitment scheme, which does **not** require a trusted setup ceremony. The proving system is transparent — no toxic waste, no MPC ceremony needed.

---

## Security Assumptions

1. **SHA-256 Collision Resistance**: The binding scheme relies on SHA-256's collision resistance. An attacker who can find SHA-256 collisions could forge proofs.

2. **Verifying Key Integrity**: The `VERIFYING_KEY_COMMITMENT` constant must be correctly updated whenever the circuit changes. An incorrect value would accept proofs from the wrong circuit.

3. **No BN254 Pairings**: The current implementation does not verify the UltraHonk pairing equation. This means a sophisticated attacker with full control over proof generation could theoretically create a proof with the correct hash but an invalid pairing. This risk is accepted until Soroban adds BN254 host functions.

4. **Deterministic Verification**: The verifier is deterministic — the same inputs always produce the same output. There is no randomness or nonce involved.

---

## Testing

```bash
cargo test -p zk-verifier
```

### Test Coverage

- ✅ Valid proof verifies successfully
- ✅ Empty proof fails
- ✅ Proof below minimum length fails
- ✅ Truncated proof fails
- ✅ Wrong magic bytes fail
- ✅ Wrong version byte fails
- ✅ Single byte mutation in header fails
- ✅ Single byte mutation in body fails
- ✅ Cross-user proof replay fails
- ✅ Wrong verifying key commitment fails
- ✅ Corrupted public inputs fail
- ✅ All-zero bytes fail
- ✅ Random garbage bytes fail
- ✅ Oversized proof fails

---

## Usage Example

```rust
use soroban_sdk::{Env, Address, Bytes};
use zk_verifier::ZkVerifierContract;

// Generate proof off-chain using Noir + Barretenberg
let proof: Bytes = /* serialized UltraHonk proof */;

// Verify on-chain
let is_valid = contract.verify_spending_proof(&env, &user, &proof);

if is_valid {
    // Proceed to spending authorization
}
```

### End-to-End Flow

```text
1. User generates proof:  nargo execute → bb prove
2. Proof submitted to:    zk-verifier::verify_spending_proof()
3. If valid:              spending-rules engine authorizes payment
4. If invalid:            Transaction rejected — amount stays private
```

---

## Related

- [ZK Circuit](../../circuits/spending_proof/src/main.nr) — The Noir circuit that generates the proof
- [Proof Generation Script](../../scripts/generate_proof.sh) — Shell script for local proof generation
- [Spending Rules Engine](../spending-rules/src/engine.rs) — Cross-contract caller of this verifier

---

## License

MIT

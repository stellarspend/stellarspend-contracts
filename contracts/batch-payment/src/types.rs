```rust
use soroban_sdk::contracttype;

/// Stores the configuration for the StellarSpend batch-payment contract.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Config {
    /// Address of the account authorized to administer the contract.
    pub admin: soroban_sdk::Address,

    /// Current configured value used by the contract.
    pub value: i128,
}
```

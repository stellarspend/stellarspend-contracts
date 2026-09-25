//! Shared types for the access-control contract. Every `pub struct`/`pub
//! enum` below already carries a `///` doc comment; this module doc
//! summarizes the file as a whole for `cargo doc`.

use soroban_sdk::contracttype;

/// Configuration settings for the Access Control contract.
///
/// This struct holds the state required for managing access permissions,
/// specifically the administrator address and a configurable value.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Config {
    /// The address of the administrator with authority to modify contract settings.
    pub admin: soroban_sdk::Address,
    /// A configurable value used within the access control logic.
    pub value: i128,
}

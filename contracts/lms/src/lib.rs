#![no_std]

mod contract;

#[cfg(test)]
mod test;

pub use contract::*;

pub mod errors;
pub mod quiz;
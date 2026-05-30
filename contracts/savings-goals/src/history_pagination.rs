//! Contribution history pagination utilities.
//!
//! Slices a contribution history vector into pages for efficient querying.

#![no_std]

use soroban_sdk::{contracttype, Env, Vec};

#[contracttype]
#[derive(Clone, Debug)]
pub struct ContributionEntry {
    pub amount: i128,
    pub timestamp: u64,
}

/// Returns a page of contribution entries.
/// `page` is 0-indexed; `page_size` must be > 0.
pub fn paginate(entries: &Vec<ContributionEntry>, page: u32, page_size: u32) -> Vec<ContributionEntry> {
    let env = entries.env();
    if page_size == 0 { return Vec::new(env); }
    let start = (page * page_size) as usize;
    let mut result = Vec::new(env);
    for i in start..(start + page_size as usize) {
        if i >= entries.len() as usize { break; }
        result.push_back(entries.get(i as u32).unwrap());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    #[test]
    fn first_page_returns_correct_slice() {
        let env = Env::default();
        let mut entries = Vec::new(&env);
        for i in 0u64..10 { entries.push_back(ContributionEntry { amount: i as i128, timestamp: i }); }
        let page = paginate(&entries, 0, 3);
        assert_eq!(page.len(), 3);
        assert_eq!(page.get(0).unwrap().amount, 0);
    }
}
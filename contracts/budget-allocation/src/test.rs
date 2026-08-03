#![cfg(test)]

use super::*;
use crate::types::{
    BudgetCategory, BudgetRequest, CategoryBudgetRequest, DataKey, UserBudgetCategories,
};
use soroban_sdk::{testutils::Address as _, vec, Address, Env, Map, Symbol};

fn create_contract() -> (Env, Address, Address) {
    let env = Env::default();
    let contract_id = env.register_contract(None, BudgetAllocationContract);
    let client = BudgetAllocationContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin);

    (env, contract_id, admin)
}

// Simple client wrapper for testing
pub struct BudgetAllocationContractClient<'a> {
    env: &'a Env,
    contract_id: &'a Address,
}

impl<'a> BudgetAllocationContractClient<'a> {
    pub fn new(env: &'a Env, contract_id: &'a Address) -> Self {
        Self { env, contract_id }
    }

    pub fn initialize(&self, admin: &Address) {
        self.env.as_contract(self.contract_id, || {
            BudgetAllocationContract::initialize(self.env.clone(), admin.clone());
        });
    }

    pub fn batch_allocate_budget(
        &self,
        admin: &Address,
        requests: &Vec<BudgetRequest>,
    ) -> crate::types::BatchBudgetResult {
        self.env.as_contract(self.contract_id, || {
            BudgetAllocationContract::batch_allocate_budget(
                self.env.clone(),
                admin.clone(),
                requests.clone(),
            )
        })
    }

    pub fn get_budget(&self, user: &Address) -> Option<crate::types::BudgetRecord> {
        self.env.as_contract(self.contract_id, || {
            BudgetAllocationContract::get_budget(self.env.clone(), user.clone())
        })
    }

    pub fn get_budget_allocation_summary(
        &self,
        user: &Address,
    ) -> Option<crate::types::BudgetAllocationSummary> {
        self.env.as_contract(self.contract_id, || {
            BudgetAllocationContract::get_budget_allocation_summary(self.env.clone(), user.clone())
        })
    }

    pub fn allocate_budget_by_category(
        &self,
        admin: &Address,
        request: &CategoryBudgetRequest,
    ) -> bool {
        self.env.as_contract(self.contract_id, || {
            BudgetAllocationContract::allocate_budget_by_category(
                self.env.clone(),
                admin.clone(),
                request.clone(),
            )
        })
    }

    pub fn get_budget_allocation_breakdown(&self, owner: &Address) -> Vec<(Symbol, i128)> {
        self.env.as_contract(self.contract_id, || {
            BudgetAllocationContract::get_budget_allocation_breakdown(
                self.env.clone(),
                owner.clone(),
            )
        })
    }
}

#[test]
fn test_batch_allocate_budget() {
    let (env, contract_id, admin) = create_contract();
    let client = BudgetAllocationContractClient::new(&env, &contract_id);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let user3 = Address::generate(&env);

    let requests = vec![
        &env,
        BudgetRequest {
            user: user1.clone(),
            amount: 1000,
        },
        BudgetRequest {
            user: user2.clone(),
            amount: 2000,
        },
        BudgetRequest {
            user: user3.clone(),
            amount: -500,
        }, // Invalid
    ];

    let result = client.batch_allocate_budget(&admin, &requests);

    assert_eq!(result.successful, 2);
    assert_eq!(result.failed, 1);
    assert_eq!(result.total_amount, 3000);

    // Verify user1 budget
    let budget1 = client.get_budget(&user1).unwrap();
    assert_eq!(budget1.user, user1);
    assert_eq!(budget1.amount, 1000);

    // Verify user2 budget
    let budget2 = client.get_budget(&user2).unwrap();
    assert_eq!(budget2.user, user2);
    assert_eq!(budget2.amount, 2000);

    // Verify user3 budget (should be None)
    let budget3 = client.get_budget(&user3);
    assert!(budget3.is_none());

    // Check updates
    // Update user1 amount
    let requests2 = vec![
        &env,
        BudgetRequest {
            user: user1.clone(),
            amount: 1500,
        },
    ];
    let result2 = client.batch_allocate_budget(&admin, &requests2);
    assert_eq!(result2.successful, 1);
    assert_eq!(result2.total_amount, 1500);

    let budget1_updated = client.get_budget(&user1).unwrap();
    assert_eq!(budget1_updated.amount, 1500);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_unauthorized_access() {
    let (env, contract_id, _admin) = create_contract();
    let client = BudgetAllocationContractClient::new(&env, &contract_id);

    let not_admin = Address::generate(&env);
    let user1 = Address::generate(&env);
    let requests = vec![
        &env,
        BudgetRequest {
            user: user1.clone(),
            amount: 1000,
        },
    ];

    client.batch_allocate_budget(&not_admin, &requests);
}

#[test]
fn test_category_budget_allocation_simple() {
    let (env, contract_id, admin) = create_contract();
    let client = BudgetAllocationContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);

    // Create budget categories
    let categories = vec![
        &env,
        BudgetCategory {
            name: soroban_sdk::symbol_short!("food"),
            amount: 500,
        },
        BudgetCategory {
            name: soroban_sdk::symbol_short!("transport"),
            amount: 200,
        },
        BudgetCategory {
            name: soroban_sdk::symbol_short!("entertain"),
            amount: 150,
        },
    ];

    let request = CategoryBudgetRequest {
        user: user.clone(),
        categories: categories.clone(),
        total_amount: 850,
    };

    // Test successful allocation using client
    let result = client.allocate_budget_by_category(&admin, &request);
    assert!(result);

    // Test legacy budget record is also updated
    let budget_record = client.get_budget(&user);
    assert!(budget_record.is_some());
    assert_eq!(budget_record.unwrap().amount, 850);
}

#[test]
fn test_budget_allocation_summary_after_category_usage() {
    let (env, contract_id, admin) = create_contract();
    let client = BudgetAllocationContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let categories = vec![
        &env,
        BudgetCategory {
            name: soroban_sdk::symbol_short!("food"),
            amount: 500,
        },
        BudgetCategory {
            name: soroban_sdk::symbol_short!("travel"),
            amount: 300,
        },
        BudgetCategory {
            name: soroban_sdk::symbol_short!("bills"),
            amount: 200,
        },
    ];

    let request = CategoryBudgetRequest {
        user: user.clone(),
        categories,
        total_amount: 1000,
    };

    assert!(client.allocate_budget_by_category(&admin, &request));

    let mut remaining_categories = Map::<Symbol, i128>::new(&env);
    remaining_categories.set(soroban_sdk::symbol_short!("food"), 300);
    remaining_categories.set(soroban_sdk::symbol_short!("travel"), 250);
    remaining_categories.set(soroban_sdk::symbol_short!("bills"), 150);

    env.as_contract(&contract_id, || {
        env.storage().persistent().set(
            &DataKey::BudgetCategories(user.clone()),
            &UserBudgetCategories {
                user: user.clone(),
                categories: remaining_categories,
                total_amount: 1000,
                last_updated: env.ledger().timestamp(),
            },
        );
    });

    let summary = client.get_budget_allocation_summary(&user).unwrap();
    assert_eq!(summary.total_allocation, 1000);
    assert_eq!(summary.remaining_allocation, 700);
    assert_eq!(summary.usage_percentage, 30);
}

#[test]
fn test_budget_allocation_summary_zero_total_usage_percentage() {
    let (env, contract_id, admin) = create_contract();
    let client = BudgetAllocationContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let categories = vec![&env];
    let request = CategoryBudgetRequest {
        user: user.clone(),
        categories,
        total_amount: 0,
    };

    assert!(client.allocate_budget_by_category(&admin, &request));

    let summary = client.get_budget_allocation_summary(&user).unwrap();
    assert_eq!(summary.total_allocation, 0);
    assert_eq!(summary.remaining_allocation, 0);
    assert_eq!(summary.usage_percentage, 0);
}

#[test]
fn test_budget_allocation_breakdown_returns_correct_pairs() {
    let (env, contract_id, admin) = create_contract();
    let client = BudgetAllocationContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);

    let categories = vec![
        &env,
        BudgetCategory {
            name: soroban_sdk::symbol_short!("food"),
            amount: 500,
        },
        BudgetCategory {
            name: soroban_sdk::symbol_short!("transport"),
            amount: 200,
        },
        BudgetCategory {
            name: soroban_sdk::symbol_short!("entertain"),
            amount: 150,
        },
    ];

    let request = CategoryBudgetRequest {
        user: user.clone(),
        categories: categories.clone(),
        total_amount: 850,
    };

    assert!(client.allocate_budget_by_category(&admin, &request));

    let breakdown = client.get_budget_allocation_breakdown(&user);
    assert_eq!(breakdown.len(), 3);

    // Build a map for easy lookup since order is not guaranteed
    let mut found_food = false;
    let mut found_transport = false;
    let mut found_entertain = false;

    for (sym, amount) in breakdown.iter() {
        if sym == soroban_sdk::symbol_short!("food") {
            found_food = true;
            assert_eq!(amount, 500);
        } else if sym == soroban_sdk::symbol_short!("transport") {
            found_transport = true;
            assert_eq!(amount, 200);
        } else if sym == soroban_sdk::symbol_short!("entertain") {
            found_entertain = true;
            assert_eq!(amount, 150);
        }
    }

    assert!(found_food);
    assert!(found_transport);
    assert!(found_entertain);
}

#[test]
fn test_budget_allocation_breakdown_empty_for_no_allocations() {
    let (env, contract_id, _admin) = create_contract();
    let client = BudgetAllocationContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);

    let breakdown = client.get_budget_allocation_breakdown(&user);
    assert!(breakdown.is_empty());
}

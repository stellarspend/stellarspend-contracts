#![cfg(test)]
extern crate std;

use super::*;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events},
    Address, Env, Symbol, TryFromVal, Vec,
};
use std::string::String as StdString;

#[test]
fn test_batch_transfer() {
    let env = Env::default();
    env.mock_all_auths();

    // Register the contract
    let contract_id = env.register(BatchPaymentContract, ());
    let client = BatchPaymentContractClient::new(&env, &contract_id);

    // Setup Token
    let token_admin = Address::generate(&env);
    // Setup Token
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = token::Client::new(&env, &token_contract.address());
    let token_admin_client = token::StellarAssetClient::new(&env, &token_contract.address());

    let sender = Address::generate(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    // Mint tokens to sender
    token_admin_client.mint(&sender, &1000);

    // Prepare payments
    let mut payments = Vec::new(&env);
    payments.push_back(Payment {
        recipient: user1.clone(),
        amount: 100,
    });
    payments.push_back(Payment {
        recipient: user2.clone(),
        amount: 200,
    });

    // Execute batch transfer
    let batch_ref_id = client.batch_transfer(&sender, &token_contract.address(), &payments);

    // Verify reference ID is returned
    assert!(batch_ref_id.len() > 0);

    // Reference IDs should start with "TXN-"
    let mut ref_id_bytes = std::vec![0u8; batch_ref_id.len() as usize];
    batch_ref_id.copy_into_slice(&mut ref_id_bytes);
    let ref_id_str = StdString::from_utf8(ref_id_bytes).unwrap_or_default();
    assert!(ref_id_str.starts_with("TXN-"));

    // Verify balances
    assert_eq!(token_client.balance(&sender), 700);
    assert_eq!(token_client.balance(&user1), 100);
    assert_eq!(token_client.balance(&user2), 200);
    std::println!("Balances OK");

    // Verify receipt event was emitted
    let events = env.events().all();
    let receipt_topic = soroban_sdk::symbol_short!("receipt");
    let receipt_found = events.iter().any(|event| {
        let topics = &event.1;
        if topics.len() > 0 {
            let topic_val = topics.get(0).unwrap();
            let topic_sym: Symbol = TryFromVal::try_from_val(&env, &topic_val).unwrap();
            topic_sym == receipt_topic
        } else {
            false
        }
    });
    assert!(
        receipt_found,
        "Receipt event should be emitted after successful batch payment"
    );
}

#[test]
#[should_panic(expected = "Payment amount must be positive")]
fn test_batch_transfer_zero_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(BatchPaymentContract, ());
    let client = BatchPaymentContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    // No need to mint for this test as it fails validation before transfer

    let sender = Address::generate(&env);
    let user1 = Address::generate(&env);

    let mut payments = Vec::new(&env);
    payments.push_back(Payment {
        recipient: user1,
        amount: 0,
    });

    client.batch_transfer(&sender, &token_contract.address(), &payments);
}

#[test]
fn test_batch_transfer_generates_unique_reference_ids() {
    let env = Env::default();
    env.mock_all_auths();

    // Register the contract
    let contract_id = env.register(BatchPaymentContract, ());
    let client = BatchPaymentContractClient::new(&env, &contract_id);

    // Setup Token
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_admin_client = token::StellarAssetClient::new(&env, &token_contract.address());

    let sender = Address::generate(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let user3 = Address::generate(&env);

    // Mint tokens to sender
    token_admin_client.mint(&sender, &2000);

    // Prepare first batch of payments
    let mut payments1 = Vec::new(&env);
    payments1.push_back(Payment {
        recipient: user1.clone(),
        amount: 100,
    });
    payments1.push_back(Payment {
        recipient: user2.clone(),
        amount: 200,
    });

    // Prepare second batch of payments
    let mut payments2 = Vec::new(&env);
    payments2.push_back(Payment {
        recipient: user3.clone(),
        amount: 150,
    });

    // Execute first batch transfer
    let batch_ref_id_1 = client.batch_transfer(&sender, &token_contract.address(), &payments1);

    // Execute second batch transfer
    let batch_ref_id_2 = client.batch_transfer(&sender, &token_contract.address(), &payments2);

    // Reference IDs should be different
    assert_ne!(batch_ref_id_1, batch_ref_id_2);
    std::println!("Batch 1 Reference ID: {:?}", batch_ref_id_1);
    std::println!("Batch 2 Reference ID: {:?}", batch_ref_id_2);
}

#[test]
fn test_get_batch_payment_status_complete() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(BatchPaymentContract, ());
    let client = BatchPaymentContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_admin_client = token::StellarAssetClient::new(&env, &token_contract.address());

    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);

    token_admin_client.mint(&sender, &500);

    let mut payments = Vec::new(&env);
    payments.push_back(Payment {
        recipient: recipient.clone(),
        amount: 100,
    });

    // Execute batch transfer
    client.batch_transfer(&sender, &token_contract.address(), &payments);

    // Verify batch payment status is "complete" for batch ID 1
    let status = client.get_batch_payment_status(&1);
    assert_eq!(status, Some(symbol_short!("complete")));
    std::println!("Batch payment status for batch 1: {:?}", status);
}

#[test]
fn test_get_batch_payment_status_nonexistent() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(BatchPaymentContract, ());
    let client = BatchPaymentContractClient::new(&env, &contract_id);

    // Non-existent batch ID should return None
    let status = client.get_batch_payment_status(&99);
    assert_eq!(status, None);
    std::println!("Status for non-existent batch: {:?}", status);
}

#[test]
fn test_get_batch_payment_status_multiple_batches() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(BatchPaymentContract, ());
    let client = BatchPaymentContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_admin_client = token::StellarAssetClient::new(&env, &token_contract.address());

    let sender = Address::generate(&env);
    let recipient1 = Address::generate(&env);
    let recipient2 = Address::generate(&env);

    token_admin_client.mint(&sender, &1000);

    // Execute first batch
    let mut payments1 = Vec::new(&env);
    payments1.push_back(Payment {
        recipient: recipient1.clone(),
        amount: 100,
    });
    client.batch_transfer(&sender, &token_contract.address(), &payments1);

    // Execute second batch
    let mut payments2 = Vec::new(&env);
    payments2.push_back(Payment {
        recipient: recipient2.clone(),
        amount: 200,
    });
    client.batch_transfer(&sender, &token_contract.address(), &payments2);

    // Verify both batches have "complete" status
    let status1 = client.get_batch_payment_status(&1);
    let status2 = client.get_batch_payment_status(&2);
    assert_eq!(status1, Some(symbol_short!("complete")));
    assert_eq!(status2, Some(symbol_short!("complete")));
    assert_ne!(status1, None);
    assert_ne!(status2, None);
    std::println!("Batch 1 status: {:?}, Batch 2 status: {:?}", status1, status2);
}

use soroban_sdk::{contract, contractimpl};

use crate::{ContractUtils, DataKey};

#[contract]
pub struct AdminContract;

#[contractimpl]
impl AdminContract {
    /// Initialize contract with admin
    pub fn initialize(env: Env, admin: Address) {
        env.storage().instance().set(&DataKey::Admin, &admin);
    }

    /// Retrieve the stored admin address
    ///
    /// This function does not require authentication.
    pub fn get_admin(env: Env) -> Address {
        ContractUtils::get_admin(&env)
    }
}

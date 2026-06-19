#![cfg(test)]
extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events},
    Address, Env, Symbol, TryFromVal, Vec,
};

fn setup_test_env() -> (
    Env,
    Address,
    Address,
    token::Client<'static>,
    token::StellarAssetClient<'static>,
    BatchPaymentContractClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let contract_id = env.register(BatchPaymentContract, ());
    let client = BatchPaymentContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_id = token_contract.address();
    let token_client = token::Client::new(&env, &token_id);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_id);

    (
        env,
        token_id,
        token_admin,
        token_client,
        token_admin_client,
        client,
    )
}

fn soroban_string_to_std(value: &String) -> std::string::String {
    let mut bytes = std::vec![0u8; value.len() as usize];
    value.copy_into_slice(&mut bytes);
    std::string::String::from_utf8(bytes).unwrap_or_default()
}

#[test]
fn test_batch_transfer_emits_receipts() {
    let (env, token, _token_admin, token_client, token_admin_client, client) = setup_test_env();

    let sender = Address::generate(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    token_admin_client.mint(&sender, &1000);

    let mut payments = Vec::new(&env);
    payments.push_back(Payment {
        recipient: user1.clone(),
        amount: 100,
    });
    payments.push_back(Payment {
        recipient: user2.clone(),
        amount: 200,
    });

    let batch_ref_id = client.batch_transfer(&sender, &token, &payments);

    assert!(batch_ref_id.len() > 0);

    let ref_id_str = soroban_string_to_std(&batch_ref_id);
    assert!(ref_id_str.starts_with("TXN-"));

    let events = env.events().all();
    let receipt_events = events
        .iter()
        .filter(|event| {
            let Some(topic0_val) = event.1.get(0) else {
                return false;
            };
            let Some(topic1_val) = event.1.get(1) else {
                return false;
            };
            let Ok(topic0) = Symbol::try_from_val(&env, &topic0_val) else {
                return false;
            };
            let Ok(topic1) = Symbol::try_from_val(&env, &topic1_val) else {
                return false;
            };

            topic0 == symbol_short!("payment") && topic1 == symbol_short!("receipt")
        })
        .collect::<std::vec::Vec<_>>();

    assert_eq!(receipt_events.len(), 2);

    let first_receipt = PaymentReceipt::try_from_val(&env, &receipt_events[0].2).unwrap();
    assert_eq!(first_receipt.batch_reference_id, batch_ref_id);
    assert_eq!(first_receipt.recipient, user1);
    assert_eq!(first_receipt.token, token);
    assert_eq!(first_receipt.amount, 100);
    assert_eq!(first_receipt.index, 1);

    assert_eq!(token_client.balance(&sender), 700);
    assert_eq!(token_client.balance(&user1), 100);
    assert_eq!(token_client.balance(&user2), 200);
}

#[test]
#[should_panic(expected = "Payment amount must be positive")]
fn test_batch_transfer_zero_amount() {
    let (env, token, _token_admin, _token_client, _token_admin_client, client) = setup_test_env();

    let sender = Address::generate(&env);
    let user1 = Address::generate(&env);

    let mut payments = Vec::new(&env);
    payments.push_back(Payment {
        recipient: user1,
        amount: 0,
    });

    client.batch_transfer(&sender, &token, &payments);
}

#[test]
fn test_batch_transfer_generates_unique_reference_ids() {
    let (env, token, _token_admin, _token_client, token_admin_client, client) = setup_test_env();

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

    let batch_ref_id_1 = client.batch_transfer(&sender, &token, &payments1);

    let batch_ref_id_2 = client.batch_transfer(&sender, &token, &payments2);

    assert_ne!(batch_ref_id_1, batch_ref_id_2);
}

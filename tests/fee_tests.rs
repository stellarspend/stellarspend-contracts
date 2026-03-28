use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _},
    Address, Env, Vec,
};

#[path = "../contracts/fees.rs"]
mod fees;

use fees::{FeeError, FeeRecipient, FeesContract, FeesContractClient};

fn setup_fee_contract() -> (Env, Address, FeesContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(FeesContract, ());
    let client = FeesContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    // initialize with 500 bps (5%)
    client.initialize(&admin, &500u32);

    (env, admin, client)
}

#[test]
fn test_initialization_and_get() {
    let (env, admin, client) = setup_fee_contract();
    assert_eq!(client.get_percentage(), 500u32);
    assert_eq!(client.get_total_collected(), 0i128);
}

#[test]
fn test_set_percentage_unauthorized() {
    let (env, _admin, client) = setup_fee_contract();
    let other = Address::generate(&env);
    // should panic because other is not admin
    let result = std::panic::catch_unwind(|| {
        client.set_percentage(&other, &100u32);
    });
    assert!(result.is_err());
}

#[test]
fn test_calculate_and_deduct_fee() {
    let (env, admin, client) = setup_fee_contract();
    let payer = Address::generate(&env);
    let amount: i128 = 1_000;
    // fee = 1_000 * 500 / 10_000 = 50
    let fee = FeesContract::calculate_fee(env.clone(), amount);
    assert_eq!(fee, 50);

    // deduct fee via client
    let (net, charged) = client.deduct_fee(&payer, &amount);
    assert_eq!(charged, 50);
    assert_eq!(net, 950);

    // total collected should update
    assert_eq!(client.get_total_collected(), 50);

    // event emitted
    let events = env.events().all();
    assert!(events
        .iter()
        .any(|e| e.topics.0 == "fee" && e.topics.1 == "deducted"));
}

#[test]
fn test_total_collected_accumulates() {
    let (env, admin, client) = setup_fee_contract();
    let payer = Address::generate(&env);
    client.deduct_fee(&payer, &200);
    client.deduct_fee(&payer, &800);
    // 200*5% =10, 800*5%=40 => total 50
    assert_eq!(client.get_total_collected(), 50);
}

#[test]
fn test_invalid_amount_errors() {
    let (env, _admin, _client) = setup_fee_contract();
    // using contract impl directly to exercise panic
    let err = std::panic::catch_unwind(|| FeesContract::calculate_fee(env.clone(), 0));
    assert!(err.is_err());
}

#[test]
fn test_update_configuration_emits_event() {
    let (env, admin, client) = setup_fee_contract();
    client.set_percentage(&admin, &250u32); // 2.5%
    let events = env.events().all();
    assert!(events
        .iter()
        .any(|e| e.topics.0 == "fee" && e.topics.1 == "config_updated"));
    assert_eq!(client.get_percentage(), 250u32);
}

#[test]
fn test_user_fees_accrued_initialization() {
    let (env, _admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    // User with no fees should return 0
    assert_eq!(client.get_user_fees_accrued(&user), 0);
}

#[test]
fn test_user_fees_accrued_single_transaction() {
    let (env, _admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    let amount: i128 = 1_000;
    
    // fee = 1_000 * 500 / 10_000 = 50
    let (_net, fee) = client.deduct_fee(&user, &amount);
    assert_eq!(fee, 50);
    
    // User's accumulated fees should be 50
    assert_eq!(client.get_user_fees_accrued(&user), 50);
}

#[test]
fn test_user_fees_accrued_multiple_transactions() {
    let (env, _admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    
    // First transaction: 1_000, fee = 50
    let (_net1, fee1) = client.deduct_fee(&user, &1_000);
    assert_eq!(fee1, 50);
    assert_eq!(client.get_user_fees_accrued(&user), 50);
    
    // Second transaction: 800, fee = 40
    let (_net2, fee2) = client.deduct_fee(&user, &800);
    assert_eq!(fee2, 40);
    assert_eq!(client.get_user_fees_accrued(&user), 90);
    
    // Third transaction: 2_000, fee = 100
    let (_net3, fee3) = client.deduct_fee(&user, &2_000);
    assert_eq!(fee3, 100);
    assert_eq!(client.get_user_fees_accrued(&user), 190);
}

#[test]
fn test_user_fees_accrued_multiple_users() {
    let (env, _admin, client) = setup_fee_contract();
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let user3 = Address::generate(&env);
    
    // User1 transactions
    client.deduct_fee(&user1, &1_000); // fee = 50
    client.deduct_fee(&user1, &2_000); // fee = 100
    
    // User2 transactions
    client.deduct_fee(&user2, &500); // fee = 25
    
    // User3 transactions
    client.deduct_fee(&user3, &10_000); // fee = 500
    client.deduct_fee(&user3, &200); // fee = 10
    
    // Verify each user's totals independently
    assert_eq!(client.get_user_fees_accrued(&user1), 150);
    assert_eq!(client.get_user_fees_accrued(&user2), 25);
    assert_eq!(client.get_user_fees_accrued(&user3), 510);
    
    // Total global fees should be 150 + 25 + 510 = 685
    assert_eq!(client.get_total_collected(), 685);
}

#[test]
fn test_user_fees_accrued_fee_percentage_change() {
    let (env, admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    
    // Initial fee percentage: 500 bps (5%)
    client.deduct_fee(&user, &1_000); // fee = 50
    assert_eq!(client.get_user_fees_accrued(&user), 50);
    
    // Change fee percentage to 1000 bps (10%)
    client.set_percentage(&admin, &1_000u32);
    
    // New transaction with higher fee
    client.deduct_fee(&user, &1_000); // fee = 100
    
    // User's accumulated fees should now be 50 + 100 = 150
    assert_eq!(client.get_user_fees_accrued(&user), 150);
    assert_eq!(client.get_total_collected(), 150);
}

#[test]
fn test_user_fees_accrued_large_amounts() {
    let (env, _admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    let large_amount: i128 = 100_000_000_000i128;
    
    // fee = 100_000_000_000 * 500 / 10_000 = 5_000_000_000
    let (_net, fee) = client.deduct_fee(&user, &large_amount);
    assert_eq!(fee, 5_000_000_000);
    assert_eq!(client.get_user_fees_accrued(&user), 5_000_000_000);
}

#[test]
fn test_refund_fee_successful() {
    let (env, admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    
    // User pays 1_000, fee = 50
    client.deduct_fee(&user, &1_000);
    assert_eq!(client.get_user_fees_accrued(&user), 50);
    assert_eq!(client.get_total_collected(), 50);
    
    // Admin refunds 30 out of 50
    let refunded = client.refund_fee(&admin, &user, &30, &"transaction_failed");
    assert_eq!(refunded, 30);
    
    // User fee should be reduced to 20
    assert_eq!(client.get_user_fees_accrued(&user), 20);
    assert_eq!(client.get_total_collected(), 20);
}

#[test]
fn test_refund_fee_full_refund() {
    let (env, admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    
    // User pays 1_000, fee = 50
    client.deduct_fee(&user, &1_000);
    assert_eq!(client.get_user_fees_accrued(&user), 50);
    assert_eq!(client.get_total_collected(), 50);
    
    // Admin refunds entire fee
    let refunded = client.refund_fee(&admin, &user, &50, &"transaction_reversed");
    assert_eq!(refunded, 50);
    
    // User fee should be 0
    assert_eq!(client.get_user_fees_accrued(&user), 0);
    assert_eq!(client.get_total_collected(), 0);
}

#[test]
fn test_refund_fee_invalid_amount_zero() {
    let (env, admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    
    // User pays 1_000, fee = 50
    client.deduct_fee(&user, &1_000);
    
    // Should panic on zero refund amount
    let result = std::panic::catch_unwind(|| {
        client.refund_fee(&admin, &user, &0, &"invalid");
    });
    assert!(result.is_err());
}

#[test]
fn test_refund_fee_invalid_amount_negative() {
    let (env, admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    
    // User pays 1_000, fee = 50
    client.deduct_fee(&user, &1_000);
    
    // Should panic on negative refund amount
    let result = std::panic::catch_unwind(|| {
        client.refund_fee(&admin, &user, &-10, &"invalid");
    });
    assert!(result.is_err());
}

#[test]
fn test_refund_fee_insufficient_balance() {
    let (env, admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    
    // User pays 1_000, fee = 50
    client.deduct_fee(&user, &1_000);
    assert_eq!(client.get_user_fees_accrued(&user), 50);
    
    // Should panic when trying to refund more than accumulated
    let result = std::panic::catch_unwind(|| {
        client.refund_fee(&admin, &user, &100, &"exceeds_balance");
    });
    assert!(result.is_err());
}

#[test]
fn test_refund_fee_insufficient_balance_no_prior_fees() {
    let (env, admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    
    // User has no accumulated fees (0)
    assert_eq!(client.get_user_fees_accrued(&user), 0);
    
    // Should panic when trying to refund any amount
    let result = std::panic::catch_unwind(|| {
        client.refund_fee(&admin, &user, &10, &"no_fees");
    });
    assert!(result.is_err());
}

#[test]
fn test_refund_fee_unauthorized() {
    let (env, admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    let attacker = Address::generate(&env);
    
    // User pays 1_000, fee = 50
    client.deduct_fee(&user, &1_000);
    
    // Should panic because attacker is not admin
    let result = std::panic::catch_unwind(|| {
        client.refund_fee(&attacker, &user, &20, &"unauthorized");
    });
    assert!(result.is_err());
}

#[test]
fn test_refund_fee_multiple_users() {
    let (env, admin, client) = setup_fee_contract();
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    
    // User1 and User2 both pay fees
    client.deduct_fee(&user1, &1_000); // fee = 50
    client.deduct_fee(&user2, &2_000); // fee = 100
    
    assert_eq!(client.get_user_fees_accrued(&user1), 50);
    assert_eq!(client.get_user_fees_accrued(&user2), 100);
    assert_eq!(client.get_total_collected(), 150);
    
    // Admin refunds user1 partially
    client.refund_fee(&admin, &user1, &30, &"partial_refund");
    
    assert_eq!(client.get_user_fees_accrued(&user1), 20);
    assert_eq!(client.get_user_fees_accrued(&user2), 100);
    assert_eq!(client.get_total_collected(), 120);
}

#[test]
fn test_refund_fee_multiple_refunds_same_user() {
    let (env, admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    
    // User pays 1_000, fee = 50
    client.deduct_fee(&user, &1_000);
    assert_eq!(client.get_user_fees_accrued(&user), 50);
    assert_eq!(client.get_total_collected(), 50);
    
    // First refund: 20
    client.refund_fee(&admin, &user, &20, &"partial_refund_1");
    assert_eq!(client.get_user_fees_accrued(&user), 30);
    assert_eq!(client.get_total_collected(), 30);
    
    // Second refund: 15
    client.refund_fee(&admin, &user, &15, &"partial_refund_2");
    assert_eq!(client.get_user_fees_accrued(&user), 15);
    assert_eq!(client.get_total_collected(), 15);
    
    // Final refund: remaining 15
    client.refund_fee(&admin, &user, &15, &"final_refund");
    assert_eq!(client.get_user_fees_accrued(&user), 0);
    assert_eq!(client.get_total_collected(), 0);
}

#[test]
fn test_refund_fee_emits_event() {
    let (env, admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    
    // User pays 1_000, fee = 50
    client.deduct_fee(&user, &1_000);
    
    // Admin refunds 30
    client.refund_fee(&admin, &user, &30, &"transaction_failed");
    
    // Check event was emitted
    let events = env.events().all();
    assert!(events
        .iter()
        .any(|e| e.topics.0 == "fee" && e.topics.1 == "refunded"));
}

#[test]
fn test_refund_fee_with_subsequent_transactions() {
    let (env, admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    
    // User pays 1_000, fee = 50
    client.deduct_fee(&user, &1_000);
    assert_eq!(client.get_user_fees_accrued(&user), 50);
    
    // Admin refunds 30
    client.refund_fee(&admin, &user, &30, &"partial_refund");
    assert_eq!(client.get_user_fees_accrued(&user), 20);
    
    // User makes another transaction, fee = 50
    client.deduct_fee(&user, &1_000);
    assert_eq!(client.get_user_fees_accrued(&user), 70);
    assert_eq!(client.get_total_collected(), 70);
}

#[test]
fn test_refund_fee_alternate_refund_reasons() {
    let (env, admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    
    // User pays 1_000, fee = 50
    client.deduct_fee(&user, &1_000);
    
    // Refund with different reasons for audit trail
    client.refund_fee(&admin, &user, &10, &"failed_transaction");
    client.refund_fee(&admin, &user, &15, &"customer_complaint");
    client.refund_fee(&admin, &user, &25, &"system_error");
    
    assert_eq!(client.get_user_fees_accrued(&user), 0);
    assert_eq!(client.get_total_collected(), 0);
}

#[test]
fn test_get_distribution_empty_default() {
    let (env, _admin, client) = setup_fee_contract();
    let dist = client.get_distribution();
    assert_eq!(dist.len(), 0);
}

#[test]
fn test_set_distribution_single_recipient() {
    let (env, admin, client) = setup_fee_contract();
    let recipient = Address::generate(&env);
    
    let mut recipients = Vec::new(&env);
    recipients.push_back(FeeRecipient {
        address: recipient.clone(),
        share_bps: 10_000, // 100%
    });
    
    client.set_distribution(&admin, &recipients);
    
    let dist = client.get_distribution();
    assert_eq!(dist.len(), 1);
    assert_eq!(dist.get(0).unwrap().share_bps, 10_000);
}

#[test]
fn test_set_distribution_multiple_recipients() {
    let (env, admin, client) = setup_fee_contract();
    let recipient1 = Address::generate(&env);
    let recipient2 = Address::generate(&env);
    let recipient3 = Address::generate(&env);
    
    let mut recipients = Vec::new(&env);
    recipients.push_back(FeeRecipient {
        address: recipient1,
        share_bps: 5_000, // 50%
    });
    recipients.push_back(FeeRecipient {
        address: recipient2,
        share_bps: 3_000, // 30%
    });
    recipients.push_back(FeeRecipient {
        address: recipient3,
        share_bps: 2_000, // 20%
    });
    
    client.set_distribution(&admin, &recipients);
    
    let dist = client.get_distribution();
    assert_eq!(dist.len(), 3);
}

#[test]
fn test_set_distribution_invalid_empty() {
    let (env, admin, client) = setup_fee_contract();
    let recipients = Vec::new(&env);
    
    // Should panic on empty distribution
    let result = std::panic::catch_unwind(|| {
        client.set_distribution(&admin, &recipients);
    });
    assert!(result.is_err());
}

#[test]
fn test_set_distribution_invalid_sum_less_than_100() {
    let (env, admin, client) = setup_fee_contract();
    let recipient1 = Address::generate(&env);
    let recipient2 = Address::generate(&env);
    
    let mut recipients = Vec::new(&env);
    recipients.push_back(FeeRecipient {
        address: recipient1,
        share_bps: 5_000, // 50%
    });
    recipients.push_back(FeeRecipient {
        address: recipient2,
        share_bps: 3_000, // 30% => total 80%
    });
    
    // Should panic because total < 100%
    let result = std::panic::catch_unwind(|| {
        client.set_distribution(&admin, &recipients);
    });
    assert!(result.is_err());
}

#[test]
fn test_set_distribution_invalid_sum_more_than_100() {
    let (env, admin, client) = setup_fee_contract();
    let recipient1 = Address::generate(&env);
    let recipient2 = Address::generate(&env);
    
    let mut recipients = Vec::new(&env);
    recipients.push_back(FeeRecipient {
        address: recipient1,
        share_bps: 6_000, // 60%
    });
    recipients.push_back(FeeRecipient {
        address: recipient2,
        share_bps: 5_000, // 50% => total 110%
    });
    
    // Should panic because total > 100%
    let result = std::panic::catch_unwind(|| {
        client.set_distribution(&admin, &recipients);
    });
    assert!(result.is_err());
}

#[test]
fn test_set_distribution_invalid_share_exceeds_100() {
    let (env, admin, client) = setup_fee_contract();
    let recipient = Address::generate(&env);
    
    let mut recipients = Vec::new(&env);
    recipients.push_back(FeeRecipient {
        address: recipient,
        share_bps: 15_000, // > 10_000 invalid
    });
    
    // Should panic because individual share > 100%
    let result = std::panic::catch_unwind(|| {
        client.set_distribution(&admin, &recipients);
    });
    assert!(result.is_err());
}

#[test]
fn test_set_distribution_unauthorized() {
    let (env, admin, client) = setup_fee_contract();
    let attacker = Address::generate(&env);
    let recipient = Address::generate(&env);
    
    let mut recipients = Vec::new(&env);
    recipients.push_back(FeeRecipient {
        address: recipient,
        share_bps: 10_000,
    });
    
    // Should panic because attacker is not admin
    let result = std::panic::catch_unwind(|| {
        client.set_distribution(&attacker, &recipients);
    });
    assert!(result.is_err());
}

#[test]
fn test_set_distribution_emits_event() {
    let (env, admin, client) = setup_fee_contract();
    let recipient = Address::generate(&env);
    
    let mut recipients = Vec::new(&env);
    recipients.push_back(FeeRecipient {
        address: recipient,
        share_bps: 10_000,
    });
    
    client.set_distribution(&admin, &recipients);
    
    let events = env.events().all();
    assert!(events
        .iter()
        .any(|e| e.topics.0 == "fee" && e.topics.1 == "dist_cfg"));
}

#[test]
fn test_distribute_fees_no_distribution_configured() {
    let (env, admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    
    // Pay fees without setting distribution
    client.deduct_fee(&user, &1_000); // fee = 50
    
    // Should panic because distribution not configured
    let result = std::panic::catch_unwind(|| {
        client.distribute_fees(&admin);
    });
    assert!(result.is_err());
}

#[test]
fn test_distribute_fees_zero_collected() {
    let (env, admin, client) = setup_fee_contract();
    let recipient = Address::generate(&env);
    
    let mut recipients = Vec::new(&env);
    recipients.push_back(FeeRecipient {
        address: recipient.clone(),
        share_bps: 10_000,
    });
    
    client.set_distribution(&admin, &recipients);
    
    // No fees collected, distribution should return 0
    let distributed = client.distribute_fees(&admin);
    assert_eq!(distributed, 0);
}

#[test]
fn test_distribute_fees_single_recipient() {
    let (env, admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    let recipient = Address::generate(&env);
    
    // Set distribution
    let mut recipients = Vec::new(&env);
    recipients.push_back(FeeRecipient {
        address: recipient.clone(),
        share_bps: 10_000, // 100%
    });
    client.set_distribution(&admin, &recipients);
    
    // Collect fees
    client.deduct_fee(&user, &2_000); // fee = 100
    assert_eq!(client.get_total_collected(), 100);
    
    // Distribute
    let distributed = client.distribute_fees(&admin);
    assert_eq!(distributed, 100);
    
    // Verify recipient received all fees
    assert_eq!(client.get_recipient_fees_accumulated(&recipient), 100);
    
    // Verify total fees cleared
    assert_eq!(client.get_total_collected(), 0);
}

#[test]
fn test_distribute_fees_multiple_recipients_equal_split() {
    let (env, admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    let recipient1 = Address::generate(&env);
    let recipient2 = Address::generate(&env);
    
    // Set distribution: 50% each
    let mut recipients = Vec::new(&env);
    recipients.push_back(FeeRecipient {
        address: recipient1.clone(),
        share_bps: 5_000,
    });
    recipients.push_back(FeeRecipient {
        address: recipient2.clone(),
        share_bps: 5_000,
    });
    client.set_distribution(&admin, &recipients);
    
    // Collect fees: 1_000 * 5% = 50
    client.deduct_fee(&user, &1_000);
    assert_eq!(client.get_total_collected(), 50);
    
    // Distribute
    let distributed = client.distribute_fees(&admin);
    assert_eq!(distributed, 50);
    
    // Verify split: 50 * 50% = 25 each
    assert_eq!(client.get_recipient_fees_accumulated(&recipient1), 25);
    assert_eq!(client.get_recipient_fees_accumulated(&recipient2), 25);
}

#[test]
fn test_distribute_fees_multiple_recipients_unequal_split() {
    let (env, admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    let recipient1 = Address::generate(&env);
    let recipient2 = Address::generate(&env);
    let recipient3 = Address::generate(&env);
    
    // Set distribution: 50%, 30%, 20%
    let mut recipients = Vec::new(&env);
    recipients.push_back(FeeRecipient {
        address: recipient1.clone(),
        share_bps: 5_000,
    });
    recipients.push_back(FeeRecipient {
        address: recipient2.clone(),
        share_bps: 3_000,
    });
    recipients.push_back(FeeRecipient {
        address: recipient3.clone(),
        share_bps: 2_000,
    });
    client.set_distribution(&admin, &recipients);
    
    // Collect fees: 1_000 * 5% = 50
    client.deduct_fee(&user, &1_000);
    
    // Distribute
    let distributed = client.distribute_fees(&admin);
    assert_eq!(distributed, 50);
    
    // Verify distribution
    // recipient1: 50 * 50% = 25
    // recipient2: 50 * 30% = 15
    // recipient3: 50 * 20% = 10
    assert_eq!(client.get_recipient_fees_accumulated(&recipient1), 25);
    assert_eq!(client.get_recipient_fees_accumulated(&recipient2), 15);
    assert_eq!(client.get_recipient_fees_accumulated(&recipient3), 10);
}

#[test]
fn test_distribute_fees_multiple_distributions_accumulates() {
    let (env, admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    let recipient = Address::generate(&env);
    
    // Set distribution: 100% to recipient
    let mut recipients = Vec::new(&env);
    recipients.push_back(FeeRecipient {
        address: recipient.clone(),
        share_bps: 10_000,
    });
    client.set_distribution(&admin, &recipients);
    
    // First fee collection and distribution
    client.deduct_fee(&user, &1_000); // fee = 50
    client.distribute_fees(&admin);
    assert_eq!(client.get_recipient_fees_accumulated(&recipient), 50);
    
    // Second fee collection and distribution
    client.deduct_fee(&user, &1_000); // fee = 50
    client.distribute_fees(&admin);
    assert_eq!(client.get_recipient_fees_accumulated(&recipient), 100);
    
    // Third fee collection and distribution
    client.deduct_fee(&user, &2_000); // fee = 100
    client.distribute_fees(&admin);
    assert_eq!(client.get_recipient_fees_accumulated(&recipient), 200);
}

#[test]
fn test_distribute_fees_unauthorized() {
    let (env, admin, client) = setup_fee_contract();
    let attacker = Address::generate(&env);
    let recipient = Address::generate(&env);
    
    let mut recipients = Vec::new(&env);
    recipients.push_back(FeeRecipient {
        address: recipient,
        share_bps: 10_000,
    });
    client.set_distribution(&admin, &recipients);
    
    // Should panic because attacker is not admin
    let result = std::panic::catch_unwind(|| {
        client.distribute_fees(&attacker);
    });
    assert!(result.is_err());
}

#[test]
fn test_distribute_fees_emits_event() {
    let (env, admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    let recipient = Address::generate(&env);
    
    let mut recipients = Vec::new(&env);
    recipients.push_back(FeeRecipient {
        address: recipient,
        share_bps: 10_000,
    });
    client.set_distribution(&admin, &recipients);
    
    client.deduct_fee(&user, &1_000);
    client.distribute_fees(&admin);
    
    let events = env.events().all();
    assert!(events
        .iter()
        .any(|e| e.topics.0 == "fee" && e.topics.1 == "distributed"));
}

#[test]
fn test_get_recipient_fees_accumulated_zero_default() {
    let (env, _admin, client) = setup_fee_contract();
    let recipient = Address::generate(&env);
    
    // Recipient with no distributions should return 0
    assert_eq!(client.get_recipient_fees_accumulated(&recipient), 0);
}

#[test]
fn test_distribution_with_refunds_before_distribution() {
    let (env, admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    let recipient = Address::generate(&env);
    
    // Set distribution
    let mut recipients = Vec::new(&env);
    recipients.push_back(FeeRecipient {
        address: recipient.clone(),
        share_bps: 10_000,
    });
    client.set_distribution(&admin, &recipients);
    
    // Collect fees: fee = 50
    client.deduct_fee(&user, &1_000);
    assert_eq!(client.get_total_collected(), 50);
    
    // Refund partial fee: 20
    client.refund_fee(&admin, &user, &20, &"partial_cancel");
    assert_eq!(client.get_total_collected(), 30);
    
    // Distribute remaining
    let distributed = client.distribute_fees(&admin);
    assert_eq!(distributed, 30);
    assert_eq!(client.get_recipient_fees_accumulated(&recipient), 30);
}

#[test]
fn test_distribution_update_configuration() {
    let (env, admin, client) = setup_fee_contract();
    let recipient1 = Address::generate(&env);
    let recipient2 = Address::generate(&env);
    
    // Initial distribution
    let mut recipients = Vec::new(&env);
    recipients.push_back(FeeRecipient {
        address: recipient1.clone(),
        share_bps: 10_000,
    });
    client.set_distribution(&admin, &recipients);
    
    let dist = client.get_distribution();
    assert_eq!(dist.len(), 1);
    
    // Update distribution to split
    let mut recipients = Vec::new(&env);
    recipients.push_back(FeeRecipient {
        address: recipient1.clone(),
        share_bps: 6_000,
    });
    recipients.push_back(FeeRecipient {
        address: recipient2.clone(),
        share_bps: 4_000,
    });
    client.set_distribution(&admin, &recipients);
    
    let dist = client.get_distribution();
    assert_eq!(dist.len(), 2);
}

#[test]
fn test_get_min_fee_default() {
    let (env, _admin, client) = setup_fee_contract();
    // Default min fee is 0
    assert_eq!(client.get_min_fee(), 0);
}

#[test]
fn test_get_max_fee_default() {
    let (env, _admin, client) = setup_fee_contract();
    // Default max fee is i128::MAX
    assert_eq!(client.get_max_fee(), i128::MAX);
}

#[test]
fn test_set_fee_bounds_valid() {
    let (env, admin, client) = setup_fee_contract();
    
    client.set_fee_bounds(&admin, &100, &1_000);
    
    assert_eq!(client.get_min_fee(), 100);
    assert_eq!(client.get_max_fee(), 1_000);
}

#[test]
fn test_set_fee_bounds_min_zero() {
    let (env, admin, client) = setup_fee_contract();
    
    client.set_fee_bounds(&admin, &0, &1_000);
    
    assert_eq!(client.get_min_fee(), 0);
    assert_eq!(client.get_max_fee(), 1_000);
}

#[test]
fn test_set_fee_bounds_equal() {
    let (env, admin, client) = setup_fee_contract();
    
    // Min and max can be equal
    client.set_fee_bounds(&admin, &500, &500);
    
    assert_eq!(client.get_min_fee(), 500);
    assert_eq!(client.get_max_fee(), 500);
}

#[test]
fn test_set_fee_bounds_invalid_negative_min() {
    let (env, admin, client) = setup_fee_contract();
    
    // Should panic on negative min_fee
    let result = std::panic::catch_unwind(|| {
        client.set_fee_bounds(&admin, &-100, &1_000);
    });
    assert!(result.is_err());
}

#[test]
fn test_set_fee_bounds_invalid_negative_max() {
    let (env, admin, client) = setup_fee_contract();
    
    // Should panic on negative max_fee
    let result = std::panic::catch_unwind(|| {
        client.set_fee_bounds(&admin, &100, &-1_000);
    });
    assert!(result.is_err());
}

#[test]
fn test_set_fee_bounds_invalid_range() {
    let (env, admin, client) = setup_fee_contract();
    
    // Should panic when max < min
    let result = std::panic::catch_unwind(|| {
        client.set_fee_bounds(&admin, &1_000, &100);
    });
    assert!(result.is_err());
}

#[test]
fn test_set_fee_bounds_unauthorized() {
    let (env, admin, client) = setup_fee_contract();
    let attacker = Address::generate(&env);
    
    // Should panic because attacker is not admin
    let result = std::panic::catch_unwind(|| {
        client.set_fee_bounds(&attacker, &100, &1_000);
    });
    assert!(result.is_err());
}

#[test]
fn test_set_fee_bounds_emits_event() {
    let (env, admin, client) = setup_fee_contract();
    
    client.set_fee_bounds(&admin, &100, &1_000);
    
    let events = env.events().all();
    assert!(events
        .iter()
        .any(|e| e.topics.0 == "fee" && e.topics.1 == "bounds_cfg"));
}

#[test]
fn test_calculate_fee_with_min_bound() {
    let (env, admin, client) = setup_fee_contract();
    
    // Set min fee to 100
    client.set_fee_bounds(&admin, &100, &i128::MAX);
    
    // Transaction with very small amount: 10 * 5% = 0.5, rounds to 0
    // But min fee is 100, so fee should be at least 100
    let fee = FeesContract::calculate_fee(env.clone(), 10);
    assert_eq!(fee, 100);
}

#[test]
fn test_calculate_fee_with_max_bound() {
    let (env, admin, client) = setup_fee_contract();
    
    // Set max fee to 100
    client.set_fee_bounds(&admin, &0, &100);
    
    // Transaction with huge amount: 10_000 * 5% = 500
    // But max fee is 100, so fee should be capped at 100
    let fee = FeesContract::calculate_fee(env.clone(), 10_000);
    assert_eq!(fee, 100);
}

#[test]
fn test_calculate_fee_between_bounds() {
    let (env, admin, client) = setup_fee_contract();
    
    // Set bounds: min=50, max=150
    client.set_fee_bounds(&admin, &50, &150);
    
    // Transaction with 2000: 2000 * 5% = 100
    // 50 < 100 < 150, so fee is 100
    let fee = FeesContract::calculate_fee(env.clone(), 2_000);
    assert_eq!(fee, 100);
}

#[test]
fn test_calculate_fee_min_and_max_equal() {
    let (env, admin, client) = setup_fee_contract();
    
    // Fixed fee of 75
    client.set_fee_bounds(&admin, &75, &75);
    
    // Any transaction should have fee = 75
    assert_eq!(FeesContract::calculate_fee(env.clone(), 100), 75);
    assert_eq!(FeesContract::calculate_fee(env.clone(), 10_000), 75);
    assert_eq!(FeesContract::calculate_fee(env.clone(), 1_000), 75);
}

#[test]
fn test_deduct_fee_respects_min_bound() {
    let (env, admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    
    // Set min fee to 200
    client.set_fee_bounds(&admin, &200, &i128::MAX);
    
    // Transaction with small amount: 50 * 5% = 2.5, rounds to 2
    // But min fee enforced, so fee should be 200
    let (_net, fee) = client.deduct_fee(&user, &50);
    assert_eq!(fee, 200);
    assert_eq!(client.get_user_fees_accrued(&user), 200);
    assert_eq!(client.get_total_collected(), 200);
}

#[test]
fn test_deduct_fee_respects_max_bound() {
    let (env, admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    
    // Set max fee to 75
    client.set_fee_bounds(&admin, &0, &75);
    
    // Transaction with 2000: 2000 * 5% = 100
    // But max fee enforced, so fee should be 75
    let (net, fee) = client.deduct_fee(&user, &2_000);
    assert_eq!(fee, 75);
    assert_eq!(net, 1_925);
    assert_eq!(client.get_user_fees_accrued(&user), 75);
    assert_eq!(client.get_total_collected(), 75);
}

#[test]
fn test_multiple_transactions_with_bounds() {
    let (env, admin, client) = setup_fee_contract();
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    
    // Set bounds: min=50, max=150
    client.set_fee_bounds(&admin, &50, &150);
    
    // Small transaction (would be 5): should be 50 (min)
    client.deduct_fee(&user1, &100);
    assert_eq!(client.get_user_fees_accrued(&user1), 50);
    
    // Medium transaction (would be 100): should be 100
    client.deduct_fee(&user2, &2_000);
    assert_eq!(client.get_user_fees_accrued(&user2), 100);
    
    // Large transaction (would be 500): should be 150 (max)
    client.deduct_fee(&user1, &10_000);
    assert_eq!(client.get_user_fees_accrued(&user1), 200); // 50 + 150
    
    assert_eq!(client.get_total_collected(), 300); // 50 + 100 + 150
}

#[test]
fn test_update_fee_bounds() {
    let (env, admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    
    // Initial bounds: min=100, max=200
    client.set_fee_bounds(&admin, &100, &200);
    client.deduct_fee(&user, &2_000); // 2000*5%=100, within bounds
    assert_eq!(client.get_user_fees_accrued(&user), 100);
    
    // Update bounds: min=500, max=1000
    client.set_fee_bounds(&admin, &500, &1_000);
    client.deduct_fee(&user, &2_000); // 2000*5%=100, but min=500
    assert_eq!(client.get_user_fees_accrued(&user), 600); // 100 + 500
}

#[test]
fn test_fee_bounds_with_distribution() {
    let (env, admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    let recipient1 = Address::generate(&env);
    let recipient2 = Address::generate(&env);
    
    // Set fee bounds
    client.set_fee_bounds(&admin, &100, &200);
    
    // Set distribution
    let mut recipients = Vec::new(&env);
    recipients.push_back(FeeRecipient {
        address: recipient1.clone(),
        share_bps: 6_000,
    });
    recipients.push_back(FeeRecipient {
        address: recipient2.clone(),
        share_bps: 4_000,
    });
    client.set_distribution(&admin, &recipients);
    
    // Collect fees with bounds applied
    client.deduct_fee(&user, &10_000); // 10_000*5%=500, capped at 200
    assert_eq!(client.get_total_collected(), 200);
    
    // Distribute
    client.distribute_fees(&admin);
    assert_eq!(client.get_recipient_fees_accumulated(&recipient1), 120); // 200 * 60%
    assert_eq!(client.get_recipient_fees_accumulated(&recipient2), 80);  // 200 * 40%
}

#[test]
fn test_fee_bounds_with_refunds() {
    let (env, admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    
    // Set min fee to 100
    client.set_fee_bounds(&admin, &100, &i128::MAX);
    
    // Small transaction gets bumped to min: 100
    client.deduct_fee(&user, &50);
    assert_eq!(client.get_user_fees_accrued(&user), 100);
    assert_eq!(client.get_total_collected(), 100);
    
    // Refund partially
    client.refund_fee(&admin, &user, &30, &"partial");
    assert_eq!(client.get_user_fees_accrued(&user), 70);
    assert_eq!(client.get_total_collected(), 70);
}

#[test]
fn test_large_transactions_exceed_percentage_fee() {
    let (env, admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    
    // Fee percentage: 5%, max fee capped at 1000
    client.set_fee_bounds(&admin, &0, &1_000);
    
    // Transaction: 100_000 * 5% = 5_000, capped at 1_000
    let (net, fee) = client.deduct_fee(&user, &100_000);
    assert_eq!(fee, 1_000);
    assert_eq!(net, 99_000);
}

#[test]
fn test_very_small_transactions_with_min_fee() {
    let (env, admin, client) = setup_fee_contract();
    let user = Address::generate(&env);
    
    // Min fee: 50
    client.set_fee_bounds(&admin, &50, &i128::MAX);
    
    // Transaction: 1 * 5% = 0, boosted to 50
    let (net, fee) = client.deduct_fee(&user, &1);
    assert_eq!(fee, 50);
    assert_eq!(net, 1 - 50); // Negative is allowed for transaction logic
}

#[test]
fn test_validate_fee_success() {
    let (env, _admin, client) = setup_fee_contract();
    let amount: i128 = 2_000;
    // 2_000 * 5% = 100
    client.validate_fee(&amount, &100);
}

#[test]
fn test_validate_fee_failure() {
    let (env, _admin, client) = setup_fee_contract();
    let amount: i128 = 2_000;
    // 2_000 * 5% = 100, so 99 should fail
    let result = std::panic::catch_unwind(|| {
        client.validate_fee(&amount, &99);
    });
    assert!(result.is_err());
}

#[test]
fn test_validate_fee_invalid_amount() {
    let (env, _admin, client) = setup_fee_contract();
    // input amount 0 should trigger InvalidAmount panic inside calculate_fee
    let result = std::panic::catch_unwind(|| {
        client.validate_fee(&0, &0);
    });
    assert!(result.is_err());
}

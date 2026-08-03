#![no_std]

mod test;
mod types;

pub use crate::types::{ContractUtils, DataKey};
use crate::types::{Payment, ReceiptEvent};
use shared::utils::generate_transaction_reference_id;
use soroban_sdk::{contract, contractimpl, symbol_short, token, Address, Env, String, Symbol, Vec};

#[contract]
pub struct BatchPaymentContract;

#[contractimpl]
impl BatchPaymentContract {
    /// Transfers tokens from the caller to multiple recipients.
    ///
    /// Generates a unique reference ID for the batch payment to enable tracking
    /// and reconciliation of the entire batch transaction.
    ///
    /// # Arguments
    /// * `env` - The contract environment.
    /// * `from` - The address sending the tokens (must authorize the call).
    /// * `token` - The address of the token contract (e.g., USDC).
    /// * `payments` - A vector of `Payment` structs containing recipients and amounts.
    ///
    /// # Returns
    /// A reference ID string for tracking this batch payment.
    pub fn batch_transfer(
        env: Env,
        from: Address,
        token: Address,
        payments: Vec<Payment>,
    ) -> String {
        // Require authorization from the sender
        from.require_auth();

        let token_client = token::Client::new(&env, &token);

        // Generate a unique batch reference ID
        let batch_ref_counter_key = Symbol::new(&env, "batch_ref_counter");
        let batch_reference_id =
            generate_transaction_reference_id(&env, &from, &batch_ref_counter_key);

        // Increment batch counter and record status
        let mut batch_counter: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::BatchCounter)
            .unwrap_or(0);
        batch_counter += 1;
        env.storage()
            .persistent()
            .set(&DataKey::BatchCounter, &batch_counter);

        let mut total_amount: i128 = 0;
        let mut count: u32 = 0;

        for payment in payments.iter() {
            // Validation
            if payment.amount <= 0 {
                panic!("Payment amount must be positive");
            }

            // Execute transfer
            token_client.transfer(&from, &payment.recipient, &payment.amount);

            total_amount += payment.amount;
            count += 1;

            // Emit per-payment event with batch reference ID
            // Topics: (payment, batch_id, recipient)
            // Data: (token, amount)
            let topics = (
                symbol_short!("payment"),
                batch_reference_id.clone(),
                payment.recipient.clone(),
            );
            env.events()
                .publish(topics, (token.clone(), payment.amount));
        }

        // Record batch payment status as complete
        env.storage().persistent().set(
            &DataKey::BatchStatus(batch_counter),
            &symbol_short!("complete"),
        );

        // Emit batch completion event with reference ID
        // Topics: (batch, complete, batch_reference_id)
        // Data: (total_payments, total_amount)
        let topics = (
            symbol_short!("batch"),
            symbol_short!("complete"),
            batch_reference_id.clone(),
        );
        env.events().publish(topics, (count, total_amount));

        // Emit receipt event for off-chain reconciliation
        env.events().publish(
            (symbol_short!("receipt"),),
            ReceiptEvent {
                batch_reference_id: batch_reference_id.clone(),
                token: token.clone(),
                from: from.clone(),
                total_payments: count,
                total_amount,
            },
        );

        batch_reference_id
    }

    /// Returns the status of a batch payment identified by its batch ID.
    ///
    /// Returns `Some(Symbol("complete"))` for successfully executed batches,
    /// or `None` if the batch ID does not exist.
    pub fn get_batch_payment_status(env: Env, batch_id: u64) -> Option<Symbol> {
        env.storage()
            .persistent()
            .get(&DataKey::BatchStatus(batch_id))
    }
}

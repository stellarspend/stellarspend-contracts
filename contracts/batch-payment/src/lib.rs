#![no_std]

#[cfg(test)]
mod test;
mod types;

use shared::utils::generate_transaction_reference_id;
use soroban_sdk::{contract, contractimpl, symbol_short, token, Address, Env, String, Symbol, Vec};

pub use crate::types::{Payment, PaymentReceipt};

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

            let receipt = PaymentReceipt {
                batch_reference_id: batch_reference_id.clone(),
                recipient: payment.recipient.clone(),
                token: token.clone(),
                amount: payment.amount,
                index: count,
            };

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

            env.events().publish(
                (
                    symbol_short!("payment"),
                    symbol_short!("receipt"),
                    payment.recipient.clone(),
                ),
                receipt,
            );
        }

        // Emit batch completion event with reference ID
        // Topics: (batch, complete, batch_reference_id)
        // Data: (total_payments, total_amount)
        let topics = (
            symbol_short!("batch"),
            symbol_short!("complete"),
            batch_reference_id.clone(),
        );
        env.events().publish(topics, (count, total_amount));

        batch_reference_id
    }
}

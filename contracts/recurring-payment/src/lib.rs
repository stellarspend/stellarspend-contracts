#![no_std]

#[cfg(test)]
mod test;
mod types;

use crate::types::{DataKey, IncomeStream, RecurringPayment, RecurringPaymentSchedule};
use soroban_sdk::{contract, contractimpl, symbol_short, token, Address, Env, Symbol, Vec};

#[contract]
pub struct RecurringPaymentContract;

#[contractimpl]
impl RecurringPaymentContract {
    /// Creates a new recurring payment schedule.
    ///
    /// # Arguments
    /// * `sender`     - The address funding the payments (must authorize)
    /// * `recipient`  - The address that receives each payment
    /// * `token`      - The token contract address
    /// * `amount`     - Amount transferred on each execution (must be > 0)
    /// * `interval`   - Seconds between executions (must be > 0)
    /// * `start_time` - Ledger timestamp of the first allowed execution
    ///
    /// # Returns
    /// The unique payment ID assigned to this schedule.
    pub fn create_payment(
        env: Env,
        sender: Address,
        recipient: Address,
        token: Address,
        amount: i128,
        interval: u64,
        start_time: u64,
    ) -> u64 {
        sender.require_auth();

        if amount <= 0 {
            panic!("Amount must be positive");
        }
        if interval == 0 {
            panic!("Interval must be positive");
        }

        let mut count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::PaymentCount)
            .unwrap_or(0);
        count += 1;

        let payment = RecurringPayment {
            sender: sender.clone(),
            recipient,
            token,
            amount,
            interval,
            next_execution: start_time,
            active: true,
            paused: false,
            execution_count: 0,
            missed_count: 0,
            last_missed_at: 0,
        };

        env.storage()
            .instance()
            .set(&DataKey::Payment(count), &payment);
        env.storage().instance().set(&DataKey::PaymentCount, &count);

        env.events().publish(
            (symbol_short!("recur"), symbol_short!("created"), count),
            sender,
        );

        count
    }

    /// # Arguments
    /// * `payment_id` - The ID returned by `create_payment`
    pub fn execute_payment(env: Env, payment_id: u64) {
        let mut payment: RecurringPayment = env
            .storage()
            .instance()
            .get(&DataKey::Payment(payment_id))
            .expect("Payment not found");

        if !payment.active {
            panic!("Payment is not active");
        }
        if payment.paused {
            panic!("Payment is paused");
        }

        let current_time = env.ledger().timestamp();
        if current_time < payment.next_execution {
            panic!("Too early for next execution");
        }

        payment.sender.require_auth();

        // Attempt token transfer; track missed execution on failure.
        let token_client = token::Client::new(&env, &payment.token);
        let sender_balance = token_client.balance(&payment.sender);
        let now = env.ledger().timestamp();

        if sender_balance < payment.amount {
            payment.missed_count = payment.missed_count.saturating_add(1);
            payment.last_missed_at = now;
            payment.next_execution = payment.next_execution.saturating_add(payment.interval);
            if payment.next_execution <= current_time {
                let intervals_passed = (current_time - payment.next_execution) / payment.interval;
                payment.next_execution += (intervals_passed + 1) * payment.interval;
            }
            env.storage()
                .instance()
                .set(&DataKey::Payment(payment_id), &payment);
            env.events().publish(
                (symbol_short!("recur"), symbol_short!("missed"), payment_id),
                (payment.missed_count, payment.last_missed_at),
            );
            return;
        }

        token_client.transfer(&payment.sender, &payment.recipient, &payment.amount);

        // Reset missed count on successful execution.
        payment.missed_count = 0;
        payment.last_missed_at = 0;

        // Update next execution time
        payment.next_execution += payment.interval;

        // If the execution was delayed, we might want to skip or catch up.
        // For simplicity, we just add the interval to the scheduled time.
        // If current_time is way past next_execution, catch up.
        if payment.next_execution <= current_time {
            // Option 1: Catch up to the next interval in the future
            // (current_time - scheduled) / interval * interval + scheduled + interval
            let intervals_passed = (current_time - payment.next_execution) / payment.interval;
            payment.next_execution += (intervals_passed + 1) * payment.interval;
        }

        // Increment execution counter
        payment.execution_count += 1;

        env.storage()
            .instance()
            .set(&DataKey::Payment(payment_id), &payment);

        env.events().publish(
            (
                symbol_short!("recur"),
                symbol_short!("executed"),
                payment_id,
            ),
            (payment.amount, payment.next_execution),
        );
    }

    /// Cancels a recurring payment. Only the original sender may cancel.
    ///
    /// # Arguments
    /// * `payment_id` - The ID returned by `create_payment`
    pub fn cancel_payment(env: Env, payment_id: u64) {
        let mut payment: RecurringPayment = env
            .storage()
            .instance()
            .get(&DataKey::Payment(payment_id))
            .expect("Payment not found");

        payment.sender.require_auth();

        if !payment.active {
            panic!("Payment is already canceled");
        }

        payment.active = false;
        env.storage()
            .instance()
            .set(&DataKey::Payment(payment_id), &payment);

        env.events().publish(
            (
                symbol_short!("recur"),
                symbol_short!("canceled"),
                payment_id,
            ),
            payment.sender,
        );
    }

    /// Pauses an active recurring payment schedule.
    ///
    /// The sender must authorize the pause action.
    pub fn pause_payment(env: Env, payment_id: u64) {
        let mut payment: RecurringPayment = env
            .storage()
            .instance()
            .get(&DataKey::Payment(payment_id))
            .expect("Payment not found");

        payment.sender.require_auth();

        if !payment.active {
            panic!("Payment is not active");
        }
        if payment.paused {
            panic!("Payment is already paused");
        }

        payment.paused = true;
        env.storage()
            .instance()
            .set(&DataKey::Payment(payment_id), &payment);

        env.events().publish(
            (symbol_short!("recur"), symbol_short!("paused"), payment_id),
            payment.sender,
        );
    }

    /// Resumes a paused recurring payment schedule.
    ///
    /// The sender must authorize the resume action.
    pub fn resume_payment(env: Env, payment_id: u64) {
        let mut payment: RecurringPayment = env
            .storage()
            .instance()
            .get(&DataKey::Payment(payment_id))
            .expect("Payment not found");

        payment.sender.require_auth();

        if !payment.active {
            panic!("Payment is not active");
        }
        if !payment.paused {
            panic!("Payment is not paused");
        }

        payment.paused = false;
        env.storage()
            .instance()
            .set(&DataKey::Payment(payment_id), &payment);

        env.events().publish(
            (symbol_short!("recur"), symbol_short!("resumed"), payment_id),
            payment.sender,
        );
    }

    /// Returns the full details of a payment schedule.
    ///
    /// # Arguments
    /// * `payment_id` - The ID returned by `create_payment`
    pub fn get_payment(env: Env, payment_id: u64) -> RecurringPayment {
        env.storage()
            .instance()
            .get(&DataKey::Payment(payment_id))
            .expect("Payment not found")
    }

    /// Returns the compact schedule details for a recurring payment.
    ///
    /// # Arguments
    /// * `payment_id` - The ID returned by `create_payment`
    ///
    /// # Errors
    /// Panics with `Payment not found` when `payment_id` does not exist.
    pub fn get_recurring_payment_schedule(env: Env, payment_id: u64) -> RecurringPaymentSchedule {
        let payment: RecurringPayment = env
            .storage()
            .instance()
            .get(&DataKey::Payment(payment_id))
            .expect("Payment not found");

        RecurringPaymentSchedule {
            amount: payment.amount,
            interval: payment.interval,
            next_due_date: payment.next_execution,
        }
    }

    /// Creates a recurring income stream that auto-funds budgets or goals.
    ///
    /// # Arguments
    /// * `recipient` - The address receiving the income (must authorize)
    /// * `source` - Description of the income source
    /// * `amount` - Amount received per interval (> 0)
    /// * `interval_seconds` - Seconds between income events (> 0)
    /// * `target_goal_id` - Goal ID to auto-fund (0 = manual allocation)
    ///
    /// # Returns
    /// The unique stream ID.
    pub fn create_income_stream(
        env: Env,
        recipient: Address,
        source: Symbol,
        amount: i128,
        interval_seconds: u64,
        target_goal_id: u64,
    ) -> u64 {
        recipient.require_auth();

        if amount <= 0 {
            panic!("Amount must be positive");
        }
        if interval_seconds == 0 {
            panic!("Interval must be positive");
        }

        let mut count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::IncomeStreamCount)
            .unwrap_or(0);
        count += 1;

        let now = env.ledger().timestamp();
        let stream = IncomeStream {
            stream_id: count,
            recipient: recipient.clone(),
            source: source.clone(),
            amount,
            interval_seconds,
            next_payout: now + interval_seconds,
            target_goal_id,
            active: true,
        };

        env.storage()
            .instance()
            .set(&DataKey::IncomeStream(count), &stream);
        env.storage()
            .instance()
            .set(&DataKey::IncomeStreamCount, &count);

        // Track stream in user's list
        let mut user_streams: Vec<u64> = env
            .storage()
            .instance()
            .get(&DataKey::UserIncomeStreams(recipient.clone()))
            .unwrap_or(Vec::new(&env));
        user_streams.push_back(count);
        env.storage().instance().set(
            &DataKey::UserIncomeStreams(recipient.clone()),
            &user_streams,
        );

        env.events().publish(
            (symbol_short!("income"), symbol_short!("created"), count),
            (recipient, source, amount),
        );

        count
    }

    /// Processes a recurring income stream, calculating and applying any
    /// missed cycles since the last payout.
    ///
    /// Automatically updates the next payout time and handles catch-up.
    ///
    /// # Arguments
    /// * `stream_id` - The ID returned by `create_income_stream`
    pub fn process_income(env: Env, stream_id: u64) {
        let mut stream: IncomeStream = env
            .storage()
            .instance()
            .get(&DataKey::IncomeStream(stream_id))
            .expect("Income stream not found");

        if !stream.active {
            panic!("Income stream is not active");
        }

        let now = env.ledger().timestamp();
        if now < stream.next_payout {
            panic!("Too early for next payout");
        }

        // Calculate cycles due (handle catch-up for missed intervals)
        let elapsed = now - stream.next_payout;
        let cycles_due = 1 + (elapsed / stream.interval_seconds);
        let total_payout = stream
            .amount
            .checked_mul(cycles_due as i128)
            .unwrap_or(i128::MAX);

        // Advance next_payout past all due cycles
        stream.next_payout += cycles_due * stream.interval_seconds;

        // If still behind, catch up to the future
        if stream.next_payout <= now {
            stream.next_payout = now + stream.interval_seconds;
        }

        env.storage()
            .instance()
            .set(&DataKey::IncomeStream(stream_id), &stream);

        env.events().publish(
            (
                symbol_short!("income"),
                symbol_short!("processed"),
                stream_id,
            ),
            (stream.recipient, total_payout, cycles_due),
        );
    }

    /// Cancels a recurring income stream. Only the recipient may cancel.
    ///
    /// # Arguments
    /// * `stream_id` - The ID returned by `create_income_stream`
    pub fn cancel_income_stream(env: Env, stream_id: u64) {
        let mut stream: IncomeStream = env
            .storage()
            .instance()
            .get(&DataKey::IncomeStream(stream_id))
            .expect("Income stream not found");

        stream.recipient.require_auth();

        if !stream.active {
            panic!("Income stream is already canceled");
        }

        stream.active = false;
        env.storage()
            .instance()
            .set(&DataKey::IncomeStream(stream_id), &stream);

        env.events().publish(
            (
                symbol_short!("income"),
                symbol_short!("canceled"),
                stream_id,
            ),
            stream.recipient,
        );
    }

    /// Returns the full details of an income stream.
    ///
    /// # Arguments
    /// * `stream_id` - The ID returned by `create_income_stream`
    pub fn get_income_stream(env: Env, stream_id: u64) -> IncomeStream {
        env.storage()
            .instance()
            .get(&DataKey::IncomeStream(stream_id))
            .expect("Income stream not found")
    }

    /// Returns all income stream IDs for a user.
    pub fn get_user_income_streams(env: Env, user: Address) -> Vec<u64> {
        env.storage()
            .instance()
            .get(&DataKey::UserIncomeStreams(user))
            .unwrap_or(Vec::new(&env))
    }
}

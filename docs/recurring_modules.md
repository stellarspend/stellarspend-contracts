# Recurring Modules — Boundary Guide

This document clarifies the responsibilities of the three recurring-related
modules to avoid confusion when contributing.

## `contracts/recurring/`

Handles **general-purpose recurring logic**: executor, scheduler, and shared
types. Use this when scheduling any repeating on-chain action.

- `executor.rs` — triggers scheduled executions
- `scheduler.rs` — stores and manages recurring schedules
- `types.rs` — shared data types (RecurringSchedule, Frequency, etc.)

## `contracts/recurring-payment/`

Owns **recurring payment flows** specifically. Extends the base recurring
crate with payment-specific state and error handling.

Use when the recurring action is a token transfer between parties.

## `contracts/recurring_savings.rs`

Handles **auto-savings contributions**. Periodically moves funds from a
spending wallet to a savings goal.

Use when the recurring action deposits into a savings goal.

## Rule of Thumb

- New recurring feature? → `contracts/recurring/`
- Recurring token send? → `contracts/recurring-payment/`
- Recurring savings deposit? → `contracts/recurring_savings.rs`

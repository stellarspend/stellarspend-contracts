#![no_std]

mod errors;
mod events;
mod budget_notifier;
mod types;

use soroban_sdk::{contract, contractimpl, contracttype, panic_with_error, Address, Env, String, Symbol, Vec};
use budget_notifier::BudgetNotifier;
use errors::NotificationError;
use types::{Notification, NotificationResult};

#[contracttype]
#[derive(Clone)]
enum DataKey {
    NotificationPreference(Address),
}

#[contract]
pub struct NotificationContract;

#[contractimpl]
impl NotificationContract {
    pub fn send_batch_notifications(
        env: Env,
        notifications: Vec<Notification>,
    ) -> Vec<NotificationResult> {
        if notifications.is_empty() {
            panic_with_error!(&env, NotificationError::EmptyBatch);
        }

        let mut results: Vec<NotificationResult> = Vec::new(&env);

        for notification in notifications.iter() {
            let mut success = true;

            if notification.message.len() == 0 {
                success = false;
            }

            if !Self::is_supported_language(&notification.language) {
                success = false;
            }

            if success {
                env.events().publish(
                    (Symbol::new(&env, "notification_sent"), notification.recipient.clone()),
                    notification.message.clone(),
                );
            }

            results.push_back(NotificationResult {
                recipient: notification.recipient.clone(),
                success,
            });
        }

        results
    }

    pub fn set_notification_preference(env: Env, owner: Address, enabled: bool) {
        env.storage().persistent().set(&DataKey::NotificationPreference(owner), &enabled);
    }

    pub fn get_notification_preference(env: Env, owner: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::NotificationPreference(owner))
            .unwrap_or(true)
    }

    pub fn update_budget(env: Env, used: i128, limit: i128) {
        BudgetNotifier::check_usage(&env, used, limit);
    }

    pub fn complete_goal(env: Env) {
        BudgetNotifier::goal_completed(&env);
    }

    fn is_supported_language(lang: &String) -> bool {
        lang == &String::from_str("en")
            || lang == &String::from_str("fr")
            || lang == &String::from_str("es")
            || lang == &String::from_str("de")
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn test_notification_preference_defaults_to_enabled_for_unset_owner() {
        let env = Env::default();
        let contract_id = env.register_contract(None, NotificationContract);
        let client = NotificationContractClient::new(&env, &contract_id);
        let owner = Address::random(&env);

        assert!(client.get_notification_preference(&owner));
    }

    #[test]
    fn test_notification_preference_can_be_overridden() {
        let env = Env::default();
        let contract_id = env.register_contract(None, NotificationContract);
        let client = NotificationContractClient::new(&env, &contract_id);
        let owner = Address::random(&env);

        client.set_notification_preference(&owner, false);

        assert!(!client.get_notification_preference(&owner));
    }
}

use soroban_sdk::{contracttype, testutils::Address as _, Address, Env, Symbol, Vec};

#[derive(Clone, Debug)]
#[contracttype]
pub struct TransactionEvent {
    pub tx_id: u64,
    pub from: Address,
    pub to: Address,
    pub amount: i128,
    pub timestamp: u64,
    pub category: Symbol,
    pub currency: Symbol,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct CategorySpendWindow {
    pub category: Symbol,
    pub total_volume: i128,
    pub tx_count: u32,
    pub currency: Symbol,
}

fn aggregate_by_category_window(
    events: &Vec<TransactionEvent>,
    window_start: u64,
    window_end: u64,
) -> Vec<CategorySpendWindow> {
    use soroban_sdk::Map;
    let mut category_map: Map<Symbol, (i128, u32, Symbol)> = Map::new(events.env());
    for event in events.iter() {
        if event.timestamp >= window_start && event.timestamp <= window_end {
            let entry = category_map.get(event.category.clone());
            let (vol, count, currency) = match entry {
                Some((v, c, cur)) => (v, c, cur),
                None => (0i128, 0u32, event.currency.clone()),
            };
            category_map.set(
                event.category.clone(),
                (
                    vol.checked_add(event.amount).unwrap_or(vol),
                    count.checked_add(1).unwrap_or(count),
                    currency,
                ),
            );
        }
    }
    let mut results: Vec<CategorySpendWindow> = Vec::new(events.env());
    for (category, (total_volume, tx_count, currency)) in category_map.iter() {
        results.push_back(CategorySpendWindow {
            category,
            total_volume,
            tx_count,
            currency,
        });
    }
    results
}

fn recategorize_event(
    events: &mut Vec<TransactionEvent>,
    tx_id: u64,
    new_category: Symbol,
) -> bool {
    let mut updated = Vec::new(events.env());
    let mut found = false;
    for event in events.iter() {
        if event.tx_id == tx_id {
            updated.push_back(TransactionEvent {
                tx_id: event.tx_id,
                from: event.from.clone(),
                to: event.to.clone(),
                amount: event.amount,
                timestamp: event.timestamp,
                category: new_category.clone(),
                currency: event.currency.clone(),
            });
            found = true;
        } else {
            updated.push_back(event);
        }
    }
    *events = updated;
    found
}

#[test]
fn test_canonical_events_aggregate_consistently() {
    let env = Env::default();
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let recipient = Address::generate(&env);

    let mut events: Vec<TransactionEvent> = Vec::new(&env);

    // Emit 5 transactions across 2 categories
    events.push_back(TransactionEvent {
        tx_id: 1,
        from: user1.clone(),
        to: recipient.clone(),
        amount: 100,
        timestamp: 1000,
        category: Symbol::new(&env, "food"),
        currency: Symbol::new(&env, "XLM"),
    });
    events.push_back(TransactionEvent {
        tx_id: 2,
        from: user1.clone(),
        to: recipient.clone(),
        amount: 200,
        timestamp: 1001,
        category: Symbol::new(&env, "food"),
        currency: Symbol::new(&env, "XLM"),
    });
    events.push_back(TransactionEvent {
        tx_id: 3,
        from: user2.clone(),
        to: recipient.clone(),
        amount: 300,
        timestamp: 1002,
        category: Symbol::new(&env, "transport"),
        currency: Symbol::new(&env, "XLM"),
    });
    events.push_back(TransactionEvent {
        tx_id: 4,
        from: user2.clone(),
        to: recipient.clone(),
        amount: 400,
        timestamp: 1003,
        category: Symbol::new(&env, "transport"),
        currency: Symbol::new(&env, "XLM"),
    });
    events.push_back(TransactionEvent {
        tx_id: 5,
        from: user1.clone(),
        to: recipient.clone(),
        amount: 500,
        timestamp: 1004,
        category: Symbol::new(&env, "food"),
        currency: Symbol::new(&env, "XLM"),
    });

    // Query spending in window [1000, 1005]
    let results = aggregate_by_category_window(&events, 1000, 1005);

    // Food: 100 + 200 + 500 = 800, 3 txs
    // Transport: 300 + 400 = 700, 2 txs
    let mut food_vol: i128 = 0;
    let mut food_count: u32 = 0;
    let mut transport_vol: i128 = 0;
    let mut transport_count: u32 = 0;

    for r in results.iter() {
        let cat: Symbol = r.category.clone();
        if cat == Symbol::new(&env, "food") {
            food_vol = r.total_volume;
            food_count = r.tx_count;
        } else if cat == Symbol::new(&env, "transport") {
            transport_vol = r.total_volume;
            transport_count = r.tx_count;
        }
    }

    assert_eq!(food_vol, 800);
    assert_eq!(food_count, 3);
    assert_eq!(transport_vol, 700);
    assert_eq!(transport_count, 2);
}

#[test]
fn test_recategorize_updates_consistently_across_views() {
    let env = Env::default();
    let user1 = Address::generate(&env);
    let recipient = Address::generate(&env);

    let mut events: Vec<TransactionEvent> = Vec::new(&env);
    events.push_back(TransactionEvent {
        tx_id: 1,
        from: user1.clone(),
        to: recipient.clone(),
        amount: 100,
        timestamp: 1000,
        category: Symbol::new(&env, "food"),
        currency: Symbol::new(&env, "XLM"),
    });

    // Before recategorization: food has 100
    let before = aggregate_by_category_window(&events, 0, u64::MAX);
    for r in before.iter() {
        if r.category == Symbol::new(&env, "food") {
            assert_eq!(r.total_volume, 100);
        }
    }

    // Recategorize tx 1 from "food" to "transport"
    recategorize_event(&mut events, 1, Symbol::new(&env, "transport"));

    // After recategorization: food is 0, transport is 100
    let after = aggregate_by_category_window(&events, 0, u64::MAX);
    for r in after.iter() {
        let cat = r.category.clone();
        if cat == Symbol::new(&env, "food") {
            assert_eq!(r.total_volume, 0);
        }
        if cat == Symbol::new(&env, "transport") {
            assert_eq!(r.total_volume, 100);
        }
    }
}

#[test]
fn test_windowed_aggregation_bounded_cost() {
    let env = Env::default();
    let user1 = Address::generate(&env);
    let recipient = Address::generate(&env);

    // Create events with timestamps spanning different windows
    let mut events: Vec<TransactionEvent> = Vec::new(&env);
    events.push_back(TransactionEvent {
        tx_id: 1,
        from: user1.clone(),
        to: recipient.clone(),
        amount: 100,
        timestamp: 100,
        category: Symbol::new(&env, "food"),
        currency: Symbol::new(&env, "XLM"),
    });
    events.push_back(TransactionEvent {
        tx_id: 2,
        from: user1.clone(),
        to: recipient.clone(),
        amount: 200,
        timestamp: 200,
        category: Symbol::new(&env, "transport"),
        currency: Symbol::new(&env, "XLM"),
    });
    events.push_back(TransactionEvent {
        tx_id: 3,
        from: user1.clone(),
        to: recipient.clone(),
        amount: 300,
        timestamp: 300,
        category: Symbol::new(&env, "food"),
        currency: Symbol::new(&env, "XLM"),
    });
    events.push_back(TransactionEvent {
        tx_id: 4,
        from: user1.clone(),
        to: recipient.clone(),
        amount: 400,
        timestamp: 400,
        category: Symbol::new(&env, "transport"),
        currency: Symbol::new(&env, "XLM"),
    });

    // Window [150, 350] should include events 2 and 3
    let window = aggregate_by_category_window(&events, 150, 350);
    let mut total: i128 = 0;
    let mut count: u32 = 0;
    for r in window.iter() {
        total = total.checked_add(r.total_volume).unwrap();
        count = count.checked_add(r.tx_count).unwrap();
    }
    assert_eq!(total, 500); // 200 + 300
    assert_eq!(count, 2);

    // Window [0, 500] should include all 4
    let full = aggregate_by_category_window(&events, 0, 500);
    let mut full_total: i128 = 0;
    let mut full_count: u32 = 0;
    for r in full.iter() {
        full_total = full_total.checked_add(r.total_volume).unwrap();
        full_count = full_count.checked_add(r.tx_count).unwrap();
    }
    assert_eq!(full_total, 1000);
    assert_eq!(full_count, 4);
}

#[test]
fn get_goal_progress_bps_returns_zero_for_missing_goal() {
    let env = Env::default();
    let contract_id = env.register(GoalsContract, ());
    let client = GoalsContractClient::new(&env, &contract_id);

    assert_eq!(client.get_goal_progress_bps(&99999), 0);
}

#[test]
fn get_goal_progress_bps_computes_basis_points() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let contract_id = env.register(GoalsContract, ());
    let client = GoalsContractClient::new(&env, &contract_id);

    let goal_id = client.create_goal(&owner, &String::from_str(&env, "Savings"), &10000, &1);

    assert_eq!(client.get_goal_progress_bps(&goal_id), 0);
    assert!(client.get_goal_progress_bps(&goal_id) <= 10000);
}

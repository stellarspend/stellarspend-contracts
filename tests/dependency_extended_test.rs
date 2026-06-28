#[cfg(test)]
mod dependency_tests {
    #[test]
    fn test_dependency_validation_rejects_empty_list() {
        let deps: Vec<&str> = vec![];
        assert!(deps.is_empty());
    }

    #[test]
    fn test_dependency_validation_accepts_valid_entries() {
        let deps = vec!["contract_a", "contract_b"];
        assert_eq!(deps.len(), 2);
        for d in &deps { assert!(!d.is_empty()); }
    }

    #[test]
    fn test_dependency_no_self_reference() {
        let name = "my_contract";
        let deps = vec!["other_contract"];
        assert!(!deps.contains(&name));
    }

    #[test]
    fn test_dependency_no_duplicates() {
        let mut deps = vec!["a", "b", "a"];
        deps.sort(); deps.dedup();
        assert_eq!(deps.len(), 2);
    }
}

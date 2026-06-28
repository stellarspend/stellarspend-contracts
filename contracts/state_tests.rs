#[cfg(test)]
mod tests {
    #[derive(Debug, PartialEq, Default)]
    struct ContractState {
        initialized: bool,
        version: u32,
        owner: String,
    }

    #[test]
    fn test_state_default_not_initialized() {
        let s = ContractState::default();
        assert!(!s.initialized);
        assert_eq!(s.version, 0);
    }

    #[test]
    fn test_state_initialization() {
        let s = ContractState { initialized: true, version: 1, owner: "GABC".into() };
        assert!(s.initialized);
        assert_eq!(s.version, 1);
        assert_eq!(s.owner, "GABC");
    }

    #[test]
    fn test_state_update_version() {
        let mut s = ContractState { initialized: true, version: 1, owner: "GABC".into() };
        s.version = 2;
        assert_eq!(s.version, 2);
    }

    #[test]
    fn test_state_owner_read() {
        let s = ContractState { initialized: true, version: 1, owner: "GXYZ".into() };
        assert_eq!(s.owner.len(), 4);
    }
}

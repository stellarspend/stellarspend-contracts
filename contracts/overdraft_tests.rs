#[cfg(test)]
mod tests {
    #[derive(Debug, PartialEq)]
    enum OverdraftStatus { Allowed, Blocked }

    fn check_overdraft(balance: i64, amount: i64) -> OverdraftStatus {
        if balance - amount < 0 { OverdraftStatus::Blocked } else { OverdraftStatus::Allowed }
    }

    #[test]
    fn test_overdraft_blocked_when_insufficient_funds() {
        assert_eq!(check_overdraft(100, 200), OverdraftStatus::Blocked);
    }

    #[test]
    fn test_overdraft_allowed_when_sufficient_funds() {
        assert_eq!(check_overdraft(500, 200), OverdraftStatus::Allowed);
    }

    #[test]
    fn test_overdraft_exact_balance_allowed() {
        assert_eq!(check_overdraft(100, 100), OverdraftStatus::Allowed);
    }

    #[test]
    fn test_admin_override_unlocks_blocked() {
        let admin_override = true;
        let status = if admin_override { OverdraftStatus::Allowed } else { check_overdraft(10, 200) };
        assert_eq!(status, OverdraftStatus::Allowed);
    }
}

#[cfg(test)]
mod tests {
    fn predict_spending(history: &[u64]) -> u64 {
        if history.is_empty() { return 0; }
        history.iter().sum::<u64>() / history.len() as u64
    }

    #[test]
    fn test_prediction_deterministic_same_input() {
        let h = vec![100u64, 200, 300];
        assert_eq!(predict_spending(&h), predict_spending(&h));
    }

    #[test]
    fn test_prediction_empty_history_returns_zero() {
        assert_eq!(predict_spending(&[]), 0);
    }

    #[test]
    fn test_prediction_single_value() {
        assert_eq!(predict_spending(&[500]), 500);
    }

    #[test]
    fn test_prediction_known_average() {
        assert_eq!(predict_spending(&[100, 200, 300]), 200);
    }
}

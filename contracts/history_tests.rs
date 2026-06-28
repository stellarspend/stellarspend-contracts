#[cfg(test)]
mod tests {
    struct HistoryLog { entries: Vec<String> }

    impl HistoryLog {
        fn new() -> Self { Self { entries: vec![] } }
        fn append(&mut self, entry: &str) { self.entries.push(entry.to_string()); }
        fn read(&self, idx: usize) -> Option<&str> { self.entries.get(idx).map(|s| s.as_str()) }
        fn paginate(&self, page: usize, size: usize) -> Vec<&str> {
            self.entries.iter().skip(page * size).take(size).map(|s| s.as_str()).collect()
        }
    }

    #[test]
    fn test_history_append() {
        let mut log = HistoryLog::new();
        log.append("tx_001"); log.append("tx_002");
        assert_eq!(log.entries.len(), 2);
    }

    #[test]
    fn test_history_read_by_index() {
        let mut log = HistoryLog::new();
        log.append("tx_abc");
        assert_eq!(log.read(0), Some("tx_abc"));
        assert_eq!(log.read(99), None);
    }

    #[test]
    fn test_history_pagination() {
        let mut log = HistoryLog::new();
        for i in 0..10 { log.append(&format!("tx_{}", i)); }
        let page = log.paginate(1, 3);
        assert_eq!(page, vec!["tx_3", "tx_4", "tx_5"]);
    }
}

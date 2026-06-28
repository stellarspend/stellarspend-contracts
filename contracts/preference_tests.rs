#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    #[test]
    fn test_preference_set_value() {
        let mut prefs: HashMap<&str, &str> = HashMap::new();
        prefs.insert("theme", "dark");
        assert_eq!(prefs["theme"], "dark");
    }

    #[test]
    fn test_preference_get_missing_returns_none() {
        let prefs: HashMap<&str, &str> = HashMap::new();
        assert!(prefs.get("notifications").is_none());
    }

    #[test]
    fn test_preference_update_overwrites() {
        let mut prefs: HashMap<&str, &str> = HashMap::new();
        prefs.insert("theme", "light");
        prefs.insert("theme", "dark");
        assert_eq!(prefs["theme"], "dark");
    }

    #[test]
    fn test_preference_multiple_keys() {
        let mut prefs: HashMap<&str, &str> = HashMap::new();
        prefs.insert("theme", "dark");
        prefs.insert("lang", "en");
        assert_eq!(prefs.len(), 2);
    }
}

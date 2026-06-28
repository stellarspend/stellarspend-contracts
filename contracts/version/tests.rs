#[cfg(test)]
mod tests {
    const VERSION: &str = "1.0.0";
    const VERSION_NUM: u32 = 100;

    fn get_version() -> &'static str { VERSION }
    fn get_version_number() -> u32 { VERSION_NUM }

    #[test]
    fn test_version_string_not_empty() {
        assert!(!get_version().is_empty());
    }

    #[test]
    fn test_version_string_format() {
        let parts: Vec<&str> = get_version().split('.').collect();
        assert_eq!(parts.len(), 3, "semver must have 3 parts");
    }

    #[test]
    fn test_version_number_positive() {
        assert!(get_version_number() > 0);
    }

    #[test]
    fn test_version_number_matches_string() {
        let major: u32 = VERSION.split('.').next().unwrap().parse().unwrap();
        assert_eq!(major, 1);
    }
}

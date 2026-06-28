#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    type MetaStore = HashMap<String, String>;

    fn meta_create(store: &mut MetaStore, k: &str, v: &str) { store.insert(k.into(), v.into()); }
    fn meta_read(store: &MetaStore, k: &str) -> Option<String> { store.get(k).cloned() }
    fn meta_update(store: &mut MetaStore, k: &str, v: &str) { store.insert(k.into(), v.into()); }
    fn meta_delete(store: &mut MetaStore, k: &str) { store.remove(k); }

    #[test]
    fn test_metadata_create() {
        let mut store = MetaStore::new();
        meta_create(&mut store, "name", "StellarSpend");
        assert_eq!(meta_read(&store, "name"), Some("StellarSpend".into()));
    }

    #[test]
    fn test_metadata_read_missing() {
        let store = MetaStore::new();
        assert_eq!(meta_read(&store, "missing"), None);
    }

    #[test]
    fn test_metadata_update() {
        let mut store = MetaStore::new();
        meta_create(&mut store, "ver", "1"); meta_update(&mut store, "ver", "2");
        assert_eq!(meta_read(&store, "ver"), Some("2".into()));
    }

    #[test]
    fn test_metadata_delete() {
        let mut store = MetaStore::new();
        meta_create(&mut store, "tmp", "val"); meta_delete(&mut store, "tmp");
        assert_eq!(meta_read(&store, "tmp"), None);
    }
}

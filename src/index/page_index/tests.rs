#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::index::page_index::constants::MAX_NODE_PAGE_INDEX_RECORD_COUNT;
    use crate::index::page_index::node_page_index_store::NodePageIndexStore;
    use crate::storage::models::PageId;
    use crate::storage::page_store_disk::RegularPageStore;
    use crate::storage::storage_manager::StorageManager;

    fn tmp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(name)
    }

    fn cleanup(path: &PathBuf) {
        let _ = std::fs::remove_file(path);
    }

    // Test 1 — single insert and lookup round-trip
    #[test]
    fn test_insert_and_lookup() {
        let path = tmp_path("test_index_insert_lookup.nxr");
        cleanup(&path);

        let store = RegularPageStore::create(&path).unwrap();
        let mut manager = StorageManager::from_page_store(store).unwrap();
        let mut index = NodePageIndexStore::new(&mut manager);

        index.insert(0, PageId(42)).unwrap();
        assert_eq!(index.lookup(0).unwrap(), PageId(42));

        cleanup(&path);
    }

    // Test 2 — multiple inserts on the same directory page
    #[test]
    fn test_multiple_inserts_single_dir_page() {
        let path = tmp_path("test_index_multi_insert.nxr");
        cleanup(&path);

        let store = RegularPageStore::create(&path).unwrap();
        let mut manager = StorageManager::from_page_store(store).unwrap();
        let mut index = NodePageIndexStore::new(&mut manager);

        for i in 0..10usize {
            index.insert(i, PageId(100 + i as u64)).unwrap();
        }
        for i in 0..10usize {
            assert_eq!(index.lookup(i).unwrap(), PageId(100 + i as u64));
        }

        cleanup(&path);
    }

    // Test 3 — overflow onto a second directory page
    #[test]
    fn test_dir_page_overflow() {
        let path = tmp_path("test_index_dir_overflow.nxr");
        cleanup(&path);

        let store = RegularPageStore::create(&path).unwrap();
        let mut manager = StorageManager::from_page_store(store).unwrap();
        let mut index = NodePageIndexStore::new(&mut manager);

        let count = MAX_NODE_PAGE_INDEX_RECORD_COUNT + 5;
        for i in 0..count {
            index.insert(i, PageId(200 + i as u64)).unwrap();
        }
        for i in 0..count {
            assert_eq!(index.lookup(i).unwrap(), PageId(200 + i as u64));
        }

        cleanup(&path);
    }

    // Test 4 — directory survives close and reopen
    #[test]
    fn test_index_persistence() {
        let path = tmp_path("test_index_persistence.nxr");
        cleanup(&path);

        {
            let store = RegularPageStore::create(&path).unwrap();
            let mut manager = StorageManager::from_page_store(store).unwrap();
            {
                let mut index = NodePageIndexStore::new(&mut manager);
                index.insert(0, PageId(10)).unwrap();
                index.insert(1, PageId(11)).unwrap();
                index.insert(2, PageId(12)).unwrap();
            }
            manager.close().unwrap();
        }

        {
            let store = RegularPageStore::open(&path).unwrap();
            let mut manager = StorageManager::from_page_store(store).unwrap();
            let mut index = NodePageIndexStore::new(&mut manager);
            assert_eq!(index.lookup(0).unwrap(), PageId(10));
            assert_eq!(index.lookup(1).unwrap(), PageId(11));
            assert_eq!(index.lookup(2).unwrap(), PageId(12));
        }

        cleanup(&path);
    }

    // Test 5 — directory spanning two pages survives close and reopen
    #[test]
    fn test_multi_dir_page_persistence() {
        let path = tmp_path("test_index_multi_dir_persistence.nxr");
        cleanup(&path);

        let count = MAX_NODE_PAGE_INDEX_RECORD_COUNT + 3;

        {
            let store = RegularPageStore::create(&path).unwrap();
            let mut manager = StorageManager::from_page_store(store).unwrap();
            {
                let mut index = NodePageIndexStore::new(&mut manager);
                for i in 0..count {
                    index.insert(i, PageId(300 + i as u64)).unwrap();
                }
            }
            manager.close().unwrap();
        }

        {
            let store = RegularPageStore::open(&path).unwrap();
            let mut manager = StorageManager::from_page_store(store).unwrap();
            let mut index = NodePageIndexStore::new(&mut manager);
            for i in 0..count {
                assert_eq!(index.lookup(i).unwrap(), PageId(300 + i as u64));
            }
        }

        cleanup(&path);
    }
}

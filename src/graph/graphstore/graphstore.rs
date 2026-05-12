use crate::graph::label::graph_label_store::LabelStore;
use crate::graph::node::graph_node_store::GraphNodeStore;
use crate::graph::record::types::PackedPtr;
use crate::storage::page_store::PageStore;
use crate::storage::storage_manager::StorageManager;


pub struct GraphStore<S: PageStore> {
    storage: StorageManager<S>,
}

impl<S: PageStore> GraphStore<S> {
    pub fn new(storage: StorageManager<S>) -> Self {
        GraphStore { storage }
    }

    pub fn insert_node(&mut self, label: &str) -> u64 {
        let label_id = {
            let mut ls = LabelStore::new(&mut self.storage);
            ls.insert_label(label.as_bytes()).unwrap()
        };

        let mut ns = GraphNodeStore::new(&mut self.storage);
        ns.insert_node(label_id, PackedPtr::NULL).unwrap()
    }
}

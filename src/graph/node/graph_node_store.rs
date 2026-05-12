use zerocopy::byteorder::little_endian::U64;
use zerocopy::{FromBytes, IntoBytes};

use crate::storage::storage_manager::StorageManager;
use crate::storage::page_store::PageStore;
use crate::storage::models::PageId;
use crate::storage::constants::{PAGE_SIZE, SENTINEL_PAGE_ID};

use crate::graph::node::page::{GraphNodePage, GraphNodeRecord};
use crate::graph::node::error::NexoraGraphNodeError;

pub struct GraphNodeStore<'a, S: PageStore> {
    storage: &'a mut StorageManager<S>,
}

impl<'a, S: PageStore> GraphNodeStore<'a, S> {
    pub fn new(storage: &'a mut StorageManager<S>) -> Self {
        GraphNodeStore { storage }
    }

    pub fn insert_node(
        &mut self,
        label_id: u32,
        properties: crate::graph::record::types::PackedPtr,
    ) -> Result<u64, NexoraGraphNodeError> {
        let node_id = self.storage.footer.next_node_id.get();
        let node = GraphNodeRecord::new(node_id, label_id, properties);

        let mut page_id_val = self.storage.footer.first_node_page.get();
        while page_id_val != SENTINEL_PAGE_ID {
            let page_id = PageId(page_id_val);
            let mut buf = [0u8; PAGE_SIZE];
            self.storage.store.read_page(page_id, &mut buf, true)?;

            let mut page = *GraphNodePage::ref_from_bytes(&buf[..]).unwrap();

            if page.insert_node(node).is_ok() {
                self.storage.store.write_page(page_id, page.as_bytes().try_into().unwrap(), true)?;
                self.storage.footer.node_count = U64::new(self.storage.footer.node_count.get() + 1);
                self.storage.footer.next_node_id = U64::new(node_id + 1);
                self.storage.mark_footer_dirty();
                return Ok(node_id);
            }

            page_id_val = page.page_header.next_page_id.get();
        }

        self.storage.footer.next_node_id = U64::new(node_id + 1);
        self.storage.mark_footer_dirty();
        self.insert_into_new_page(node, node_id)
    }

    fn insert_into_new_page(&mut self, node: GraphNodeRecord, node_id: u64) -> Result<u64, NexoraGraphNodeError> {
        let new_page_id = self.storage.allocate_page()?;
        let old_first   = self.storage.footer.first_node_page.get();

        let buf = [0u8; PAGE_SIZE];
        let mut page = *GraphNodePage::ref_from_bytes(&buf[..]).unwrap();
        page.init(new_page_id.as_u64(), old_first);
        page.insert_node(node).unwrap();

        self.storage.store.write_page(new_page_id, page.as_bytes().try_into().unwrap(), true)?;
        self.storage.footer.first_node_page = U64::new(new_page_id.as_u64());
        self.storage.footer.node_count      = U64::new(self.storage.footer.node_count.get() + 1);
        self.storage.mark_footer_dirty();

        Ok(node_id)
    }

    pub fn get_node(&mut self, node_id: u64) -> Result<GraphNodeRecord, NexoraGraphNodeError> {
        if node_id >= self.storage.footer.next_node_id.get() {
            return Err(NexoraGraphNodeError::NodeNotFound);
        }

        let mut page_id_val = self.storage.footer.first_node_page.get();

        while page_id_val != SENTINEL_PAGE_ID {
            let mut buf = [0u8; PAGE_SIZE];
            self.storage.store.read_page(PageId(page_id_val), &mut buf, true)?;

            let page = GraphNodePage::ref_from_bytes(&buf[..]).unwrap();

            if let Some(record) = page.get_node(node_id) {
                return Ok(record);
            }

            page_id_val = page.page_header.next_page_id.get();
        }

        Err(NexoraGraphNodeError::NodeNotFound)
    }

    pub fn delete_node(&mut self, node_id: u64) -> Result<(), NexoraGraphNodeError> {
        if node_id >= self.storage.footer.next_node_id.get() {
            return Err(NexoraGraphNodeError::NodeNotFound);
        }

        let mut page_id_val = self.storage.footer.first_node_page.get();

        while page_id_val != SENTINEL_PAGE_ID {
            let page_id = PageId(page_id_val);
            let mut buf = [0u8; PAGE_SIZE];
            self.storage.store.read_page(page_id, &mut buf, true)?;

            let mut page = *GraphNodePage::ref_from_bytes(&buf[..]).unwrap();

            match page.delete_node(node_id) {
                Ok(()) => {
                    self.storage.store.write_page(page_id, page.as_bytes().try_into().unwrap(), true)?;
                    self.storage.footer.node_count = U64::new(self.storage.footer.node_count.get() - 1);
                    self.storage.mark_footer_dirty();
                    return Ok(());
                }
                Err(NexoraGraphNodeError::NodeNotFound) => {
                    page_id_val = page.page_header.next_page_id.get();
                }
                Err(e) => return Err(e),
            }
        }

        Err(NexoraGraphNodeError::NodeNotFound)
    }
}

use zerocopy::byteorder::little_endian::{U64, U32};
use zerocopy::{FromBytes, IntoBytes};

use crate::storage::storage_manager::StorageManager;
use crate::storage::page_store::PageStore;
use crate::storage::models::PageId;
use crate::storage::constants::{PAGE_SIZE, SENTINEL_PAGE_ID};
use crate::storage::error::NexoraStorageError;

use crate::graph::node::page::{GraphNodePage, GraphNodeRecord};
use crate::graph::node::error::NexoraGraphNodeError;
use crate::graph::record::types::{PackedPtr, RecordCursor};

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

            let mut page = *GraphNodePage::ref_from_bytes(&buf[..])
                .map_err(|_| NexoraStorageError::CorruptPage(page_id_val))?;

            if page.insert_node(node).is_ok() {
                self.storage.store.write_page(page_id, page.as_bytes().try_into().expect("GraphNodePage is PAGE_SIZE"), true)?;
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
        let mut page = *GraphNodePage::ref_from_bytes(&buf[..]).expect("zero-initialized PAGE_SIZE buf");
        page.init(new_page_id.as_u64(), old_first);
        page.insert_node(node)?;

        self.storage.store.write_page(new_page_id, page.as_bytes().try_into().expect("GraphNodePage is PAGE_SIZE"), true)?;
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

            let page = GraphNodePage::ref_from_bytes(&buf[..])
                .map_err(|_| NexoraStorageError::CorruptPage(page_id_val))?;

            if let Some(record) = page.get_node(node_id) {
                return Ok(record);
            }

            page_id_val = page.page_header.next_page_id.get();
        }

        Err(NexoraGraphNodeError::NodeNotFound)
    }

    pub fn update_properties(
        &mut self,
        node_id:    u64,
        properties: crate::graph::record::types::PackedPtr,
    ) -> Result<(), NexoraGraphNodeError> {
        if node_id >= self.storage.footer.next_node_id.get() {
            return Err(NexoraGraphNodeError::NodeNotFound);
        }

        let mut page_id_val = self.storage.footer.first_node_page.get();
        while page_id_val != SENTINEL_PAGE_ID {
            let page_id = PageId(page_id_val);
            let mut buf = [0u8; PAGE_SIZE];
            self.storage.store.read_page(page_id, &mut buf, true)?;
            let mut page = *GraphNodePage::ref_from_bytes(&buf[..])
                .map_err(|_| NexoraStorageError::CorruptPage(page_id_val))?;

            let mut bits = page.graph_page_header.occupied.get();
            while bits != 0 {
                let slot = bits.trailing_zeros() as usize;
                if page.graph_node_records[slot].node_id.get() == node_id {
                    page.graph_node_records[slot].properties = properties;
                    self.storage.store.write_page(page_id, page.as_bytes().try_into().expect("GraphNodePage is PAGE_SIZE"), true)?;
                    return Ok(());
                }
                bits &= bits - 1;
            }

            page_id_val = page.page_header.next_page_id.get();
        }

        Err(NexoraGraphNodeError::NodeNotFound)
    }

    pub fn scan_cursor(&mut self) -> Result<RecordCursor, NexoraGraphNodeError> {
        let first_page = self.storage.footer.first_node_page.get();
        if first_page == SENTINEL_PAGE_ID {
            return Ok(RecordCursor::new(PackedPtr::NULL));
        }
        Ok(RecordCursor::new(PackedPtr::new(first_page, 0)))
    }

    pub fn cursor_next_node(
        &mut self,
        cursor: &mut RecordCursor,
    ) -> Result<Option<GraphNodeRecord>, NexoraGraphNodeError> {
        loop {
            if cursor.is_done() {
                return Ok(None);
            }

            let page_id_val = cursor.ptr().page_id();
            let slot_start  = cursor.ptr().slot() as usize;

            let mut buf = [0u8; PAGE_SIZE];
            self.storage.store.read_page(PageId(page_id_val), &mut buf, true)?;
            let page = *GraphNodePage::ref_from_bytes(&buf[..])
                .map_err(|_| NexoraStorageError::CorruptPage(page_id_val))?;

            let occupied = page.graph_page_header.occupied.get();
            let mask     = if slot_start == 0 { 0u128 } else { (1u128 << slot_start) - 1 };
            let bits     = occupied & !mask;

            if bits != 0 {
                let slot   = bits.trailing_zeros() as usize;
                let record = page.graph_node_records[slot];

                let remaining = bits & (bits - 1);
                let next_ptr = if remaining != 0 {
                    PackedPtr::new(page_id_val, remaining.trailing_zeros() as u8)
                } else {
                    let next_page = page.page_header.next_page_id.get();
                    if next_page == SENTINEL_PAGE_ID { PackedPtr::NULL } else { PackedPtr::new(next_page, 0) }
                };
                cursor.advance_to(next_ptr);
                return Ok(Some(record));
            } else {
                let next_page = page.page_header.next_page_id.get();
                if next_page == SENTINEL_PAGE_ID {
                    cursor.advance_to(PackedPtr::NULL);
                    return Ok(None);
                }
                cursor.advance_to(PackedPtr::new(next_page, 0));
            }
        }
    }

    pub fn update_label(
        &mut self,
        node_id:  u64,
        label_id: u32,
    ) -> Result<(), NexoraGraphNodeError> {
        if node_id >= self.storage.footer.next_node_id.get() {
            return Err(NexoraGraphNodeError::NodeNotFound);
        }

        let mut page_id_val = self.storage.footer.first_node_page.get();
        while page_id_val != SENTINEL_PAGE_ID {
            let page_id = PageId(page_id_val);
            let mut buf = [0u8; PAGE_SIZE];
            self.storage.store.read_page(page_id, &mut buf, true)?;
            let mut page = *GraphNodePage::ref_from_bytes(&buf[..])
                .map_err(|_| NexoraStorageError::CorruptPage(page_id_val))?;

            let mut bits = page.graph_page_header.occupied.get();
            while bits != 0 {
                let slot = bits.trailing_zeros() as usize;
                if page.graph_node_records[slot].node_id.get() == node_id {
                    page.graph_node_records[slot].label_id = U32::new(label_id);
                    self.storage.store.write_page(page_id, page.as_bytes().try_into().expect("GraphNodePage is PAGE_SIZE"), true)?;
                    return Ok(());
                }
                bits &= bits - 1;
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

            let mut page = *GraphNodePage::ref_from_bytes(&buf[..])
                .map_err(|_| NexoraStorageError::CorruptPage(page_id_val))?;

            match page.delete_node(node_id) {
                Ok(()) => {
                    self.storage.store.write_page(page_id, page.as_bytes().try_into().expect("GraphNodePage is PAGE_SIZE"), true)?;
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

use zerocopy::byteorder::little_endian::U64;
use zerocopy::{FromBytes, IntoBytes};

use crate::storage::storage_manager::StorageManager;
use crate::storage::page_store::PageStore;
use crate::storage::models::PageId;
use crate::storage::constants::{PAGE_SIZE, SENTINEL_PAGE_ID};
use crate::storage::error::NexoraStorageError;

use crate::graph::node::page::{GraphNodePage, GraphNodeRecord};
use crate::graph::node::constants::MAX_RECORD_COUNT;
use crate::graph::node::error::NexoraGraphNodeError;
use crate::graph::record::types::{PackedPtr, Record, RecordCursor};
use crate::index::page_index::node_page_index_store::NodePageIndexStore;

pub struct GraphNodeStore<'a, S: PageStore> {
    storage: &'a mut StorageManager<S>,
}

impl<'a, S: PageStore> GraphNodeStore<'a, S> {
    pub fn new(storage: &'a mut StorageManager<S>) -> Self {
        GraphNodeStore { storage }
    }

    fn node_page_id(&mut self, node_id: u64) -> Result<PageId, NexoraGraphNodeError> {
        let page_index = (node_id / MAX_RECORD_COUNT as u64) as usize;
        NodePageIndexStore::new(self.storage).lookup(page_index).map_err(NexoraGraphNodeError::Storage)
    }

    pub fn insert_node(
        &mut self,
        label_id: u32,
        properties: PackedPtr,
    ) -> Result<u64, NexoraGraphNodeError> {
        let node_id = self.storage.footer.next_node_id.get();
        let node    = GraphNodeRecord::new(node_id, label_id, properties);

        // Allocate a new page at every page boundary (including the very first insert).
        if node_id % MAX_RECORD_COUNT as u64 == 0 {
            self.storage.footer.next_node_id = U64::new(node_id + 1);
            self.storage.mark_footer_dirty();
            return self.insert_into_new_page(node, node_id);
        }

        let last_page_id = PageId(self.storage.footer.last_node_page.get());
        let mut buf = [0u8; PAGE_SIZE];
        self.storage.store.read_page(last_page_id, &mut buf, true)?;
        let mut page = *GraphNodePage::ref_from_bytes(&buf[..])
            .map_err(|_| NexoraStorageError::CorruptPage(last_page_id.as_u64()))?;

        page.insert_node(node)?;
        self.storage.store.write_page(last_page_id, page.as_bytes().try_into().expect("GraphNodePage is PAGE_SIZE"), true)?;
        self.storage.footer.node_count   = U64::new(self.storage.footer.node_count.get() + 1);
        self.storage.footer.next_node_id = U64::new(node_id + 1);
        self.storage.mark_footer_dirty();
        Ok(node_id)
    }

    fn insert_into_new_page(&mut self, node: GraphNodeRecord, node_id: u64) -> Result<u64, NexoraGraphNodeError> {
        let new_page_id = self.storage.allocate_page()?;
        let old_last    = self.storage.footer.last_node_page.get();

        let buf = [0u8; PAGE_SIZE];
        let mut page = *GraphNodePage::ref_from_bytes(&buf[..]).expect("zero-initialized PAGE_SIZE buf");
        page.init(new_page_id.as_u64(), SENTINEL_PAGE_ID);
        page.insert_node(node)?;
        self.storage.store.write_page(new_page_id, page.as_bytes().try_into().expect("GraphNodePage is PAGE_SIZE"), true)?;

        if old_last == SENTINEL_PAGE_ID {
            self.storage.footer.first_node_page = U64::new(new_page_id.as_u64());
        } else {
            // Link previous last page → new page.
            let mut old_buf = [0u8; PAGE_SIZE];
            self.storage.store.read_page(PageId(old_last), &mut old_buf, false)?;
            let mut old_page = *GraphNodePage::ref_from_bytes(&old_buf[..])
                .map_err(|_| NexoraStorageError::CorruptPage(old_last))?;
            old_page.page_header.next_page_id = U64::new(new_page_id.as_u64());
            self.storage.store.write_page(PageId(old_last), old_page.as_bytes().try_into().expect("GraphNodePage is PAGE_SIZE"), true)?;
        }

        let page_index = (node_id / MAX_RECORD_COUNT as u64) as usize;
        NodePageIndexStore::new(self.storage).insert(page_index, new_page_id)?;

        self.storage.footer.last_node_page = U64::new(new_page_id.as_u64());
        self.storage.footer.node_count     = U64::new(self.storage.footer.node_count.get() + 1);
        self.storage.mark_footer_dirty();
        Ok(node_id)
    }

    pub fn get_node(&mut self, node_id: u64) -> Result<GraphNodeRecord, NexoraGraphNodeError> {
        if node_id >= self.storage.footer.next_node_id.get() {
            return Err(NexoraGraphNodeError::NodeNotFound);
        }
        let page_id = self.node_page_id(node_id)?;
        let mut buf = [0u8; PAGE_SIZE];
        self.storage.store.read_page(page_id, &mut buf, true)?;
        let page = GraphNodePage::ref_from_bytes(&buf[..])
            .map_err(|_| NexoraStorageError::CorruptPage(page_id.as_u64()))?;
        page.get_node(node_id).ok_or(NexoraGraphNodeError::NodeNotFound)
    }

    pub fn update_properties(
        &mut self,
        node_id:    u64,
        properties: PackedPtr,
    ) -> Result<(), NexoraGraphNodeError> {
        if node_id >= self.storage.footer.next_node_id.get() {
            return Err(NexoraGraphNodeError::NodeNotFound);
        }
        let page_id = self.node_page_id(node_id)?;
        let slot    = (node_id % MAX_RECORD_COUNT as u64) as usize;
        let mut buf = [0u8; PAGE_SIZE];
        self.storage.store.read_page(page_id, &mut buf, true)?;
        let mut page = *GraphNodePage::ref_from_bytes(&buf[..])
            .map_err(|_| NexoraStorageError::CorruptPage(page_id.as_u64()))?;
        if !page.graph_node_records[slot].is_active() {
            return Err(NexoraGraphNodeError::NodeNotFound);
        }
        page.graph_node_records[slot].properties = properties;
        self.storage.store.write_page(page_id, page.as_bytes().try_into().expect("GraphNodePage is PAGE_SIZE"), true)?;
        Ok(())
    }

    pub fn update_label(
        &mut self,
        node_id:  u64,
        label_id: u32,
    ) -> Result<(), NexoraGraphNodeError> {
        use zerocopy::byteorder::little_endian::U32;
        if node_id >= self.storage.footer.next_node_id.get() {
            return Err(NexoraGraphNodeError::NodeNotFound);
        }
        let page_id = self.node_page_id(node_id)?;
        let slot    = (node_id % MAX_RECORD_COUNT as u64) as usize;
        let mut buf = [0u8; PAGE_SIZE];
        self.storage.store.read_page(page_id, &mut buf, true)?;
        let mut page = *GraphNodePage::ref_from_bytes(&buf[..])
            .map_err(|_| NexoraStorageError::CorruptPage(page_id.as_u64()))?;
        if !page.graph_node_records[slot].is_active() {
            return Err(NexoraGraphNodeError::NodeNotFound);
        }
        page.graph_node_records[slot].label_id = U32::new(label_id);
        self.storage.store.write_page(page_id, page.as_bytes().try_into().expect("GraphNodePage is PAGE_SIZE"), true)?;
        Ok(())
    }

    pub fn update_out_edge(&mut self, node_id: u64, ptr: PackedPtr) -> Result<(), NexoraGraphNodeError> {
        let page_id = self.node_page_id(node_id)?;
        let slot    = (node_id % MAX_RECORD_COUNT as u64) as usize;
        let mut buf = [0u8; PAGE_SIZE];
        self.storage.store.read_page(page_id, &mut buf, true)?;
        let mut page = *GraphNodePage::ref_from_bytes(&buf[..])
            .map_err(|_| NexoraStorageError::CorruptPage(page_id.as_u64()))?;
        if !page.graph_node_records[slot].is_active() {
            return Err(NexoraGraphNodeError::NodeNotFound);
        }
        page.graph_node_records[slot].first_out_edge = ptr;
        self.storage.store.write_page(page_id, page.as_bytes().try_into().expect("GraphNodePage is PAGE_SIZE"), true)?;
        Ok(())
    }

    pub fn update_in_edge(&mut self, node_id: u64, ptr: PackedPtr) -> Result<(), NexoraGraphNodeError> {
        let page_id = self.node_page_id(node_id)?;
        let slot    = (node_id % MAX_RECORD_COUNT as u64) as usize;
        let mut buf = [0u8; PAGE_SIZE];
        self.storage.store.read_page(page_id, &mut buf, true)?;
        let mut page = *GraphNodePage::ref_from_bytes(&buf[..])
            .map_err(|_| NexoraStorageError::CorruptPage(page_id.as_u64()))?;
        if !page.graph_node_records[slot].is_active() {
            return Err(NexoraGraphNodeError::NodeNotFound);
        }
        page.graph_node_records[slot].first_in_edge = ptr;
        self.storage.store.write_page(page_id, page.as_bytes().try_into().expect("GraphNodePage is PAGE_SIZE"), true)?;
        Ok(())
    }

    pub fn delete_node(&mut self, node_id: u64) -> Result<(), NexoraGraphNodeError> {
        if node_id >= self.storage.footer.next_node_id.get() {
            return Err(NexoraGraphNodeError::NodeNotFound);
        }
        let page_id = self.node_page_id(node_id)?;
        let mut buf = [0u8; PAGE_SIZE];
        self.storage.store.read_page(page_id, &mut buf, true)?;
        let mut page = *GraphNodePage::ref_from_bytes(&buf[..])
            .map_err(|_| NexoraStorageError::CorruptPage(page_id.as_u64()))?;
        page.delete_node(node_id)?;
        self.storage.store.write_page(page_id, page.as_bytes().try_into().expect("GraphNodePage is PAGE_SIZE"), true)?;
        self.storage.footer.node_count = U64::new(self.storage.footer.node_count.get() - 1);
        self.storage.mark_footer_dirty();
        Ok(())
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

            let record_count = page.graph_page_header.record_count as usize;

            for slot in slot_start..record_count {
                let record = page.graph_node_records[slot];
                if record.is_active() {
                    let next_ptr = if slot + 1 < record_count {
                        PackedPtr::new(page_id_val, (slot + 1) as u8)
                    } else {
                        let next_page = page.page_header.next_page_id.get();
                        if next_page == SENTINEL_PAGE_ID { PackedPtr::NULL } else { PackedPtr::new(next_page, 0) }
                    };
                    cursor.advance_to(next_ptr);
                    return Ok(Some(record));
                }
            }

            let next_page = page.page_header.next_page_id.get();
            if next_page == SENTINEL_PAGE_ID {
                cursor.advance_to(PackedPtr::NULL);
                return Ok(None);
            }
            cursor.advance_to(PackedPtr::new(next_page, 0));
        }
    }
}

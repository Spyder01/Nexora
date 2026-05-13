use zerocopy::byteorder::little_endian::{U64, F64};
use zerocopy::{FromBytes, IntoBytes};

use crate::storage::storage_manager::StorageManager;
use crate::storage::page_store::PageStore;
use crate::storage::models::PageId;
use crate::storage::constants::{PAGE_SIZE, SENTINEL_PAGE_ID};
use crate::storage::error::NexoraStorageError;

use crate::graph::edge::page::{GraphEdgePage, GraphEdgeRecord};
use crate::graph::edge::error::NexoraGraphEdgeError;
use crate::graph::node::page::GraphNodePage;
use crate::graph::record::types::{PackedPtr, RecordCursor};

pub struct GraphEdgeStore<'a, S: PageStore> {
    storage: &'a mut StorageManager<S>,
}

impl<'a, S: PageStore> GraphEdgeStore<'a, S> {
    pub fn new(storage: &'a mut StorageManager<S>) -> Self {
        GraphEdgeStore { storage }
    }

    pub fn insert_edge(
        &mut self,
        src_node_id: u64,
        dst_node_id: u64,
        label_id: u32,
        weight: f64,
    ) -> Result<u64, NexoraGraphEdgeError> {
        let edge_id = self.storage.footer.next_edge_id.get();

        let (src_page_id, src_slot) = self.find_node_location(src_node_id)?;
        let (dst_page_id, dst_slot) = self.find_node_location(dst_node_id)?;

        let mut src_buf = [0u8; PAGE_SIZE];
        self.storage.store.read_page(src_page_id, &mut src_buf, true)?;
        let mut src_page = *GraphNodePage::ref_from_bytes(&src_buf[..])
            .map_err(|_| NexoraStorageError::CorruptPage(src_page_id.as_u64()))?;

        // Fix 4: keep dst_page alive to avoid re-reading it later
        let mut dst_buf = [0u8; PAGE_SIZE];
        let mut dst_page_opt: Option<GraphNodePage> = None;

        let old_first_out = src_page.graph_node_records[src_slot].first_out_edge;
        let old_first_in = if src_page_id == dst_page_id {
            src_page.graph_node_records[dst_slot].first_in_edge
        } else {
            self.storage.store.read_page(dst_page_id, &mut dst_buf, true)?;
            let dst_page = *GraphNodePage::ref_from_bytes(&dst_buf[..])
                .map_err(|_| NexoraStorageError::CorruptPage(dst_page_id.as_u64()))?;
            let old_in = dst_page.graph_node_records[dst_slot].first_in_edge;
            dst_page_opt = Some(dst_page);
            old_in
        };

        let edge = GraphEdgeRecord::new(edge_id, src_node_id, dst_node_id, label_id, weight, old_first_out, old_first_in);
        let (edge_page_id, slot) = self.insert_into_page(edge)?;
        let edge_ptr = PackedPtr::new(edge_page_id.as_u64(), slot as u8);

        src_page.graph_node_records[src_slot].first_out_edge = edge_ptr;
        if src_page_id == dst_page_id {
            src_page.graph_node_records[dst_slot].first_in_edge = edge_ptr;
            self.storage.store.write_page(src_page_id, src_page.as_bytes().try_into().expect("GraphNodePage is PAGE_SIZE"), true)?;
        } else {
            self.storage.store.write_page(src_page_id, src_page.as_bytes().try_into().expect("GraphNodePage is PAGE_SIZE"), true)?;
            // reuse already-read dst_page — no second read
            let mut dst_page = dst_page_opt.expect("set in else branch above");
            dst_page.graph_node_records[dst_slot].first_in_edge = edge_ptr;
            self.storage.store.write_page(dst_page_id, dst_page.as_bytes().try_into().expect("GraphNodePage is PAGE_SIZE"), true)?;
        }

        self.storage.footer.next_edge_id = U64::new(edge_id + 1);
        self.storage.footer.edge_count   = U64::new(self.storage.footer.edge_count.get() + 1);
        self.storage.mark_footer_dirty();

        Ok(edge_id)
    }

    pub fn get_edge(&mut self, edge_id: u64) -> Result<GraphEdgeRecord, NexoraGraphEdgeError> {
        if edge_id >= self.storage.footer.next_edge_id.get() {
            return Err(NexoraGraphEdgeError::EdgeNotFound);
        }

        let mut page_id_val = self.storage.footer.first_edge_page.get();
        while page_id_val != SENTINEL_PAGE_ID {
            let mut buf = [0u8; PAGE_SIZE];
            self.storage.store.read_page(PageId(page_id_val), &mut buf, true)?;
            let page = GraphEdgePage::ref_from_bytes(&buf[..])
                .map_err(|_| NexoraStorageError::CorruptPage(page_id_val))?;

            if let Some(record) = page.get_edge(edge_id) {
                return Ok(record);
            }

            page_id_val = page.page_header.next_page_id.get();
        }

        Err(NexoraGraphEdgeError::EdgeNotFound)
    }

    // Fix 2: find, delete, and repair chains in one scan — no double pass
    pub fn delete_edge(&mut self, edge_id: u64) -> Result<(), NexoraGraphEdgeError> {
        if edge_id >= self.storage.footer.next_edge_id.get() {
            return Err(NexoraGraphEdgeError::EdgeNotFound);
        }

        let mut page_id_val = self.storage.footer.first_edge_page.get();
        while page_id_val != SENTINEL_PAGE_ID {
            let page_id = PageId(page_id_val);
            let mut buf = [0u8; PAGE_SIZE];
            self.storage.store.read_page(page_id, &mut buf, true)?;
            let mut page = *GraphEdgePage::ref_from_bytes(&buf[..])
                .map_err(|_| NexoraStorageError::CorruptPage(page_id_val))?;

            let mut bits = page.graph_page_header.occupied.get();
            while bits != 0 {
                let slot = bits.trailing_zeros() as usize;
                if page.edge_records[slot].edge_id.get() == edge_id {
                    let edge = page.edge_records[slot];

                    page.delete_slot(slot);
                    self.storage.store.write_page(page_id, page.as_bytes().try_into().expect("GraphEdgePage is PAGE_SIZE"), true)?;

                    self.remove_from_outgoing_chain(edge.src_node_id(), edge_id, edge.next_outgoing)?;
                    self.remove_from_incoming_chain(edge.dst_node_id(), edge_id, edge.next_incoming_address_packed)?;

                    self.storage.footer.edge_count = U64::new(self.storage.footer.edge_count.get() - 1);
                    self.storage.mark_footer_dirty();
                    return Ok(());
                }
                bits &= bits - 1;
            }

            page_id_val = page.page_header.next_page_id.get();
        }

        Err(NexoraGraphEdgeError::EdgeNotFound)
    }

    pub fn outgoing_cursor(&mut self, node_id: u64) -> Result<RecordCursor, NexoraGraphEdgeError> {
        let (page_id, slot) = self.find_node_location(node_id)?;
        let mut buf = [0u8; PAGE_SIZE];
        self.storage.store.read_page(page_id, &mut buf, true)?;
        let node_page = GraphNodePage::ref_from_bytes(&buf[..])
            .map_err(|_| NexoraStorageError::CorruptPage(page_id.as_u64()))?;
        Ok(RecordCursor::new(node_page.graph_node_records[slot].first_out_edge))
    }

    pub fn incoming_cursor(&mut self, node_id: u64) -> Result<RecordCursor, NexoraGraphEdgeError> {
        let (page_id, slot) = self.find_node_location(node_id)?;
        let mut buf = [0u8; PAGE_SIZE];
        self.storage.store.read_page(page_id, &mut buf, true)?;
        let node_page = GraphNodePage::ref_from_bytes(&buf[..])
            .map_err(|_| NexoraStorageError::CorruptPage(page_id.as_u64()))?;
        Ok(RecordCursor::new(node_page.graph_node_records[slot].first_in_edge))
    }

    // Fix 1: combined read+advance — one page read per hop instead of two
    pub fn cursor_next_outgoing(&mut self, cursor: &mut RecordCursor) -> Result<Option<GraphEdgeRecord>, NexoraGraphEdgeError> {
        if cursor.is_done() { return Ok(None); }
        let mut buf = [0u8; PAGE_SIZE];
        self.storage.store.read_page(PageId(cursor.ptr().page_id()), &mut buf, true)?;
        let page = GraphEdgePage::ref_from_bytes(&buf[..])
            .map_err(|_| NexoraStorageError::CorruptPage(cursor.ptr().page_id()))?;
        let edge = page.edge_records[cursor.ptr().slot() as usize];
        cursor.advance_to(edge.next_outgoing);
        Ok(Some(edge))
    }

    pub fn cursor_next_incoming(&mut self, cursor: &mut RecordCursor) -> Result<Option<GraphEdgeRecord>, NexoraGraphEdgeError> {
        if cursor.is_done() { return Ok(None); }
        let mut buf = [0u8; PAGE_SIZE];
        self.storage.store.read_page(PageId(cursor.ptr().page_id()), &mut buf, true)?;
        let page = GraphEdgePage::ref_from_bytes(&buf[..])
            .map_err(|_| NexoraStorageError::CorruptPage(cursor.ptr().page_id()))?;
        let edge = page.edge_records[cursor.ptr().slot() as usize];
        cursor.advance_to(edge.next_incoming_address_packed);
        Ok(Some(edge))
    }

    pub fn update_edge_properties(
        &mut self,
        edge_id:    u64,
        properties: PackedPtr,
    ) -> Result<(), NexoraGraphEdgeError> {
        if edge_id >= self.storage.footer.next_edge_id.get() {
            return Err(NexoraGraphEdgeError::EdgeNotFound);
        }

        let mut page_id_val = self.storage.footer.first_edge_page.get();
        while page_id_val != SENTINEL_PAGE_ID {
            let page_id = PageId(page_id_val);
            let mut buf = [0u8; PAGE_SIZE];
            self.storage.store.read_page(page_id, &mut buf, true)?;
            let mut page = *GraphEdgePage::ref_from_bytes(&buf[..])
                .map_err(|_| NexoraStorageError::CorruptPage(page_id_val))?;

            let mut bits = page.graph_page_header.occupied.get();
            while bits != 0 {
                let slot = bits.trailing_zeros() as usize;
                if page.edge_records[slot].edge_id.get() == edge_id {
                    page.edge_records[slot].set_properties(properties);
                    self.storage.store.write_page(page_id, page.as_bytes().try_into().expect("GraphEdgePage is PAGE_SIZE"), true)?;
                    return Ok(());
                }
                bits &= bits - 1;
            }

            page_id_val = page.page_header.next_page_id.get();
        }

        Err(NexoraGraphEdgeError::EdgeNotFound)
    }

    pub fn scan_cursor(&mut self) -> Result<RecordCursor, NexoraGraphEdgeError> {
        let first_page = self.storage.footer.first_edge_page.get();
        if first_page == SENTINEL_PAGE_ID {
            return Ok(RecordCursor::new(PackedPtr::NULL));
        }
        Ok(RecordCursor::new(PackedPtr::new(first_page, 0)))
    }

    pub fn cursor_next_edge(
        &mut self,
        cursor: &mut RecordCursor,
    ) -> Result<Option<GraphEdgeRecord>, NexoraGraphEdgeError> {
        loop {
            if cursor.is_done() {
                return Ok(None);
            }

            let page_id_val = cursor.ptr().page_id();
            let slot_start  = cursor.ptr().slot() as usize;

            let mut buf = [0u8; PAGE_SIZE];
            self.storage.store.read_page(PageId(page_id_val), &mut buf, true)?;
            let page = *GraphEdgePage::ref_from_bytes(&buf[..])
                .map_err(|_| NexoraStorageError::CorruptPage(page_id_val))?;

            let occupied = page.graph_page_header.occupied.get();
            let mask     = if slot_start == 0 { 0u64 } else { (1u64 << slot_start) - 1 };
            let bits     = occupied & !mask;

            if bits != 0 {
                let slot   = bits.trailing_zeros() as usize;
                let record = page.edge_records[slot];

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

    pub fn update_label_and_weight(
        &mut self,
        edge_id:  u64,
        label_id: u32,
        weight:   f64,
    ) -> Result<(), NexoraGraphEdgeError> {
        if edge_id >= self.storage.footer.next_edge_id.get() {
            return Err(NexoraGraphEdgeError::EdgeNotFound);
        }

        let mut page_id_val = self.storage.footer.first_edge_page.get();
        while page_id_val != SENTINEL_PAGE_ID {
            let page_id = PageId(page_id_val);
            let mut buf = [0u8; PAGE_SIZE];
            self.storage.store.read_page(page_id, &mut buf, true)?;
            let mut page = *GraphEdgePage::ref_from_bytes(&buf[..])
                .map_err(|_| NexoraStorageError::CorruptPage(page_id_val))?;

            let mut bits = page.graph_page_header.occupied.get();
            while bits != 0 {
                let slot = bits.trailing_zeros() as usize;
                if page.edge_records[slot].edge_id.get() == edge_id {
                    page.edge_records[slot].set_label_id(label_id);
                    page.edge_records[slot].weight = F64::new(weight);
                    self.storage.store.write_page(page_id, page.as_bytes().try_into().expect("GraphEdgePage is PAGE_SIZE"), true)?;
                    return Ok(());
                }
                bits &= bits - 1;
            }

            page_id_val = page.page_header.next_page_id.get();
        }

        Err(NexoraGraphEdgeError::EdgeNotFound)
    }

    fn insert_into_page(&mut self, edge: GraphEdgeRecord) -> Result<(PageId, usize), NexoraGraphEdgeError> {
        let mut page_id_val = self.storage.footer.first_edge_page.get();
        while page_id_val != SENTINEL_PAGE_ID {
            let page_id = PageId(page_id_val);
            let mut buf = [0u8; PAGE_SIZE];
            self.storage.store.read_page(page_id, &mut buf, true)?;
            let mut page = *GraphEdgePage::ref_from_bytes(&buf[..])
                .map_err(|_| NexoraStorageError::CorruptPage(page_id_val))?;

            if let Ok(slot) = page.insert_edge(edge) {
                self.storage.store.write_page(page_id, page.as_bytes().try_into().expect("GraphEdgePage is PAGE_SIZE"), true)?;
                return Ok((page_id, slot));
            }

            page_id_val = page.page_header.next_page_id.get();
        }

        self.insert_into_new_page(edge)
    }

    fn insert_into_new_page(&mut self, edge: GraphEdgeRecord) -> Result<(PageId, usize), NexoraGraphEdgeError> {
        let new_page_id = self.storage.allocate_page()?;
        let old_first   = self.storage.footer.first_edge_page.get();

        let buf = [0u8; PAGE_SIZE];
        let mut page = *GraphEdgePage::ref_from_bytes(&buf[..]).expect("zero-initialized PAGE_SIZE buf");
        page.init(new_page_id.as_u64(), old_first);
        let slot = page.insert_edge(edge)?;

        self.storage.store.write_page(new_page_id, page.as_bytes().try_into().expect("GraphEdgePage is PAGE_SIZE"), true)?;
        self.storage.footer.first_edge_page = U64::new(new_page_id.as_u64());
        self.storage.mark_footer_dirty();

        Ok((new_page_id, slot))
    }

    // Fix 3: single read per traversal step, track prev to avoid lookahead read
    fn remove_from_outgoing_chain(
        &mut self,
        src_node_id: u64,
        edge_id: u64,
        next_outgoing: PackedPtr,
    ) -> Result<(), NexoraGraphEdgeError> {
        let (node_page_id, node_slot) = self.find_node_location(src_node_id)?;
        let mut node_buf = [0u8; PAGE_SIZE];
        self.storage.store.read_page(node_page_id, &mut node_buf, true)?;
        let mut node_page = *GraphNodePage::ref_from_bytes(&node_buf[..])
            .map_err(|_| NexoraStorageError::CorruptPage(node_page_id.as_u64()))?;

        let mut ptr = node_page.graph_node_records[node_slot].first_out_edge;
        let mut prev_ptr: Option<PackedPtr> = None;

        while !ptr.is_null() {
            let cur_page_id = PageId(ptr.page_id());
            let mut cur_buf = [0u8; PAGE_SIZE];
            self.storage.store.read_page(cur_page_id, &mut cur_buf, true)?;
            let cur_page = *GraphEdgePage::ref_from_bytes(&cur_buf[..])
                .map_err(|_| NexoraStorageError::CorruptPage(cur_page_id.as_u64()))?;
            let cur_slot = ptr.slot() as usize;

            if cur_page.edge_records[cur_slot].edge_id.get() == edge_id {
                match prev_ptr {
                    None => {
                        node_page.graph_node_records[node_slot].first_out_edge = next_outgoing;
                        self.storage.store.write_page(node_page_id, node_page.as_bytes().try_into().expect("GraphNodePage is PAGE_SIZE"), true)?;
                    }
                    Some(prev) => {
                        let prev_page_id = PageId(prev.page_id());
                        let mut prev_buf = [0u8; PAGE_SIZE];
                        self.storage.store.read_page(prev_page_id, &mut prev_buf, true)?;
                        let mut prev_page = *GraphEdgePage::ref_from_bytes(&prev_buf[..])
                            .map_err(|_| NexoraStorageError::CorruptPage(prev_page_id.as_u64()))?;
                        prev_page.edge_records[prev.slot() as usize].next_outgoing = next_outgoing;
                        self.storage.store.write_page(prev_page_id, prev_page.as_bytes().try_into().expect("GraphEdgePage is PAGE_SIZE"), true)?;
                    }
                }
                return Ok(());
            }

            prev_ptr = Some(ptr);
            ptr = cur_page.edge_records[cur_slot].next_outgoing;
        }

        Err(NexoraGraphEdgeError::EdgeNotFound)
    }

    fn remove_from_incoming_chain(
        &mut self,
        dst_node_id: u64,
        edge_id: u64,
        next_incoming: PackedPtr,
    ) -> Result<(), NexoraGraphEdgeError> {
        let (node_page_id, node_slot) = self.find_node_location(dst_node_id)?;
        let mut node_buf = [0u8; PAGE_SIZE];
        self.storage.store.read_page(node_page_id, &mut node_buf, true)?;
        let mut node_page = *GraphNodePage::ref_from_bytes(&node_buf[..])
            .map_err(|_| NexoraStorageError::CorruptPage(node_page_id.as_u64()))?;

        let mut ptr = node_page.graph_node_records[node_slot].first_in_edge;
        let mut prev_ptr: Option<PackedPtr> = None;

        while !ptr.is_null() {
            let cur_page_id = PageId(ptr.page_id());
            let mut cur_buf = [0u8; PAGE_SIZE];
            self.storage.store.read_page(cur_page_id, &mut cur_buf, true)?;
            let cur_page = *GraphEdgePage::ref_from_bytes(&cur_buf[..])
                .map_err(|_| NexoraStorageError::CorruptPage(cur_page_id.as_u64()))?;
            let cur_slot = ptr.slot() as usize;

            if cur_page.edge_records[cur_slot].edge_id.get() == edge_id {
                match prev_ptr {
                    None => {
                        node_page.graph_node_records[node_slot].first_in_edge = next_incoming;
                        self.storage.store.write_page(node_page_id, node_page.as_bytes().try_into().expect("GraphNodePage is PAGE_SIZE"), true)?;
                    }
                    Some(prev) => {
                        let prev_page_id = PageId(prev.page_id());
                        let mut prev_buf = [0u8; PAGE_SIZE];
                        self.storage.store.read_page(prev_page_id, &mut prev_buf, true)?;
                        let mut prev_page = *GraphEdgePage::ref_from_bytes(&prev_buf[..])
                            .map_err(|_| NexoraStorageError::CorruptPage(prev_page_id.as_u64()))?;
                        prev_page.edge_records[prev.slot() as usize].next_incoming_address_packed = next_incoming;
                        self.storage.store.write_page(prev_page_id, prev_page.as_bytes().try_into().expect("GraphEdgePage is PAGE_SIZE"), true)?;
                    }
                }
                return Ok(());
            }

            prev_ptr = Some(ptr);
            ptr = cur_page.edge_records[cur_slot].next_incoming_address_packed;
        }

        Err(NexoraGraphEdgeError::EdgeNotFound)
    }

    fn find_node_location(&mut self, node_id: u64) -> Result<(PageId, usize), NexoraGraphEdgeError> {
        let mut page_id_val = self.storage.footer.first_node_page.get();
        while page_id_val != SENTINEL_PAGE_ID {
            let page_id = PageId(page_id_val);
            let mut buf = [0u8; PAGE_SIZE];
            self.storage.store.read_page(page_id, &mut buf, true)?;
            let page = GraphNodePage::ref_from_bytes(&buf[..])
                .map_err(|_| NexoraStorageError::CorruptPage(page_id_val))?;

            if node_id >= page.graph_page_header.min_node_id.get()
            && node_id <= page.graph_page_header.max_node_id.get() {
                let mut bits = page.graph_page_header.occupied.get();
                while bits != 0 {
                    let slot = bits.trailing_zeros() as usize;
                    if page.graph_node_records[slot].node_id.get() == node_id {
                        return Ok((page_id, slot));
                    }
                    bits &= bits - 1;
                }
            }

            page_id_val = page.page_header.next_page_id.get();
        }

        Err(NexoraGraphEdgeError::NodeNotFound)
    }
}

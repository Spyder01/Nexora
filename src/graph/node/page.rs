use core::convert::TryFrom;
use core::option::Option::Some;
use core::result::Result;

use zerocopy::{FromBytes, IntoBytes, Immutable, KnownLayout};
use zerocopy::byteorder::little_endian::{U64, U32};

use crate::storage::constants::{PAGE_SIZE, PAGE_HEADER_SIZE, SENTINEL_PAGE_ID};
use crate::storage::models::{NexoraPageHeader, PageType};

use crate::graph::node::constants::{NODE_RECORD_SIZE, MAX_RECORD_COUNT, NODE_PAGE_HEADER_SIZE};
use crate::graph::node::error::NexoraGraphNodeError;

use crate::graph::record::types::{GraphRecordFlag, PackedPtr, Record};


#[derive(Debug, FromBytes, IntoBytes, Immutable, KnownLayout, Copy, Clone)]
#[repr(C)]
pub struct GraphNodePageHeader {
    pub record_count: u8,
    _pad: [u8; NODE_PAGE_HEADER_SIZE - 1],
}

const _: () = assert!(
    std::mem::size_of::<GraphNodePageHeader>() == NODE_PAGE_HEADER_SIZE,
    "GraphNodePageHeader should be exactly NODE_PAGE_HEADER_SIZE."
);


#[derive(Debug, FromBytes, IntoBytes, Immutable, KnownLayout, Copy, Clone)]
#[repr(C)]
pub struct GraphNodeRecord {
    pub node_id:        U64,
    pub first_out_edge: PackedPtr,
    pub first_in_edge:  PackedPtr,
    pub properties:     PackedPtr,
    pub label_id:       U32,
    pub flags:          u8,

    _pad: [u8; 3],
}

const _: () = assert!(
    std::mem::size_of::<GraphNodeRecord>() == NODE_RECORD_SIZE,
    "GraphNodeRecord should be exactly NODE_RECORD_SIZE."
);

impl Record for GraphNodeRecord {
    fn id(&self) -> u64 {
        self.node_id.get()
    }

    fn is_active(&self) -> bool {
        GraphRecordFlag::try_from(self.flags).unwrap_or(GraphRecordFlag::UNCLASSIFIED) == GraphRecordFlag::ACTIVE
    }

    fn mark_deleted(&mut self) {
        self.flags = GraphRecordFlag::DELETED as u8;
    }
}

impl GraphNodeRecord {
    pub fn new(node_id: u64, label_id: u32, properties: PackedPtr) -> Self {
        GraphNodeRecord {
            node_id:        U64::new(node_id),
            first_out_edge: PackedPtr::NULL,
            first_in_edge:  PackedPtr::NULL,
            properties,
            label_id:       U32::new(label_id),
            flags:          GraphRecordFlag::ACTIVE as u8,
            _pad:           [0u8; 3],
        }
    }
}


#[derive(Debug, FromBytes, IntoBytes, Immutable, KnownLayout, Copy, Clone)]
#[repr(C)]
pub struct GraphNodePage {
    pub page_header:       NexoraPageHeader,
    pub graph_page_header: GraphNodePageHeader,
    pub graph_node_records: [GraphNodeRecord; MAX_RECORD_COUNT],
    _pad: [u8; PAGE_SIZE - NODE_PAGE_HEADER_SIZE - PAGE_HEADER_SIZE - MAX_RECORD_COUNT * NODE_RECORD_SIZE],
}

const _: () = assert!(
    std::mem::size_of::<GraphNodePage>() == PAGE_SIZE,
    "GraphNodePage should be exactly one page."
);

impl GraphNodePage {
    pub fn init(&mut self, page_id: u64, next_page_id: u64) {
        self.page_header.page_id      = U64::new(page_id);
        self.page_header.page_type    = PageType::Node as u8;
        self.page_header.next_page_id = U64::new(next_page_id);
        self.page_header.prev_page_id = U64::new(SENTINEL_PAGE_ID);
        self.page_header.checksum     = U32::new(0);
        self.graph_page_header.record_count = 0;
    }

    pub fn insert_node(&mut self, node: GraphNodeRecord) -> Result<(), NexoraGraphNodeError> {
        let slot = self.graph_page_header.record_count as usize;
        if slot >= MAX_RECORD_COUNT {
            return Err(NexoraGraphNodeError::NoFreeSlot);
        }
        self.graph_node_records[slot] = node;
        self.graph_page_header.record_count += 1;
        Ok(())
    }

    pub fn get_node(&self, node_id: u64) -> Option<GraphNodeRecord> {
        let slot = (node_id % MAX_RECORD_COUNT as u64) as usize;
        if slot >= self.graph_page_header.record_count as usize {
            return None;
        }
        let record = self.graph_node_records[slot];
        if !record.is_active() {
            return None;
        }
        Some(record)
    }

    pub fn delete_node(&mut self, node_id: u64) -> Result<(), NexoraGraphNodeError> {
        let slot = (node_id % MAX_RECORD_COUNT as u64) as usize;
        if slot >= self.graph_page_header.record_count as usize
            || !self.graph_node_records[slot].is_active()
        {
            return Err(NexoraGraphNodeError::NodeNotFound);
        }
        self.graph_node_records[slot].flags = GraphRecordFlag::DELETED as u8;
        Ok(())
    }
}

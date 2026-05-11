use zerocopy::{FromBytes, IntoBytes, Immutable, KnownLayout};
use zerocopy::byteorder::little_endian::{U64, U32, U16, F64};

use crate::graph::edge::constants::{EDGE_PAGE_HEADER_SIZE, EDGE_RECORD_SIZE, MAX_EDGE_RECORD_COUNT};
use crate::graph::edge::error::NexoraGraphEdgeError;
use crate::graph::record::types::{RecordHeader, GraphRecordFlag, PackedPtr};

use crate::storage::constants::{PAGE_HEADER_SIZE, PAGE_SIZE, SENTINEL_PAGE_ID};
use crate::storage::models::{NexoraPageHeader, PageType};


#[derive(Debug, FromBytes, IntoBytes, Immutable, KnownLayout, Copy, Clone)]
#[repr(C)]
pub struct GraphEdgePageHeader {
    pub occupied: U64,

    pub record_count: u8,
    _pad:  [u8; EDGE_PAGE_HEADER_SIZE - 8 - 1],
}

const _: () = assert!(
    std::mem::size_of::<GraphEdgePageHeader>() == EDGE_PAGE_HEADER_SIZE,
    "GraphEdgePageHeader should be size of EDGE_PAGE_HEADER_SIZE"
);

impl RecordHeader for GraphEdgePageHeader {
   fn get_next_record_offset(&self) -> u64 {
       return (PAGE_HEADER_SIZE + EDGE_PAGE_HEADER_SIZE) as  u64 + self.record_count as u64 * EDGE_RECORD_SIZE as u64;
   } 

   fn find_free_slot(&mut self) -> Option<usize> {
        let bounds_mask = (1u64 << self.record_count) - 1;
        let deleted = !self.occupied.get() & bounds_mask;

        if deleted != 0 {
          Some(deleted.trailing_zeros() as usize)
        } else if (self.record_count as usize) < MAX_EDGE_RECORD_COUNT {
          Some(self.record_count as usize)
        } else {
          None
        }
   }

  fn occupy_slot(&mut self, slot: usize) {
    self.occupied = U64::new(self.occupied.get() | 1u64 << slot);

    if slot == self.record_count as usize {
        self.record_count += 1;
    }
  }

  fn free_slot(&mut self, slot: usize) {
      self.occupied = U64::new(self.occupied.get() & !(1u64 << slot));
  }
}


#[derive(Debug, FromBytes, IntoBytes, Immutable, KnownLayout, Copy, Clone)]
#[repr(C)]
pub struct GraphEdgeRecord {
    pub edge_id: U64,

    pub weight: F64,
    pub src_node_id: U64,
    pub dst_node_id: U64,
    pub next_outgoing: PackedPtr,
    pub next_incoming_address_packed: PackedPtr,

    property_page_id: U64,
    label_id: U32,
    property_slot: U16,

    flags: u8,

    _pad: [u8; EDGE_RECORD_SIZE - 8 - 8 - 8 - 8 - 8 - 8 - 8 - 4 - 2 - 1],
}

const _: () = assert!(
    std::mem::size_of::<GraphEdgeRecord>() == EDGE_RECORD_SIZE,
    "GraphEdgeRecord should be size of EDGE_RECORD_SIZE"
);

impl GraphEdgeRecord {
    pub fn new(
        edge_id: u64,
        src_node_id: u64,
        dst_node_id: u64,
        label_id: u32,
        weight: f64,
        next_outgoing: PackedPtr,
        next_incoming: PackedPtr,
    ) -> Self {
        GraphEdgeRecord {
            edge_id:                     U64::new(edge_id),
            weight:                      F64::new(weight),
            src_node_id:                 U64::new(src_node_id),
            dst_node_id:                 U64::new(dst_node_id),
            next_outgoing,
            next_incoming_address_packed: next_incoming,
            property_page_id:            U64::new(SENTINEL_PAGE_ID),
            label_id:                    U32::new(label_id),
            property_slot:               U16::new(0),
            flags:                       GraphRecordFlag::ACTIVE as u8,
            _pad:                        [0u8; EDGE_RECORD_SIZE - 8 - 8 - 8 - 8 - 8 - 8 - 8 - 4 - 2 - 1],
        }
    }

    pub fn edge_id(&self) -> u64        { self.edge_id.get() }
    pub fn src_node_id(&self) -> u64    { self.src_node_id.get() }
    pub fn dst_node_id(&self) -> u64    { self.dst_node_id.get() }
    pub fn weight(&self) -> f64         { self.weight.get() }
}


#[derive(Debug, FromBytes, IntoBytes, Immutable, KnownLayout, Copy, Clone)]
#[repr(C)]
pub struct GraphEdgePage {
    pub page_header: NexoraPageHeader,
    pub graph_page_header: GraphEdgePageHeader,
    pub edge_records: [GraphEdgeRecord; MAX_EDGE_RECORD_COUNT],

    _pad: [u8; PAGE_SIZE - PAGE_HEADER_SIZE - EDGE_PAGE_HEADER_SIZE - MAX_EDGE_RECORD_COUNT*EDGE_RECORD_SIZE],
}

const _: () = assert!(
    std::mem::size_of::<GraphEdgePage>() == PAGE_SIZE,
    "GraphEdgePage should be size of PAGE_SIZE"
);

impl GraphEdgePage {
    pub fn init(&mut self, page_id: u64, next_page_id: u64) {
        self.page_header.page_id      = U64::new(page_id);
        self.page_header.page_type    = PageType::Edge as u8;
        self.page_header.next_page_id = U64::new(next_page_id);
        self.page_header.prev_page_id = U64::new(SENTINEL_PAGE_ID);
        self.page_header.checksum     = U32::new(0);
        self.graph_page_header.occupied     = U64::new(0);
        self.graph_page_header.record_count = 0;
    }

    pub fn insert_edge(&mut self, mut edge: GraphEdgeRecord) -> Result<usize, NexoraGraphEdgeError> {
        let slot = self.graph_page_header.find_free_slot()
            .ok_or(NexoraGraphEdgeError::NoFreeSlot)?;

        edge.flags = GraphRecordFlag::ACTIVE as u8;
        self.edge_records[slot] = edge;
        self.graph_page_header.occupy_slot(slot);

        Ok(slot)
    }

    pub fn delete_slot(&mut self, slot: usize) {
        assert!(slot < MAX_EDGE_RECORD_COUNT, "Slot cannot exceed MAX_EDGE_RECORD_COUNT");
        self.edge_records[slot].flags = GraphRecordFlag::DELETED as u8;
        self.graph_page_header.free_slot(slot);
    }

    pub fn get_edge(&self, edge_id: u64) -> Option<GraphEdgeRecord> {
        let mut bits = self.graph_page_header.occupied.get();
        while bits != 0 {
            let slot = bits.trailing_zeros() as usize;
            if self.edge_records[slot].edge_id.get() == edge_id {
                return Some(self.edge_records[slot]);
            }
            bits &= bits - 1;
        }
        None
    }

    pub fn delete_edge(&mut self, edge_id: u64) -> Result<(), NexoraGraphEdgeError> {
        let mut bits = self.graph_page_header.occupied.get();
        while bits != 0 {
            let slot = bits.trailing_zeros() as usize;
            if self.edge_records[slot].edge_id.get() == edge_id {
                self.delete_slot(slot);
                return Ok(());
            }
            bits &= bits - 1;
        }
        Err(NexoraGraphEdgeError::EdgeNotFound)
    }
}


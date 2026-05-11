use core::convert::TryFrom;
use core::option::Option::Some;
use core::result::Result;

use zerocopy::{FromBytes, IntoBytes, Immutable, KnownLayout};
use zerocopy::byteorder::little_endian::{U64, U32, U16, U128};

use crate::storage::constants::{PAGE_SIZE, PAGE_HEADER_SIZE, SENTINEL_PAGE_ID};
use crate::storage::models::{NexoraPageHeader, PageType};

use crate::graph::node::constants::{NODE_RECORD_SIZE, MAX_RECORD_COUNT, NODE_PAGE_HEADER_SIZE};
use crate::graph::node::error::NexoraGraphNodeError;

use crate::graph::record::types::{RecordHeader, GraphRecordFlag, PackedPtr, Record};


#[derive(Debug, FromBytes, IntoBytes, Immutable, KnownLayout, Copy, Clone)]
#[repr(C)]
pub struct GraphNodePageHeader {
    
    pub occupied: U128,
    pub min_node_id:  U64,
    pub max_node_id: U64,

    pub record_count: u8, 

    _pad: [u8; NODE_PAGE_HEADER_SIZE - 16 - 8 - 8 - 1],
}

const _: () = assert!(
    std::mem::size_of::<GraphNodePageHeader>() == NODE_PAGE_HEADER_SIZE,
    "GraphNodePage should be exactly PAGE_HEADER_SIZE."
);

impl GraphNodePageHeader {

  pub fn update_zone_map(&mut self, node_id: u64) {
      if node_id < self.min_node_id.get() {
          self.min_node_id = U64::new(node_id);
      }
      if node_id > self.max_node_id.get() {
          self.max_node_id = U64::new(node_id);
      }
  }

}

impl RecordHeader for GraphNodePageHeader {
    fn get_next_record_offset(&self) -> u64 {
        if self.record_count >= MAX_RECORD_COUNT as u8 {
            return SENTINEL_PAGE_ID;
        }

        return NODE_PAGE_HEADER_SIZE as u64 + PAGE_HEADER_SIZE as u64 + self.record_count as u64 * NODE_RECORD_SIZE as u64;
    }

   fn find_free_slot(&mut self) -> Option<usize> {
        let bounds_mask = (1u128 << self.record_count) - 1;
        let deleted = !self.occupied.get() & bounds_mask;

        if deleted != 0 {
          Some(deleted.trailing_zeros() as usize)
        } else if (self.record_count as usize) < MAX_RECORD_COUNT {
          Some(self.record_count as usize)
        } else {
          None
        }
  }

  fn occupy_slot(&mut self, slot: usize) {
    let mut occupied = self.occupied.get(); 

    occupied |= 1u128 << slot;
    self.occupied = U128::new(occupied);

    if slot == self.record_count as usize {
      self.record_count += 1;
    }
  }

  fn free_slot(&mut self, slot: usize) {
      self.occupied = U128::new(self.occupied.get() & !(1u128 << slot));
  }
    
}


#[derive(Debug, FromBytes, IntoBytes, Immutable, KnownLayout, Copy, Clone)]
#[repr(C)]
pub struct GraphNodeRecord {
    pub node_id: U64,
    pub first_out_edge: PackedPtr,
    pub first_in_edge: PackedPtr,
    property_page_id: U64,
    label_id: U32,
    property_slot: U16,
    flags: u8,

    _pad: u8,
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
    pub fn new(node_id: u64, label_id: u32, property_page_id: u64, property_slot: u16) -> Self {
        GraphNodeRecord {
            node_id:          U64::new(node_id),
            first_out_edge:   PackedPtr::NULL,
            first_in_edge:    PackedPtr::NULL,
            property_page_id: U64::new(property_page_id),
            label_id:         U32::new(label_id),
            property_slot:    U16::new(property_slot),
            flags:            GraphRecordFlag::ACTIVE as u8,
            _pad:             0,
        }
    }
}


#[derive(Debug, FromBytes, IntoBytes, Immutable, KnownLayout, Copy, Clone)]
#[repr(C)]
pub struct GraphNodePage {
    pub page_header: NexoraPageHeader,
    pub graph_page_header: GraphNodePageHeader,
    
    pub graph_node_records: [GraphNodeRecord; MAX_RECORD_COUNT],

    _pad: [u8; PAGE_SIZE - NODE_PAGE_HEADER_SIZE - PAGE_HEADER_SIZE - MAX_RECORD_COUNT*NODE_RECORD_SIZE],
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
        self.graph_page_header.min_node_id = U64::new(u64::MAX);
        self.graph_page_header.max_node_id = U64::new(0);
        self.graph_page_header.occupied    = U128::new(0);
        self.graph_page_header.record_count = 0;
    }

    pub fn insert_node(&mut self, mut node: GraphNodeRecord) -> Result<(), NexoraGraphNodeError>  {
        let slot = self.graph_page_header.find_free_slot()
            .ok_or(NexoraGraphNodeError::NoFreeSlot)?;

        node.flags = GraphRecordFlag::ACTIVE as u8;
        self.graph_node_records[slot] = node;
        self.graph_page_header.occupy_slot(slot);
        self.graph_page_header.update_zone_map(node.node_id.get());

        return Ok(());
    }

    fn delete_slot(&mut self, slot: usize) {
        assert!(slot < MAX_RECORD_COUNT, "Slots cannot be more than MAX_RECORD_COUNT");
        self.graph_node_records[slot].flags = GraphRecordFlag::DELETED as u8; 
        self.graph_page_header.free_slot(slot); 
    }

    pub fn get_node(&self, node_id: u64) -> Option<GraphNodeRecord> {
        if node_id < self.graph_page_header.min_node_id.get()
        || node_id > self.graph_page_header.max_node_id.get() {
            return None;
        }

        let mut bits = self.graph_page_header.occupied.get();
        while bits != 0 {
            let slot = bits.trailing_zeros() as usize;
            if self.graph_node_records[slot].node_id.get() == node_id {
                return Some(self.graph_node_records[slot]);
            }
            bits &= bits - 1;
        }

        None
    }

    pub fn delete_node(&mut self, node_id: u64) -> Result<(), NexoraGraphNodeError> {
        if node_id < self.graph_page_header.min_node_id.get() || node_id > self.graph_page_header.max_node_id.get() {
            return Err(NexoraGraphNodeError::NodeNotFound);
        } 

        if self.graph_page_header.record_count == 1  {
            if self.graph_node_records[0].node_id.get() == node_id {
                self.delete_slot(0);
        
                self.graph_page_header.max_node_id = U64::new(0);
                self.graph_page_header.min_node_id = U64::new(u64::MAX);

                return Ok(());
            } else {
                return Err(NexoraGraphNodeError::NodeNotFound);
            }
        }

        let mut bits = self.graph_page_header.occupied.get();
        let mut min = u64::MAX;
        let mut max: u64 = 0;
        let mut found = false;

        while bits != 0 {
            let slot = bits.trailing_zeros() as usize;
            let curr_id = self.graph_node_records[slot].node_id.get();

            if curr_id == node_id {
               self.delete_slot(slot);
               found = true;
            } else {
                min = min.min(curr_id);
                max = max.max(curr_id);
            }

            bits &= bits - 1;
        }

        if !found {
            return Err(NexoraGraphNodeError::NodeNotFound);
        }


        if self.graph_page_header.max_node_id.get() == node_id {
            self.graph_page_header.max_node_id = U64::new(max);
        }
        if self.graph_page_header.min_node_id.get() == node_id {
            self.graph_page_header.min_node_id = U64::new(min);
        }
        

        return Ok(());
    }
}



use zerocopy::{FromBytes, IntoBytes, Immutable, KnownLayout};
use zerocopy::byteorder::little_endian::{U64, U16};

use crate::graph::string::constants::{STRING_PAGE_HEADER_SIZE, STRING_PAGE_SLOT_SIZE, STRING_PAGE_SLOT_SHIFT};

use crate::graph::record::types::{PackedPtr};

use crate::storage::constants::{PAGE_HEADER_SIZE, PAGE_SIZE};
use crate::storage::models::{NexoraPageHeader};


#[derive(Debug, Copy, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct GraphStringPageHeader {
    pub slot_count: U16,
    pub record_offset: U16,

    _pad: [u8; STRING_PAGE_HEADER_SIZE - 2 - 2],
}

const _: () = assert!(
    std::mem::size_of::<GraphStringPageHeader>() == STRING_PAGE_HEADER_SIZE,
    "GraphStringPageHeader should be the size of STRING_PAGE_HEADER_SIZE",
);

impl GraphStringPageHeader {
    pub fn init() -> Self {
        Self {
            slot_count: U16::new(0),
            record_offset: U16::new(0),

            _pad: [0u8; STRING_PAGE_HEADER_SIZE - 2 - 2],
        }
    }

    pub fn next_slot_offset(&self) -> Option<u64> {
        let slot_offset = self.slot_count.get() << STRING_PAGE_SLOT_SHIFT as u16; 
        if slot_offset + STRING_PAGE_SLOT_SIZE as u16 >= self.record_offset.get() {
            return None;
        }

        return Some((PAGE_HEADER_SIZE + STRING_PAGE_HEADER_SIZE) as u64 + slot_offset as u64);
    }
}

#[derive(Debug, Copy, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct GraphStringSlot {
    pub overflow_slot: PackedPtr,

    pub total_length: U16,
    pub chuck_length: U16,
    pub offset: U16,

    _pad: [u8; STRING_PAGE_SLOT_SIZE - 8 - 2 - 2 - 2],
}


const _: () = assert!(
    std::mem::size_of::<GraphStringSlot>() == STRING_PAGE_SLOT_SIZE,
    "GraphStringSlot should be the size of STRING_PAGE_SLOT_SIZE",
);

const fn get_graph_string_page_buffer_size() -> usize {
    PAGE_SIZE - PAGE_HEADER_SIZE - STRING_PAGE_HEADER_SIZE
}


#[derive(Debug, Copy, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct GraphStringPage {
    pub page_header: NexoraPageHeader,
    pub string_page_header: GraphStringPageHeader,

    pub buf: [u8; get_graph_string_page_buffer_size()],
}

impl GraphStringPage {
    pub fn init(page_id: u64) -> Self {
        Self {
            page_header: NexoraPageHeader::init(page_id),
            string_page_header: GraphStringPageHeader::init(),

            buf: [0u8; get_graph_string_page_buffer_size()],
        }
    } 
}



use zerocopy::{FromBytes, IntoBytes, Immutable, KnownLayout};
use zerocopy::byteorder::little_endian::{U64, U32, U16};

use crate::graph::label::constants::{LABEL_PAGE_HEADER_SIZE, MAX_PAGE_LABEL_COUNT, LABEL_RECORD_SIZE};
use crate::graph::label::error::NexoraGraphLabelError;
use crate::graph::record::types::PackedPtr;

use crate::storage::constants::{PAGE_HEADER_SIZE, PAGE_SIZE};
use crate::storage::models::{NexoraPageHeader, PageType};


const LABEL_PAGE_HEADER_PADDING_SIZE: usize = LABEL_PAGE_HEADER_SIZE - 4 - 2;

#[derive(Debug, Copy, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct LabelPageHeader {
    pub first_label_id: U32,
    pub label_count: U16,
    _pad: [u8; LABEL_PAGE_HEADER_PADDING_SIZE]
}

const _ : () = assert!(
    std::mem::size_of::<LabelPageHeader>() == LABEL_PAGE_HEADER_SIZE,
    "LabelPageHeader must be of size LABEL_PAGE_HEADER_SIZE"
);

impl LabelPageHeader {
    pub fn init(first_label_id: u32) -> Self {
        Self {
            first_label_id: U32::new(first_label_id),
            label_count:    U16::new(0),
            _pad:           [0u8; LABEL_PAGE_HEADER_PADDING_SIZE],
        }
    }

    pub fn is_full(&self) -> bool {
        self.label_count.get() >= MAX_PAGE_LABEL_COUNT
    }

    pub fn exists(&self, label_id: u32) -> bool {
        let first = self.first_label_id.get();
        label_id >= first && label_id < first + self.label_count.get() as u32
    }
}

const LABEL_RECORD_PADDING_SIZE: usize = LABEL_RECORD_SIZE - 8 - 8;

#[derive(Debug, Copy, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct LabelRecord {
    pub label_id: U64,
    pub string_address: PackedPtr,

    _pad: [u8; LABEL_RECORD_PADDING_SIZE],
}

const _ : () = assert!(
    std::mem::size_of::<LabelRecord>() == LABEL_RECORD_SIZE,
    "LabelRecord must be of size LABEL_RECORD_SIZE"
);

const LABEL_GRAPH_PAGE_PADDING: usize = PAGE_SIZE - PAGE_HEADER_SIZE - LABEL_PAGE_HEADER_SIZE - MAX_PAGE_LABEL_COUNT as usize * LABEL_RECORD_SIZE;

#[derive(Debug, Copy, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct GraphLabelPage {
    pub page_header: NexoraPageHeader,
    pub label_page_header: LabelPageHeader,

    pub label_records: [LabelRecord; MAX_PAGE_LABEL_COUNT as usize],

    _pad: [u8; LABEL_GRAPH_PAGE_PADDING],
}

const _ : () = assert!(
    std::mem::size_of::<GraphLabelPage>() == PAGE_SIZE,
    "GraphLabelPage must be of size PAGE_SIZE"
);

impl GraphLabelPage {
    pub fn init(page_id: u64, first_label_id: u32) -> Self {
        Self {
            page_header: NexoraPageHeader {
                page_type: PageType::Label as u8,
                ..NexoraPageHeader::init(page_id)
            },
            label_page_header: LabelPageHeader::init(first_label_id),
            label_records: [LabelRecord {
                label_id:       U64::new(0),
                string_address: PackedPtr::NULL,
                _pad:           [0u8; LABEL_RECORD_PADDING_SIZE],
            }; MAX_PAGE_LABEL_COUNT as usize],
            _pad: [0u8; LABEL_GRAPH_PAGE_PADDING],
        }
    }

    pub fn insert_entry(&mut self, label_id: u32, ptr: PackedPtr) -> Result<(), NexoraGraphLabelError> {
        if self.label_page_header.is_full() {
            return Err(NexoraGraphLabelError::PageFull);
        }
        let slot = self.label_page_header.label_count.get() as usize;
        self.label_records[slot] = LabelRecord {
            label_id:       U64::new(label_id as u64),
            string_address: ptr,
            _pad:           [0u8; LABEL_RECORD_PADDING_SIZE],
        };
        self.label_page_header.label_count = U16::new(slot as u16 + 1);
        Ok(())
    }

    pub fn get_entry(&self, label_id: u32) -> Result<PackedPtr, NexoraGraphLabelError> {
        if !self.label_page_header.exists(label_id) {
            return Err(NexoraGraphLabelError::LabelNotFound);
        }
        let slot = (label_id - self.label_page_header.first_label_id.get()) as usize;
        Ok(self.label_records[slot].string_address)
    }
}





use zerocopy::{FromBytes, IntoBytes, Immutable, KnownLayout};
use zerocopy::byteorder::little_endian::U128;

use crate::graph::property::constants::{
    PROPERTY_HEADER_SIZE, PROPERTY_RECORD_SIZE, PROPERTY_RECORD_SHIFT, MAX_PROPERTY_RECORD_COUNT,
};
use crate::graph::property::error::NexoraGraphPropertyError;
use crate::graph::record::types::{GraphRecordFlag, PackedPtr, RecordHeader};
use crate::storage::constants::{PAGE_HEADER_SIZE, PAGE_SIZE, SENTINEL_PAGE_ID};
use crate::storage::models::{NexoraPageHeader, PageType};


const PROPERTY_HEADER_PADDING: usize = PROPERTY_HEADER_SIZE - 16 - 1;

#[derive(Debug, Copy, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct PropertyPageHeader {
    pub occupied:     U128,
    pub record_count: u8,
    _pad: [u8; PROPERTY_HEADER_PADDING],
}

const _: () = assert!(
    std::mem::size_of::<PropertyPageHeader>() == PROPERTY_HEADER_SIZE,
    "PropertyPageHeader should be equal to PROPERTY_HEADER_SIZE"
);

impl PropertyPageHeader {
    pub fn init() -> Self {
        Self {
            occupied:     U128::new(0),
            record_count: 0,
            _pad:         [0u8; PROPERTY_HEADER_PADDING],
        }
    }
}

impl RecordHeader for PropertyPageHeader {
    fn get_next_record_offset(&self) -> u64 {
        if self.record_count as usize >= MAX_PROPERTY_RECORD_COUNT {
            return SENTINEL_PAGE_ID;
        }
        (PAGE_HEADER_SIZE + PROPERTY_HEADER_SIZE + ((self.record_count as usize) << PROPERTY_RECORD_SHIFT)) as u64
    }

    fn find_free_slot(&mut self) -> Option<usize> {
        let bounds_mask = (1u128 << self.record_count) - 1;
        let deleted = !self.occupied.get() & bounds_mask;
        if deleted != 0 {
            Some(deleted.trailing_zeros() as usize)
        } else if (self.record_count as usize) < MAX_PROPERTY_RECORD_COUNT {
            Some(self.record_count as usize)
        } else {
            None
        }
    }

    fn occupy_slot(&mut self, slot: usize) {
        self.occupied = U128::new(self.occupied.get() | (1u128 << slot));
        if slot == self.record_count as usize {
            self.record_count += 1;
        }
    }

    fn free_slot(&mut self, slot: usize) {
        self.occupied = U128::new(self.occupied.get() & !(1u128 << slot));
    }
}


const PROPERTY_RECORD_PADDING: usize = PROPERTY_RECORD_SIZE - 8 - 8 - 8 - 1;

#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct PropertyPageRecord {
    pub key:   PackedPtr,
    pub value: PackedPtr,
    pub next:  PackedPtr,
    pub flags: u8,
    _pad: [u8; PROPERTY_RECORD_PADDING],
}

const _: () = assert!(
    std::mem::size_of::<PropertyPageRecord>() == PROPERTY_RECORD_SIZE,
    "PropertyPageRecord should be equal to PROPERTY_RECORD_SIZE"
);

impl PropertyPageRecord {
    pub fn empty() -> Self {
        Self {
            key:   PackedPtr::NULL,
            value: PackedPtr::NULL,
            next:  PackedPtr::NULL,
            flags: GraphRecordFlag::ACTIVE as u8,
            _pad:  [0u8; PROPERTY_RECORD_PADDING],
        }
    }
}


#[derive(Debug, Copy, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct GraphPropertyPage {
    pub page_header:          NexoraPageHeader,
    pub property_page_header: PropertyPageHeader,
    pub records:              [PropertyPageRecord; MAX_PROPERTY_RECORD_COUNT],
}

const _: () = assert!(
    std::mem::size_of::<GraphPropertyPage>() == PAGE_SIZE,
    "GraphPropertyPage must be PAGE_SIZE"
);

impl GraphPropertyPage {
    pub fn init(page_id: u64) -> Self {
        Self {
            page_header: NexoraPageHeader {
                page_type: PageType::Property as u8,
                ..NexoraPageHeader::init(page_id)
            },
            property_page_header: PropertyPageHeader::init(),
            records: [PropertyPageRecord::empty(); MAX_PROPERTY_RECORD_COUNT],
        }
    }

    pub fn insert_record(
        &mut self,
        key:   PackedPtr,
        value: PackedPtr,
        next:  PackedPtr,
    ) -> Result<usize, NexoraGraphPropertyError> {
        let slot = self.property_page_header.find_free_slot()
            .ok_or(NexoraGraphPropertyError::PageFull)?;

        self.records[slot] = PropertyPageRecord {
            key,
            value,
            next,
            flags: GraphRecordFlag::ACTIVE as u8,
            _pad:  [0u8; PROPERTY_RECORD_PADDING],
        };
        self.property_page_header.occupy_slot(slot);
        Ok(slot)
    }

    pub fn get_record(&self, index: usize) -> Result<PropertyPageRecord, NexoraGraphPropertyError> {
        let occupied = self.property_page_header.occupied.get();
        if index >= self.property_page_header.record_count as usize
            || occupied & (1u128 << index) == 0
        {
            return Err(NexoraGraphPropertyError::RecordNotFound);
        }
        Ok(self.records[index])
    }

    pub fn delete_record(&mut self, index: usize) {
        self.records[index].flags = GraphRecordFlag::DELETED as u8;
        self.property_page_header.free_slot(index);
    }

    pub fn is_full(&self) -> bool {
        let h = &self.property_page_header;
        let record_count = h.record_count as usize;
        if record_count < MAX_PROPERTY_RECORD_COUNT {
            return false;
        }
        let bounds_mask = (1u128 << record_count) - 1;
        !h.occupied.get() & bounds_mask == 0
    }
}

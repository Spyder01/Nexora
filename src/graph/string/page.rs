use zerocopy::{FromBytes, IntoBytes, Immutable, KnownLayout};
use zerocopy::byteorder::little_endian::U16;

use crate::graph::string::constants::{STRING_PAGE_HEADER_SIZE, STRING_PAGE_SLOT_SIZE, STRING_PAGE_SLOT_SHIFT};
use crate::graph::string::error::NexoraGraphStringError;

use crate::graph::record::types::{Bitset256, PackedPtr};

use crate::storage::constants::{PAGE_HEADER_SIZE, PAGE_SIZE};
use crate::storage::models::NexoraPageHeader;


#[derive(Debug, Copy, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct GraphStringPageHeader {
    pub occupied: Bitset256,
    pub slot_count: U16,
    pub record_offset: U16,

    _pad: [u8; STRING_PAGE_HEADER_SIZE - 32 - 2 - 2],
}

const _: () = assert!(
    std::mem::size_of::<GraphStringPageHeader>() == STRING_PAGE_HEADER_SIZE,
    "GraphStringPageHeader should be the size of STRING_PAGE_HEADER_SIZE",
);

impl GraphStringPageHeader {
    pub fn init() -> Self {
        Self {
            occupied:      Bitset256::ZERO,
            slot_count:    U16::new(0),
            record_offset: U16::new(get_graph_string_page_buffer_size() as u16),
            _pad:          [0u8; STRING_PAGE_HEADER_SIZE - 32 - 2 - 2],
        }
    }
}

#[derive(Debug, Copy, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct GraphStringSlot {
    pub overflow_slot: PackedPtr,

    pub total_length: U16,
    pub chunk_length: U16,
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

    pub fn insert_slot(&mut self, total_length: u16, data: &[u8], overflow: PackedPtr) -> Result<usize, NexoraGraphStringError> {
      let slot_count     = self.string_page_header.slot_count.get() as usize;
      let record_offset  = self.string_page_header.record_offset.get() as usize;

      let next_slot_end     = (slot_count + 1) << STRING_PAGE_SLOT_SHIFT;
      let new_record_offset = record_offset.checked_sub(data.len())
          .ok_or(NexoraGraphStringError::NoFreeSlot)?;

      if next_slot_end > new_record_offset {
          return Err(NexoraGraphStringError::NoFreeSlot);
      }

      self.buf[new_record_offset..new_record_offset + data.len()].copy_from_slice(data);

      let slot = GraphStringSlot {
          overflow_slot: overflow,
          total_length:  U16::new(total_length),
          chunk_length:  U16::new(data.len() as u16),
          offset:        U16::new(new_record_offset as u16),
          _pad:          [0u8; STRING_PAGE_SLOT_SIZE - 8 - 2 - 2 - 2],
      };
      let buf_slot_start = slot_count << STRING_PAGE_SLOT_SHIFT;
      self.buf[buf_slot_start..buf_slot_start + STRING_PAGE_SLOT_SIZE]
          .copy_from_slice(slot.as_bytes());

      self.string_page_header.slot_count    = U16::new((slot_count + 1) as u16);
      self.string_page_header.record_offset = U16::new(new_record_offset as u16);
      self.string_page_header.occupied.set(slot_count);

      Ok(slot_count)
    }

    pub fn get_slot(&self, index: usize) -> Result<GraphStringSlot, NexoraGraphStringError> {
        if index >= self.string_page_header.slot_count.get() as usize {
            return Err(NexoraGraphStringError::SlotNotFound);
        }
        let start = index << STRING_PAGE_SLOT_SHIFT;
        Ok(*GraphStringSlot::ref_from_bytes(&self.buf[start..start + STRING_PAGE_SLOT_SIZE])
            .expect("slice length and alignment guaranteed by compile-time assert"))
    }

    pub fn delete_slot(&mut self, index: usize) {
        self.string_page_header.occupied.clear(index);
    }

    pub fn available_data(&self) -> usize {
        let slot_count    = self.string_page_header.slot_count.get() as usize;
        let record_offset = self.string_page_header.record_offset.get() as usize;
        let next_slot_end = (slot_count + 1) << STRING_PAGE_SLOT_SHIFT;
        record_offset.saturating_sub(next_slot_end)
    }

    pub fn has_space(&self, data_len: usize) -> bool {
        self.available_data() >= data_len
    }

    pub fn read_data(&self, offset: u16, len: u16, out: &mut [u8]) -> Result<(), NexoraGraphStringError> {
        let start = offset as usize;
        let end   = start + len as usize;
        if end > self.buf.len() {
            return Err(NexoraGraphStringError::SlotNotFound);
        }
        out[..len as usize].copy_from_slice(&self.buf[start..end]);
        Ok(())
    }
}



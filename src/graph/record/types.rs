use core::marker::{Copy, Sized};

use zerocopy::{FromBytes, IntoBytes, Immutable, KnownLayout};
use zerocopy::byteorder::little_endian::U64;

pub trait RecordHeader {
   fn get_next_record_offset(&self) -> u64; 
   fn find_free_slot(&mut self) -> Option<usize>;
   fn occupy_slot(&mut self, slot: usize);
   fn free_slot(&mut self, slot: usize);
}


#[derive(Debug, PartialEq)]
#[repr(u8)]
pub enum GraphRecordFlag {
    ACTIVE = 0,
    DELETED = 1,

    UNCLASSIFIED = 2,
}


impl TryFrom<u8> for GraphRecordFlag {
  type Error = u8;

  fn try_from(value: u8) -> Result<Self, Self::Error> {
      match value {
          0 => Ok(GraphRecordFlag::ACTIVE),
          1 => Ok(GraphRecordFlag::DELETED),
          2 => Ok(GraphRecordFlag::UNCLASSIFIED),

          unknown => Err(unknown),
      }
  }
}

pub trait Record: Sized + Copy + FromBytes + IntoBytes {
    fn id(&self) -> u64;
    fn is_active(&self) -> bool;
    fn mark_deleted(&mut self);
}


#[derive(Debug, FromBytes, IntoBytes, Immutable, KnownLayout, Copy, Clone)]
#[repr(transparent)]
pub struct PackedPtr(U64);

impl PackedPtr {
    pub const NULL: PackedPtr = PackedPtr(U64::new(u64::MAX));

    pub fn new(page_id: u64, slot: u8) -> Self {
      PackedPtr(U64::new((page_id << 8) | slot as u64))
    }

    pub fn page_id(&self) -> u64 {
      self.0.get() >> 8
    }

    pub fn slot(&self) -> u8 {
      (self.0.get() & 0xFF) as u8
    }

    pub fn is_null(&self) -> bool {
      self.0.get() == u64::MAX
    }
}

pub struct RecordCursor {
    next_ptr: PackedPtr,
}

impl RecordCursor {
    pub fn new(ptr: PackedPtr) -> Self {
        RecordCursor { next_ptr: ptr }
    }

    pub fn is_done(&self) -> bool {
        self.next_ptr.is_null()
    }

    pub fn ptr(&self) -> PackedPtr {
        self.next_ptr
    }

    pub fn advance_to(&mut self, ptr: PackedPtr) {
        self.next_ptr = ptr;
    }
}


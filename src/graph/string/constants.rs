use crate::storage::constants::{PAGE_HEADER_SIZE, PAGE_SIZE};

pub const STRING_PAGE_HEADER_SIZE: usize = 1 << 6;

pub const STRING_PAGE_SLOT_SIZE: usize = 1 << 4;
pub const STRING_PAGE_SLOT_SHIFT: u32 = STRING_PAGE_SLOT_SIZE.trailing_zeros();

pub const MAX_STRING_LENGTH: usize = u16::MAX as usize;
pub const MAX_STRING_CHUNK_SIZE: usize = PAGE_SIZE - PAGE_HEADER_SIZE - STRING_PAGE_HEADER_SIZE - STRING_PAGE_SLOT_SIZE;

const _: () = assert!(1 << STRING_PAGE_SLOT_SHIFT == STRING_PAGE_SLOT_SIZE);


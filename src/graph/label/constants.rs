use crate::storage::constants::{PAGE_SIZE, PAGE_HEADER_SIZE};

pub const LABEL_PAGE_HEADER_SIZE: usize = 1 << 3;
pub const LABEL_RECORD_SIZE: usize = 1 << 6;

pub const MAX_PAGE_LABEL_COUNT: u16 = ((PAGE_SIZE - PAGE_HEADER_SIZE - LABEL_PAGE_HEADER_SIZE) / LABEL_RECORD_SIZE) as u16;

pub const MAX_LABEL_LENGTH: usize = 256;


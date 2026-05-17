use crate::storage::constants::{PAGE_SIZE, PAGE_HEADER_SIZE};

pub const NODE_PAGE_HEADER_SIZE: usize = 1 << 2;

pub const NODE_RECORD_SIZE: usize = 40;
pub const MAX_RECORD_COUNT: usize = (PAGE_SIZE - PAGE_HEADER_SIZE - NODE_PAGE_HEADER_SIZE) / NODE_RECORD_SIZE;



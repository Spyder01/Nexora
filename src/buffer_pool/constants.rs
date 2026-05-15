use crate::storage::constants::PAGE_SIZE_SHIFT;

pub const BUFFER_POOL_SIZE: usize = 1 << 20;

pub const BUFFER_POOL_MAX_PAGE_COUNT: usize = BUFFER_POOL_SIZE >> PAGE_SIZE_SHIFT;
pub const BUFFER_POOL_WORD_COUNT: usize = BUFFER_POOL_MAX_PAGE_COUNT >> 6;


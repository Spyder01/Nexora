pub const PAGE_SIZE: usize = 1 << 12;
pub const PAGE_SIZE_SHIFT: u32 = PAGE_SIZE.trailing_zeros();

pub const PAGE_HEADER_SIZE: usize = 1 << 5;

pub const SUPPORTED_VERSIONS: [u32; 1] = [1];

pub const SENTINEL_PAGE_ID: u64 = u64::MAX;

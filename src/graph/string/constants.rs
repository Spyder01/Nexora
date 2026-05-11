pub const STRING_PAGE_HEADER_SIZE: usize = 1 << 5;

pub const STRING_PAGE_SLOT_SIZE: usize = 1 << 4;
pub const STRING_PAGE_SLOT_SHIFT: u32 = STRING_PAGE_SLOT_SIZE.trailing_zeros();

const _: () = assert!(1 << STRING_PAGE_SLOT_SHIFT == STRING_PAGE_SLOT_SIZE);


use core::result::Result;

use crate::storage::models::{NexoraPageHeader, PageId};
use crate::storage::constants::PAGE_SIZE;
use crate::storage::error::NexoraStorageError;

const CHECKSUM_OFFSET: usize = 24;
const CHECKSUM_LEN:    usize = 4;

pub trait PageStore {
    fn read_page(&self, page_id: PageId, buf: &mut [u8; PAGE_SIZE], verify_checksum: bool) -> Result<(), NexoraStorageError>;
    fn write_page(&self, page_id: PageId, buf: &[u8; PAGE_SIZE], stamp_checksum: bool) -> Result<(), NexoraStorageError>;

    // Reads only the page header — no checksum verification, no full page I/O.
    // Safe to call only when the caller does not need data integrity guarantees
    // (e.g. traversing the free list to read next_page_id).
    fn read_page_header_unchecked(&self, page_id: PageId) -> Result<NexoraPageHeader, NexoraStorageError>;

    fn get_checksum(buf: &[u8; PAGE_SIZE]) -> u32 {
        u32::from_le_bytes(buf[CHECKSUM_OFFSET..CHECKSUM_OFFSET + CHECKSUM_LEN].try_into().expect("slice is always CHECKSUM_LEN bytes"))
    }

    fn verify_checksum(buf: &[u8; PAGE_SIZE], stored: u32) -> bool {
        let mut h = crc32fast::Hasher::new();
        h.update(&buf[..CHECKSUM_OFFSET]);
        h.update(&[0u8; CHECKSUM_LEN]);
        h.update(&buf[CHECKSUM_OFFSET + CHECKSUM_LEN..]);
        h.finalize() == stored
    }

    fn stamp_checksum(buf: &mut [u8; PAGE_SIZE]) {
        buf[CHECKSUM_OFFSET..CHECKSUM_OFFSET + CHECKSUM_LEN].copy_from_slice(&[0u8; CHECKSUM_LEN]);
        let checksum = crc32fast::hash(buf);
        buf[CHECKSUM_OFFSET..CHECKSUM_OFFSET + CHECKSUM_LEN].copy_from_slice(&checksum.to_le_bytes());
    }
}

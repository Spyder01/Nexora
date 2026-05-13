use zerocopy::byteorder::little_endian::U64;
use zerocopy::{FromBytes, IntoBytes};

use crate::storage::storage_manager::StorageManager;
use crate::storage::page_store::PageStore;
use crate::storage::models::{PageId, PageType};
use crate::storage::constants::{PAGE_SIZE, SENTINEL_PAGE_ID};
use crate::storage::error::NexoraStorageError;

use crate::graph::string::page::GraphStringPage;
use crate::graph::string::error::NexoraGraphStringError;
use crate::graph::string::constants::{MAX_STRING_LENGTH, MAX_STRING_CHUNK_SIZE};
use crate::graph::record::types::PackedPtr;

pub struct StringStore<'a, S: PageStore> {
    storage: &'a mut StorageManager<S>,
}

impl<'a, S: PageStore> StringStore<'a, S> {
    pub fn new(storage: &'a mut StorageManager<S>) -> Self {
        StringStore { storage }
    }

    pub fn insert(&mut self, data: &[u8]) -> Result<PackedPtr, NexoraGraphStringError> {
        if data.len() > MAX_STRING_LENGTH {
            return Err(NexoraGraphStringError::StringTooLong);
        }

        let total_length = data.len() as u16;
        let mut overflow  = PackedPtr::NULL;
        let mut cursor    = data.len();
        let mut first_ptr = PackedPtr::NULL;

        while cursor > 0 {
            let chunk_start = cursor.saturating_sub(MAX_STRING_CHUNK_SIZE);
            let chunk       = &data[chunk_start..cursor];
            let tl          = if chunk_start == 0 { total_length } else { 0 };

            let (page_id, slot) = self.insert_into_page(chunk, tl, overflow)?;
            let ptr = PackedPtr::new(page_id.as_u64(), slot as u8);

            if chunk_start == 0 {
                first_ptr = ptr;
            }

            overflow = ptr;
            cursor   = chunk_start;
        }

        Ok(first_ptr)
    }

    pub fn get(&mut self, ptr: PackedPtr, out: &mut [u8]) -> Result<u16, NexoraGraphStringError> {
        let mut current_ptr  = ptr;
        let mut bytes_written = 0usize;
        let mut total_length  = 0u16;

        while !current_ptr.is_null() {
            let page_id = PageId(current_ptr.page_id());
            let mut buf = [0u8; PAGE_SIZE];
            self.storage.store.read_page(page_id, &mut buf, true)?;
            let page = *GraphStringPage::ref_from_bytes(&buf[..])
                .map_err(|_| NexoraStorageError::CorruptPage(current_ptr.page_id()))?;

            let slot = page.get_slot(current_ptr.slot() as usize)?;

            if bytes_written == 0 {
                total_length = slot.total_length.get();
                if out.len() < total_length as usize {
                    return Err(NexoraGraphStringError::BufferTooSmall);
                }
            }

            let chunk_len = slot.chunk_length.get() as usize;
            page.read_data(
                slot.offset.get(),
                slot.chunk_length.get(),
                &mut out[bytes_written..bytes_written + chunk_len],
            )?;

            bytes_written  += chunk_len;
            current_ptr     = slot.overflow_slot;
        }

        Ok(total_length)
    }

    pub fn delete(&mut self, ptr: PackedPtr) -> Result<(), NexoraGraphStringError> {
        let mut current_ptr = ptr;
        while !current_ptr.is_null() {
            let page_id = PageId(current_ptr.page_id());
            let mut buf = [0u8; PAGE_SIZE];
            self.storage.store.read_page(page_id, &mut buf, true)?;
            let mut page = *GraphStringPage::ref_from_bytes(&buf[..])
                .map_err(|_| NexoraStorageError::CorruptPage(current_ptr.page_id()))?;

            let slot = page.get_slot(current_ptr.slot() as usize)?;
            let next_ptr = slot.overflow_slot;

            page.delete_slot(current_ptr.slot() as usize);
            self.storage.store.write_page(page_id, page.as_bytes().try_into().expect("GraphStringPage is PAGE_SIZE"), true)?;

            current_ptr = next_ptr;
        }
        Ok(())
    }

    fn insert_into_page(
        &mut self,
        data: &[u8],
        total_length: u16,
        overflow: PackedPtr,
    ) -> Result<(PageId, usize), NexoraGraphStringError> {
        let mut page_id_val = self.storage.footer.first_string_page.get();

        while page_id_val != SENTINEL_PAGE_ID {
            let page_id = PageId(page_id_val);
            let mut buf = [0u8; PAGE_SIZE];
            self.storage.store.read_page(page_id, &mut buf, true)?;
            let mut page = *GraphStringPage::ref_from_bytes(&buf[..])
                .map_err(|_| NexoraStorageError::CorruptPage(page_id_val))?;

            if page.has_space(data.len()) {
                let slot = page.insert_slot(total_length, data, overflow)?;
                self.storage.store.write_page(page_id, page.as_bytes().try_into().expect("GraphStringPage is PAGE_SIZE"), true)?;
                return Ok((page_id, slot));
            }

            page_id_val = page.page_header.next_page_id.get();
        }

        self.insert_into_new_page(data, total_length, overflow)
    }

    fn insert_into_new_page(
        &mut self,
        data: &[u8],
        total_length: u16,
        overflow: PackedPtr,
    ) -> Result<(PageId, usize), NexoraGraphStringError> {
        let new_page_id = self.storage.allocate_page()?;
        let old_first   = self.storage.footer.first_string_page.get();

        let mut page = GraphStringPage::init(new_page_id.as_u64());
        page.page_header.next_page_id = U64::new(old_first);
        page.page_header.page_type    = PageType::String as u8;

        let slot = page.insert_slot(total_length, data, overflow)?;
        self.storage.store.write_page(new_page_id, page.as_bytes().try_into().expect("GraphStringPage is PAGE_SIZE"), true)?;

        self.storage.footer.first_string_page = U64::new(new_page_id.as_u64());
        self.storage.mark_footer_dirty();

        Ok((new_page_id, slot))
    }
}

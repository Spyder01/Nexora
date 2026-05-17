use zerocopy::byteorder::little_endian::U64;
use zerocopy::{FromBytes, IntoBytes};

use crate::storage::storage_manager::StorageManager;
use crate::storage::page_store::PageStore;
use crate::storage::models::{PageId, PageType};
use crate::storage::constants::{PAGE_SIZE, SENTINEL_PAGE_ID};
use crate::storage::error::NexoraStorageError;

use crate::index::page_index::page::NodePageIndexPage;
use crate::index::page_index::constants::MAX_NODE_PAGE_INDEX_RECORD_COUNT;

pub struct NodePageIndexStore<'a, S: PageStore> {
    storage: &'a mut StorageManager<S>,
}

impl<'a, S: PageStore> NodePageIndexStore<'a, S> {
    pub fn new(storage: &'a mut StorageManager<S>) -> Self {
        NodePageIndexStore { storage }
    }

    fn dir_page_id(&mut self, dir_page_index: usize) -> Result<PageId, NexoraStorageError> {
        let mut page_id_val = self.storage.footer.first_page_index_page.get();
        for _ in 0..dir_page_index {
            if page_id_val == SENTINEL_PAGE_ID {
                return Err(NexoraStorageError::CorruptPage(SENTINEL_PAGE_ID));
            }
            let header = self.storage.store.read_page_header_unchecked(PageId(page_id_val))?;
            page_id_val = header.next_page_id.get();
        }
        if page_id_val == SENTINEL_PAGE_ID {
            return Err(NexoraStorageError::CorruptPage(SENTINEL_PAGE_ID));
        }
        Ok(PageId(page_id_val))
    }

    pub fn lookup(&mut self, page_index: usize) -> Result<PageId, NexoraStorageError> {
        let dir_page_index = page_index / MAX_NODE_PAGE_INDEX_RECORD_COUNT;
        let slot          = page_index % MAX_NODE_PAGE_INDEX_RECORD_COUNT;

        let dir_page_id = self.dir_page_id(dir_page_index)?;
        let mut buf = [0u8; PAGE_SIZE];
        self.storage.store.read_page(dir_page_id, &mut buf, true)?;
        let page = NodePageIndexPage::ref_from_bytes(&buf[..])
            .map_err(|_| NexoraStorageError::CorruptPage(dir_page_id.as_u64()))?;

        Ok(PageId(page.records[slot].page_id.get()))
    }

    pub fn insert(&mut self, page_index: usize, page_id: PageId) -> Result<(), NexoraStorageError> {
        let dir_page_index = page_index / MAX_NODE_PAGE_INDEX_RECORD_COUNT;
        let slot          = page_index % MAX_NODE_PAGE_INDEX_RECORD_COUNT;

        if slot == 0 {
            self.append_dir_page(dir_page_index, page_id)?;
        } else {
            let dir_page_id = self.dir_page_id(dir_page_index)?;
            let mut buf = [0u8; PAGE_SIZE];
            self.storage.store.read_page(dir_page_id, &mut buf, true)?;
            let mut page = *NodePageIndexPage::ref_from_bytes(&buf[..])
                .map_err(|_| NexoraStorageError::CorruptPage(dir_page_id.as_u64()))?;
            page.records[slot].page_id = U64::new(page_id.as_u64());
            self.storage.store.write_page(dir_page_id, page.as_bytes().try_into().expect("NodePageIndexPage is PAGE_SIZE"), true)?;
        }

        Ok(())
    }

    fn append_dir_page(&mut self, dir_page_index: usize, page_id: PageId) -> Result<(), NexoraStorageError> {
        let new_dir_page_id = self.storage.allocate_page()?;

        let buf = [0u8; PAGE_SIZE];
        let mut page = *NodePageIndexPage::ref_from_bytes(&buf[..]).expect("zero-initialized PAGE_SIZE buf");
        page.page_header.page_id      = U64::new(new_dir_page_id.as_u64());
        page.page_header.page_type    = PageType::PageIndex as u8;
        page.page_header.next_page_id = U64::new(SENTINEL_PAGE_ID);
        page.page_header.prev_page_id = U64::new(SENTINEL_PAGE_ID);
        page.records[0].page_id       = U64::new(page_id.as_u64());
        self.storage.store.write_page(new_dir_page_id, page.as_bytes().try_into().expect("NodePageIndexPage is PAGE_SIZE"), true)?;

        if dir_page_index == 0 {
            self.storage.footer.first_page_index_page = U64::new(new_dir_page_id.as_u64());
            self.storage.mark_footer_dirty();
        } else {
            // Link previous last directory page → new directory page.
            let prev_dir_page_id = self.dir_page_id(dir_page_index - 1)?;
            let mut prev_buf = [0u8; PAGE_SIZE];
            self.storage.store.read_page(prev_dir_page_id, &mut prev_buf, false)?;
            let mut prev_page = *NodePageIndexPage::ref_from_bytes(&prev_buf[..])
                .map_err(|_| NexoraStorageError::CorruptPage(prev_dir_page_id.as_u64()))?;
            prev_page.page_header.next_page_id = U64::new(new_dir_page_id.as_u64());
            self.storage.store.write_page(prev_dir_page_id, prev_page.as_bytes().try_into().expect("NodePageIndexPage is PAGE_SIZE"), true)?;
        }

        Ok(())
    }
}

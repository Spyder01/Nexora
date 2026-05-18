use zerocopy::byteorder::little_endian::U64;
use zerocopy::{FromBytes, IntoBytes};

use crate::storage::storage_manager::StorageManager;
use crate::storage::page_store::PageStore;
use crate::storage::models::PageId;
use crate::storage::constants::{PAGE_SIZE, SENTINEL_PAGE_ID};
use crate::storage::error::NexoraStorageError;

use crate::graph::label::page::GraphLabelPage;
use crate::graph::label::constants::MAX_LABEL_LENGTH;
use crate::graph::label::error::NexoraGraphLabelError;
use crate::graph::string::label_string_store::LabelStringStore;

pub struct LabelStore<'a, S: PageStore> {
    storage: &'a mut StorageManager<S>,
}

impl<'a, S: PageStore> LabelStore<'a, S> {
    pub fn new(storage: &'a mut StorageManager<S>) -> Self {
        LabelStore { storage }
    }

    pub fn insert_label(&mut self, data: &[u8]) -> Result<u32, NexoraGraphLabelError> {
        if let Some(existing_id) = self.find_duplicate(data)? {
            return Ok(existing_id);
        }

        let ptr = {
            let mut string_store = LabelStringStore::new(self.storage);
            string_store.insert(data)?
        };

        let label_id = self.storage.footer.label_pages_count.get() as u32;
        self.insert_into_page(label_id, ptr)?;

        self.storage.footer.label_pages_count = U64::new(label_id as u64 + 1);
        self.storage.mark_footer_dirty();

        Ok(label_id)
    }

    fn find_duplicate(&mut self, data: &[u8]) -> Result<Option<u32>, NexoraGraphLabelError> {
        let mut page_id_val = self.storage.footer.first_label_page.get();
        let mut buf = [0u8; MAX_LABEL_LENGTH];

        while page_id_val != SENTINEL_PAGE_ID {
            let page_id = PageId(page_id_val);
            let mut page_buf = [0u8; PAGE_SIZE];
            self.storage.store.read_page(page_id, &mut page_buf, true)?;
            let page = *GraphLabelPage::ref_from_bytes(&page_buf[..])
                .map_err(|_| NexoraStorageError::CorruptPage(page_id_val))?;

            let count = page.label_page_header.label_count.get() as usize;
            for i in 0..count {
                let record = page.label_records[i];
                let len = match LabelStringStore::new(self.storage).get(record.string_address, &mut buf) {
                    Ok(n)                                                => n,
                    Err(crate::graph::string::error::NexoraGraphStringError::BufferTooSmall) => continue,
                    Err(e) => return Err(NexoraGraphLabelError::StringError(e)),
                };
                if len as usize == data.len() && &buf[..len as usize] == data {
                    return Ok(Some(record.label_id.get() as u32));
                }
            }

            page_id_val = page.page_header.next_page_id.get();
        }

        Ok(None)
    }

    pub fn get_label(&mut self, label_id: u32, out: &mut [u8]) -> Result<u16, NexoraGraphLabelError> {
        let ptr = self.find_label_ptr(label_id)?;

        let len = {
            let mut string_store = LabelStringStore::new(self.storage);
            string_store.get(ptr, out)?
        };

        Ok(len)
    }

    fn find_label_ptr(&mut self, label_id: u32) -> Result<crate::graph::record::types::PackedPtr, NexoraGraphLabelError> {
        let mut page_id_val = self.storage.footer.first_label_page.get();

        while page_id_val != SENTINEL_PAGE_ID {
            let page_id = PageId(page_id_val);
            let mut buf = [0u8; PAGE_SIZE];
            self.storage.store.read_page(page_id, &mut buf, true)?;
            let page = *GraphLabelPage::ref_from_bytes(&buf[..])
                .map_err(|_| NexoraStorageError::CorruptPage(page_id_val))?;

            if page.label_page_header.exists(label_id) {
                return Ok(page.get_entry(label_id)?);
            }

            page_id_val = page.page_header.next_page_id.get();
        }

        Err(NexoraGraphLabelError::LabelNotFound)
    }

    fn insert_into_page(
        &mut self,
        label_id: u32,
        ptr: crate::graph::record::types::PackedPtr,
    ) -> Result<(), NexoraGraphLabelError> {
        let mut page_id_val = self.storage.footer.first_label_page.get();

        while page_id_val != SENTINEL_PAGE_ID {
            let page_id = PageId(page_id_val);
            let mut buf = [0u8; PAGE_SIZE];
            self.storage.store.read_page(page_id, &mut buf, true)?;
            let mut page = *GraphLabelPage::ref_from_bytes(&buf[..])
                .map_err(|_| NexoraStorageError::CorruptPage(page_id_val))?;

            if !page.label_page_header.is_full() {
                page.insert_entry(label_id, ptr)?;
                self.storage.store.write_page(page_id, page.as_bytes().try_into().expect("GraphLabelPage is PAGE_SIZE"), true)?;
                return Ok(());
            }

            page_id_val = page.page_header.next_page_id.get();
        }

        self.insert_into_new_page(label_id, ptr)
    }

    fn insert_into_new_page(
        &mut self,
        label_id: u32,
        ptr: crate::graph::record::types::PackedPtr,
    ) -> Result<(), NexoraGraphLabelError> {
        let new_page_id = self.storage.allocate_page()?;
        let old_first   = self.storage.footer.first_label_page.get();

        let mut page = GraphLabelPage::init(new_page_id.as_u64(), label_id);
        page.page_header.next_page_id = U64::new(old_first);

        page.insert_entry(label_id, ptr)?;
        self.storage.store.write_page(new_page_id, page.as_bytes().try_into().expect("GraphLabelPage is PAGE_SIZE"), true)?;

        self.storage.footer.first_label_page = U64::new(new_page_id.as_u64());
        self.storage.mark_footer_dirty();

        Ok(())
    }
}

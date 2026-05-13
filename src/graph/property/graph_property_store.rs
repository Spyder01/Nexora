use zerocopy::byteorder::little_endian::U64;
use zerocopy::{FromBytes, IntoBytes};

use crate::storage::storage_manager::StorageManager;
use crate::storage::page_store::PageStore;
use crate::storage::models::PageId;
use crate::storage::constants::{PAGE_SIZE, SENTINEL_PAGE_ID};
use crate::storage::error::NexoraStorageError;

use crate::graph::property::page::{GraphPropertyPage, PropertyPageRecord};
use crate::graph::property::error::NexoraGraphPropertyError;
use crate::graph::record::types::PackedPtr;

pub struct PropertyStore<'a, S: PageStore> {
    storage: &'a mut StorageManager<S>,
}

impl<'a, S: PageStore> PropertyStore<'a, S> {
    pub fn new(storage: &'a mut StorageManager<S>) -> Self {
        PropertyStore { storage }
    }

    pub fn insert(
        &mut self,
        key:   PackedPtr,
        value: PackedPtr,
    ) -> Result<PackedPtr, NexoraGraphPropertyError> {
        let (page_id, slot) = self.insert_into_page(key, value, PackedPtr::NULL)?;
        Ok(PackedPtr::new(page_id.as_u64(), slot as u8))
    }

    pub fn insert_chained(
        &mut self,
        key:   PackedPtr,
        value: PackedPtr,
        next:  PackedPtr,
    ) -> Result<PackedPtr, NexoraGraphPropertyError> {
        let (page_id, slot) = self.insert_into_page(key, value, next)?;
        Ok(PackedPtr::new(page_id.as_u64(), slot as u8))
    }

    pub fn update_value(
        &mut self,
        ptr:       PackedPtr,
        new_value: PackedPtr,
    ) -> Result<(), NexoraGraphPropertyError> {
        let page_id = PageId(ptr.page_id());
        let mut buf = [0u8; PAGE_SIZE];
        self.storage.store.read_page(page_id, &mut buf, true)?;
        let mut page = *GraphPropertyPage::ref_from_bytes(&buf[..])
            .map_err(|_| NexoraStorageError::CorruptPage(ptr.page_id()))?;
        page.records[ptr.slot() as usize].value = new_value;
        self.storage.store.write_page(page_id, page.as_bytes().try_into().expect("GraphPropertyPage is PAGE_SIZE"), true)?;
        Ok(())
    }

    pub fn get(&mut self, ptr: PackedPtr) -> Result<PropertyPageRecord, NexoraGraphPropertyError> {
        let page_id = PageId(ptr.page_id());
        let mut buf = [0u8; PAGE_SIZE];
        self.storage.store.read_page(page_id, &mut buf, true)?;
        let page = *GraphPropertyPage::ref_from_bytes(&buf[..])
            .map_err(|_| NexoraStorageError::CorruptPage(ptr.page_id()))?;
        page.get_record(ptr.slot() as usize)
    }

    pub fn update_next(
        &mut self,
        ptr:      PackedPtr,
        new_next: PackedPtr,
    ) -> Result<(), NexoraGraphPropertyError> {
        let page_id = PageId(ptr.page_id());
        let mut buf = [0u8; PAGE_SIZE];
        self.storage.store.read_page(page_id, &mut buf, true)?;
        let mut page = *GraphPropertyPage::ref_from_bytes(&buf[..])
            .map_err(|_| NexoraStorageError::CorruptPage(ptr.page_id()))?;
        page.records[ptr.slot() as usize].next = new_next;
        self.storage.store.write_page(page_id, page.as_bytes().try_into().expect("GraphPropertyPage is PAGE_SIZE"), true)?;
        Ok(())
    }

    pub fn delete(&mut self, ptr: PackedPtr) -> Result<(), NexoraGraphPropertyError> {
        let page_id = PageId(ptr.page_id());
        let mut buf = [0u8; PAGE_SIZE];
        self.storage.store.read_page(page_id, &mut buf, true)?;
        let mut page = *GraphPropertyPage::ref_from_bytes(&buf[..])
            .map_err(|_| NexoraStorageError::CorruptPage(ptr.page_id()))?;
        page.delete_record(ptr.slot() as usize);
        self.storage.store.write_page(page_id, page.as_bytes().try_into().expect("GraphPropertyPage is PAGE_SIZE"), true)?;
        Ok(())
    }

    fn insert_into_page(
        &mut self,
        key:   PackedPtr,
        value: PackedPtr,
        next:  PackedPtr,
    ) -> Result<(PageId, usize), NexoraGraphPropertyError> {
        let mut page_id_val = self.storage.footer.first_property_page.get();

        while page_id_val != SENTINEL_PAGE_ID {
            let page_id = PageId(page_id_val);
            let mut buf = [0u8; PAGE_SIZE];
            self.storage.store.read_page(page_id, &mut buf, true)?;
            let mut page = *GraphPropertyPage::ref_from_bytes(&buf[..])
                .map_err(|_| NexoraStorageError::CorruptPage(page_id_val))?;

            if !page.is_full() {
                let slot = page.insert_record(key, value, next)?;
                self.storage.store.write_page(page_id, page.as_bytes().try_into().expect("GraphPropertyPage is PAGE_SIZE"), true)?;
                return Ok((page_id, slot));
            }

            page_id_val = page.page_header.next_page_id.get();
        }

        self.insert_into_new_page(key, value, next)
    }

    fn insert_into_new_page(
        &mut self,
        key:   PackedPtr,
        value: PackedPtr,
        next:  PackedPtr,
    ) -> Result<(PageId, usize), NexoraGraphPropertyError> {
        let new_page_id = self.storage.allocate_page()?;
        let old_first   = self.storage.footer.first_property_page.get();

        let mut page = GraphPropertyPage::init(new_page_id.as_u64());
        page.page_header.next_page_id = U64::new(old_first);

        let slot = page.insert_record(key, value, next)?;
        self.storage.store.write_page(new_page_id, page.as_bytes().try_into().expect("GraphPropertyPage is PAGE_SIZE"), true)?;

        self.storage.footer.first_property_page = U64::new(new_page_id.as_u64());
        self.storage.mark_footer_dirty();

        Ok((new_page_id, slot))
    }
}

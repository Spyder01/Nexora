use core::result::Result::{self, Ok};
use zerocopy::{FromBytes, IntoBytes};
use zerocopy::byteorder::little_endian::{U32, U64};

use crate::storage::models::{NexoraFooter, NexoraHeader, NexoraPageHeader, PageId, PageType};
use crate::storage::constants::{PAGE_HEADER_SIZE, PAGE_SIZE, SENTINEL_PAGE_ID, SUPPORTED_VERSIONS};
use crate::storage::page_store::PageStore;
use crate::storage::error::NexoraStorageError;

pub struct StorageManager<S: PageStore> {
    pub store: S,
    pub header: NexoraHeader,
    pub footer: NexoraFooter,
    footer_dirty: bool,
}

impl<S: PageStore> StorageManager<S> {
    fn read_header(store: &S) -> Result<NexoraHeader, NexoraStorageError> {

      let mut buf = [0u8; PAGE_SIZE];
      store.read_page(PageId(0), &mut buf, true)?;

      let header = NexoraHeader::ref_from_bytes(&buf)
          .map_err(|_| NexoraStorageError::CorruptPage(0))?;

      if &header.magic != b"NXRA" {
          return Err(NexoraStorageError::InvalidMagicBytes);
      }

      if !SUPPORTED_VERSIONS.contains(&header.version.get()) {
          return Err(NexoraStorageError::UnsupportedVersion(header.version.get()));
      }

      Ok(*header)
    }

    fn read_footer(store: &S, header: &NexoraHeader) -> Result<NexoraFooter, NexoraStorageError> {
     let footer_page_id = PageId(header.footer_page_id.get());

      let mut buf = [0u8; PAGE_SIZE];
      store.read_page(footer_page_id, &mut buf, true)?;

      let footer = NexoraFooter::ref_from_bytes(&buf)
          .map_err(|_| NexoraStorageError::CorruptPage(footer_page_id.as_u64()))?;

      Ok(*footer)
    }

    pub fn new(store: S, header: NexoraHeader, footer: NexoraFooter) -> Self {
        StorageManager { store, header, footer, footer_dirty: false }
    }

    pub fn from_page_store(store: S) -> Result<Self, NexoraStorageError> {
        let header = StorageManager::read_header(&store)?;
        let footer = StorageManager::read_footer(&store, &header)?;

        return Ok(StorageManager { store, header, footer, footer_dirty: false });
    }

    pub fn mark_footer_dirty(&mut self) {
        self.footer_dirty = true;
    }

    pub fn close(&mut self) -> Result<(), NexoraStorageError> {
        if !self.footer_dirty {
            return Ok(());
        }

        let footer_bytes: &[u8; PAGE_SIZE] = self.footer.as_bytes().try_into().unwrap();
        self.store.write_page(PageId(self.header.footer_page_id.get()), footer_bytes, true)?;
        self.footer_dirty = false;

        Ok(())
    }

    pub fn allocate_page(&mut self) -> Result<PageId, NexoraStorageError> {
        if self.footer.first_free_page.get() != SENTINEL_PAGE_ID {
            let page_id = PageId(self.footer.first_free_page.get());

            let page_header = self.store.read_page_header_unchecked(page_id)?;
            let next_free = page_header.next_page_id.get();

            self.footer.first_free_page  = U64::new(next_free);
            self.footer.free_pages_count = U64::new(self.footer.free_pages_count.get() - 1);
            self.footer_dirty = true;

            Ok(page_id)
        } else {
            let page_id = PageId(self.footer.page_count.get());
            self.footer.page_count = U64::new(self.footer.page_count.get() + 1);
            self.footer_dirty = true;

            Ok(page_id)
        }
    }

    pub fn free_page(&mut self, page_id: PageId) -> Result<(), NexoraStorageError> {
        let mut buf = [0u8; PAGE_SIZE];

        let free_header = NexoraPageHeader {
            page_id:      U64::new(page_id.as_u64()),
            next_page_id: U64::new(self.footer.first_free_page.get()),
            prev_page_id: U64::new(SENTINEL_PAGE_ID),
            checksum:     U32::new(0),
            page_type:    PageType::Free as u8,
            _pad:         [0u8; PAGE_HEADER_SIZE - 8 - 8 - 8 - 4 - 1],
        };

        buf[..PAGE_HEADER_SIZE].copy_from_slice(free_header.as_bytes());

        self.store.write_page(page_id, &buf, false)?;

        self.footer.first_free_page  = U64::new(page_id.as_u64());
        self.footer.free_pages_count = U64::new(self.footer.free_pages_count.get() + 1);
        self.footer_dirty = true;

        Ok(())
    }
}

impl<S: PageStore> Drop for StorageManager<S> {
    fn drop(&mut self) {
        if self.footer_dirty {
            if let Ok(footer_bytes) = self.footer.as_bytes().try_into() {
                let _ = self.store.write_page(
                    PageId(self.header.footer_page_id.get()),
                    footer_bytes,
                    true
                );
            }
        }
    }
}




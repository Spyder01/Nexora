use core::result::Result;
use std::os::unix::fs::FileExt;
use zerocopy::{FromBytes, IntoBytes};

use crate::storage::page_store::PageStore;
use crate::storage::models::{INITIAL_FOOTER, INITIAL_HEADER, NexoraPageHeader, PageId};
use crate::storage::constants::{PAGE_HEADER_SIZE, PAGE_SIZE};
use crate::storage::error::NexoraStorageError;

pub struct RegularPageStore {
    file: std::fs::File,
}

impl RegularPageStore {
  pub fn create(path: &std::path::Path) -> Result<Self, NexoraStorageError> {
      let file = std::fs::OpenOptions::new()
          .read(true)
          .write(true)
          .create_new(true)
          .open(path)?;

      let mut store = RegularPageStore { file };
      store.write_default_header()?;
      store.write_default_footer()?;
      Ok(store)
  }

  fn write_default_header(&mut self) -> Result<(), NexoraStorageError> {
      self.write_page(PageId(0), INITIAL_HEADER.as_bytes().try_into().expect("INITIAL_HEADER is PAGE_SIZE"), true)
  }

  fn write_default_footer(&mut self) -> Result<(), NexoraStorageError> {
      self.write_page(PageId(1), INITIAL_FOOTER.as_bytes().try_into().expect("INITIAL_FOOTER is PAGE_SIZE"), true)
  }

  pub fn open(path: &std::path::Path) -> Result<Self, NexoraStorageError> {
      let file = std::fs::OpenOptions::new()
          .read(true)
          .write(true)
          .open(path)?;
      Ok(RegularPageStore { file })
  }
}


impl PageStore for RegularPageStore {
    fn read_page(&mut self, page_id: PageId, buf: &mut [u8; PAGE_SIZE], verify_checksum: bool) -> Result<(), NexoraStorageError> {
        self.file.read_exact_at(buf, page_id.byte_offset())?;
        let stored = Self::get_checksum(buf);
        if verify_checksum && !Self::verify_checksum(buf, stored) {
            return Err(NexoraStorageError::ChecksumMismatch(page_id.as_u64()));
        }
        Ok(())
    }

    fn write_page(&mut self, page_id: PageId, buf: &[u8; PAGE_SIZE], stamp_checksum: bool) -> Result<(), NexoraStorageError> {
        let mut page = *buf;

        if stamp_checksum {
            Self::stamp_checksum(&mut page);
        }

        self.file.write_all_at(&page, page_id.byte_offset())?;
        Ok(())
    }

    fn read_page_header_unchecked(&mut self, page_id: PageId) -> Result<NexoraPageHeader, NexoraStorageError> {
        let mut buf = [0u8; PAGE_HEADER_SIZE];
        self.file.read_exact_at(&mut buf, page_id.byte_offset())?;
        let header = NexoraPageHeader::ref_from_bytes(&buf)
            .map_err(|_| NexoraStorageError::CorruptPage(page_id.as_u64()))?;
        Ok(*header)
    }
}


use core::convert::TryFrom;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use zerocopy::{FromBytes, IntoBytes, Immutable, KnownLayout};
use zerocopy::byteorder::little_endian::{U32, U64};

use crate::storage::constants::{PAGE_HEADER_SIZE, PAGE_SIZE};
use crate::storage::error::NexoraStorageError;
use crate::storage::models::{NexoraPageHeader, PageId};
use crate::storage::page_store::PageStore;

const WAL_MAGIC: [u8; 4] = *b"NXWL";
const WAL_VERSION: u32 = 1;
const WAL_HEADER_SIZE: usize = std::mem::size_of::<WALHeader>();
const WAL_INDEX_CAPACITY: usize = 4096;

#[derive(Debug, FromBytes, IntoBytes, Immutable, KnownLayout, Clone, Copy)]
#[repr(C)]
pub struct WALHeader {
    magic:     [u8; 4],
    page_size: U32,
    version:   U32,
    checksum:  U32,
}

impl WALHeader {
    fn new(page_size: u32) -> Self {
        let bytes_before_checksum = 12;
        let mut h = WALHeader {
            magic:     WAL_MAGIC,
            page_size: U32::new(page_size),
            version:   U32::new(WAL_VERSION),
            checksum:  U32::new(0),
        };
        let checksum = crc32fast::hash(&h.as_bytes()[..bytes_before_checksum]);
        h.checksum = U32::new(checksum);

        h
    }

    fn validate(&self) -> Result<(), NexoraStorageError> {
        if self.magic != WAL_MAGIC {
            return Err(NexoraStorageError::InvalidMagicBytes);
        }
        
        if self.version.get() != WAL_VERSION {
            return Err(NexoraStorageError::UnsupportedVersion(self.version.get()));
        }
        
        if self.page_size.get() != PAGE_SIZE as u32 {
            return Err(NexoraStorageError::CorruptWal);
        }

        let stored = self.checksum.get();
        let computed = crc32fast::hash(&self.as_bytes()[..12]);
        if stored != computed {
            return Err(NexoraStorageError::CorruptWal);
        }

        Ok(())
    }
}

#[derive(Debug, Copy, Clone)]
#[repr(u8)]
pub enum WALFrameFlag {
    Data   = 0,
    Commit = 1,
}

impl TryFrom<u8> for WALFrameFlag {
    type Error = u8;

    fn try_from(value: u8) -> core::result::Result<Self, Self::Error> {
        match value {
            0 => Ok(WALFrameFlag::Data),
            1 => Ok(WALFrameFlag::Commit),
            v => Err(v),
        }
    }
}

pub const WAL_FRAME_PADDING: usize = 8 - (8 + 1) % 8;
pub const WAL_FRAME_SIZE: usize = 8 + 1 + WAL_FRAME_PADDING + PAGE_SIZE;

const _: () = assert!(
    std::mem::size_of::<WALFrame>() == WAL_FRAME_SIZE,
    "WALFrame size must match WAL_FRAME_SIZE"
);

#[derive(Debug, FromBytes, IntoBytes, Immutable, KnownLayout, Clone, Copy)]
#[repr(C)]
pub struct WALFrame {
    page_id: U64,
    flags:   u8,
    _pad:    [u8; WAL_FRAME_PADDING],
    page:    [u8; PAGE_SIZE],
}

impl WALFrame {
    fn data(page_id: u64, page: &[u8; PAGE_SIZE]) -> Self {
        WALFrame {
            page_id: U64::new(page_id),
            flags:   WALFrameFlag::Data as u8,
            _pad:    [0u8; WAL_FRAME_PADDING],
            page:    *page,
        }
    }

    fn commit() -> Self {
        WALFrame {
            page_id: U64::new(u64::MAX),
            flags:   WALFrameFlag::Commit as u8,
            _pad:    [0u8; WAL_FRAME_PADDING],
            page:    [0u8; PAGE_SIZE],
        }
    }
}

pub struct WALPageStore<S: PageStore> {
    inner:     S,
    wal_file:  File,
    index:     Box<[(u64, u64)]>,
    index_len: usize,
    wal_end:   u64,
}

impl<S: PageStore> WALPageStore<S> {
    pub fn create(inner: S, wal_path: &Path) -> Result<Self, NexoraStorageError> {
        let mut file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(wal_path)?;

        let header = WALHeader::new(PAGE_SIZE as u32);
        file.write_all(header.as_bytes())?;

        Ok(WALPageStore {
            inner,
            wal_file:  file,
            index:     vec![(0u64, 0u64); WAL_INDEX_CAPACITY].into_boxed_slice(),
            index_len: 0,
            wal_end:   WAL_HEADER_SIZE as u64,
        })
    }

    pub fn open(inner: S, wal_path: &Path) -> Result<Self, NexoraStorageError> {
        let (file, index_pairs, wal_end) = if wal_path.exists() {
            let mut f = File::options().read(true).write(true).open(wal_path)?;
            let file_len = f.seek(SeekFrom::End(0))?;

            if file_len < WAL_HEADER_SIZE as u64 {
                f.seek(SeekFrom::Start(0))?;
                f.write_all(WALHeader::new(PAGE_SIZE as u32).as_bytes())?;
                f.set_len(WAL_HEADER_SIZE as u64)?;
                (f, vec![], WAL_HEADER_SIZE as u64)
            } else {
                f.seek(SeekFrom::Start(0))?;
                let mut buf = [0u8; WAL_HEADER_SIZE];
                f.read_exact(&mut buf)?;
                let header = WALHeader::ref_from_bytes(&buf)
                    .map_err(|_| NexoraStorageError::CorruptWal)?;
                header.validate()?;

                let pairs = Self::recover(&mut f, file_len)?;
                let end = file_len;
                (f, pairs, end)
            }
        } else {
            let mut f = File::options()
                .read(true).write(true).create(true)
                .open(wal_path)?;
            f.write_all(WALHeader::new(PAGE_SIZE as u32).as_bytes())?;
            (f, vec![], WAL_HEADER_SIZE as u64)
        };

        if index_pairs.len() > WAL_INDEX_CAPACITY {
            return Err(NexoraStorageError::WalIndexFull);
        }

        let mut index = vec![(0u64, 0u64); WAL_INDEX_CAPACITY].into_boxed_slice();
        let index_len = index_pairs.len();
        for (i, pair) in index_pairs.into_iter().enumerate() {
            index[i] = pair;
        }

        Ok(WALPageStore { inner, wal_file: file, index, index_len, wal_end })
    }

    fn recover(file: &mut File, file_len: u64) -> Result<Vec<(u64, u64)>, NexoraStorageError> {
        let mut committed: Vec<(u64, u64)> = Vec::new();
        let mut pending:   Vec<(u64, u64)> = Vec::new();

        file.seek(SeekFrom::Start(WAL_HEADER_SIZE as u64))?;
        let mut offset = WAL_HEADER_SIZE as u64;

        while offset + WAL_FRAME_SIZE as u64 <= file_len {
            let mut buf = [0u8; WAL_FRAME_SIZE];
            file.read_exact(&mut buf)?;

            let frame = WALFrame::ref_from_bytes(&buf[..])
                .map_err(|_| NexoraStorageError::CorruptWal)?;

            let flag = WALFrameFlag::try_from(frame.flags)
                .map_err(|_| NexoraStorageError::CorruptWal)?;

            match flag {
                WALFrameFlag::Data => {
                    let page_id = frame.page_id.get();
                    if let Some(e) = pending.iter_mut().find(|(pid, _)| *pid == page_id) {
                        e.1 = offset;
                    } else {
                        pending.push((page_id, offset));
                    }
                }
                WALFrameFlag::Commit => {
                    for (page_id, frame_off) in pending.drain(..) {
                        if let Some(e) = committed.iter_mut().find(|(pid, _)| *pid == page_id) {
                            e.1 = frame_off;
                        } else {
                            committed.push((page_id, frame_off));
                        }
                    }
                }
            }

            offset += WAL_FRAME_SIZE as u64;
        }

        Ok(committed)
    }

    fn index_lookup(&self, page_id: u64) -> Option<u64> {
        self.index[..self.index_len]
            .iter()
            .find(|(pid, _)| *pid == page_id)
            .map(|(_, offset)| *offset)
    }

    fn index_upsert(&mut self, page_id: u64, offset: u64) -> Result<(), NexoraStorageError> {
        for entry in &mut self.index[..self.index_len] {
            if entry.0 == page_id {
                entry.1 = offset;
                return Ok(());
            }
        }
        if self.index_len >= WAL_INDEX_CAPACITY {
            return Err(NexoraStorageError::WalIndexFull);
        }
        self.index[self.index_len] = (page_id, offset);
        self.index_len += 1;
        Ok(())
    }

    pub fn commit(&mut self) -> Result<(), NexoraStorageError> {
        self.wal_file.seek(SeekFrom::Start(self.wal_end))?;
        self.wal_file.write_all(WALFrame::commit().as_bytes())?;
        self.wal_end += WAL_FRAME_SIZE as u64;
        self.wal_file.sync_all()?;
        Ok(())
    }

    pub fn checkpoint(&mut self) -> Result<(), NexoraStorageError> {
        let mut buf = [0u8; WAL_FRAME_SIZE];

        for i in 0..self.index_len {
            let (page_id, offset) = self.index[i];
            self.wal_file.seek(SeekFrom::Start(offset))?;
            self.wal_file.read_exact(&mut buf)?;
            let frame = WALFrame::ref_from_bytes(&buf[..])
                .map_err(|_| NexoraStorageError::CorruptWal)?;
            self.inner.write_page(PageId(page_id), &frame.page, false)?;
        }

        self.inner.sync()?;

        self.wal_file.set_len(WAL_HEADER_SIZE as u64)?;
        self.wal_end = WAL_HEADER_SIZE as u64;
        self.index_len = 0;

        Ok(())
    }
}

impl<S: PageStore> PageStore for WALPageStore<S> {
    fn read_page(&mut self, page_id: PageId, buf: &mut [u8; PAGE_SIZE], verify_checksum: bool) -> Result<(), NexoraStorageError> {
        if let Some(offset) = self.index_lookup(page_id.as_u64()) {
            let mut frame_buf = [0u8; WAL_FRAME_SIZE];
            self.wal_file.seek(SeekFrom::Start(offset))?;
            self.wal_file.read_exact(&mut frame_buf)?;
            let frame = WALFrame::ref_from_bytes(&frame_buf[..])
                .map_err(|_| NexoraStorageError::CorruptPage(page_id.as_u64()))?;
            buf.copy_from_slice(&frame.page);
            if verify_checksum {
                let stored = Self::get_checksum(buf);
                if !Self::verify_checksum(buf, stored) {
                    return Err(NexoraStorageError::ChecksumMismatch(page_id.as_u64()));
                }
            }
            Ok(())
        } else {
            self.inner.read_page(page_id, buf, verify_checksum)
        }
    }

    fn write_page(&mut self, page_id: PageId, buf: &[u8; PAGE_SIZE], stamp_checksum: bool) -> Result<(), NexoraStorageError> {
        let mut page = *buf;
        if stamp_checksum {
            Self::stamp_checksum(&mut page);
        }

        let offset = self.wal_end;
        self.wal_file.seek(SeekFrom::Start(offset))?;
        self.wal_file.write_all(WALFrame::data(page_id.as_u64(), &page).as_bytes())?;
        self.wal_end += WAL_FRAME_SIZE as u64;

        self.index_upsert(page_id.as_u64(), offset)
    }

    fn read_page_header_unchecked(&mut self, page_id: PageId) -> Result<NexoraPageHeader, NexoraStorageError> {
        if let Some(offset) = self.index_lookup(page_id.as_u64()) {
            let mut frame_buf = [0u8; WAL_FRAME_SIZE];
            self.wal_file.seek(SeekFrom::Start(offset))?;
            self.wal_file.read_exact(&mut frame_buf)?;
            let frame = WALFrame::ref_from_bytes(&frame_buf[..])
                .map_err(|_| NexoraStorageError::CorruptPage(page_id.as_u64()))?;
            NexoraPageHeader::ref_from_bytes(&frame.page[..PAGE_HEADER_SIZE])
                .map(|h| *h)
                .map_err(|_| NexoraStorageError::CorruptPage(page_id.as_u64()))
        } else {
            self.inner.read_page_header_unchecked(page_id)
        }
    }

    fn close(&mut self) -> Result<(), NexoraStorageError> {
        self.checkpoint()?;
        self.inner.close()
    }
}

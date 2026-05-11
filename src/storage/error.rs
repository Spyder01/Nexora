use thiserror::Error;

#[derive(Debug, Error)]
pub enum NexoraStorageError {
  #[error("I/O error: {0}")]
  Io(#[from] std::io::Error),

  #[error("invalid magic bytes — not a Nexora file")]
  InvalidMagicBytes,

  #[error("unsupported version: {0}")]
  UnsupportedVersion(u32),

  #[error("corrupt page: {0}")]
  CorruptPage(u64),

  #[error("invalid page type byte: {0}")]
  InvalidPageType(u8),

  #[error("checksum mismatch on page {0}")]
  ChecksumMismatch(u64),
}



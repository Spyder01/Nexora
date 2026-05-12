use thiserror::Error;

use crate::storage::error::NexoraStorageError;

#[derive(Debug, Error)]
pub enum NexoraGraphStringError {
  #[error("Storage error: {0}")]
  Storage(#[from] NexoraStorageError),

  #[error("No free slot to insert the string")]
  NoFreeSlot,

  #[error("String record does not exist in the page")]
  StringNotFound,

  #[error("Slot index out of bounds")]
  SlotNotFound,

  #[error("String exceeds maximum allowed length")]
  StringTooLong,

  #[error("Output buffer too small for string")]
  BufferTooSmall,
}



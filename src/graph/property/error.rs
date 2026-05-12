use thiserror::Error;

use crate::storage::error::NexoraStorageError;

#[derive(Debug, Error)]
pub enum NexoraGraphPropertyError {
    #[error("Storage error: {0}")]
    Storage(#[from] NexoraStorageError),

    #[error("Property page is full")]
    PageFull,

    #[error("Property record not found")]
    RecordNotFound,
}

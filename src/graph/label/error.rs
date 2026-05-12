use thiserror::Error;

use crate::storage::error::NexoraStorageError;
use crate::graph::string::error::NexoraGraphStringError;

#[derive(Debug, Error)]
pub enum NexoraGraphLabelError {
    #[error("Storage error: {0}")]
    Storage(#[from] NexoraStorageError),

    #[error("String error: {0}")]
    StringError(#[from] NexoraGraphStringError),

    #[error("Label not found")]
    LabelNotFound,

    #[error("Label page is full")]
    PageFull,
}

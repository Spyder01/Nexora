use thiserror::Error;

use crate::storage::error::NexoraStorageError;

#[derive(Debug, Error)]
pub enum NexoraGraphNodeError {
  #[error("Storage error: {0}")]
  Storage(#[from] NexoraStorageError),

  #[error("No free slot to insert the node")]
  NoFreeSlot,

  #[error("Node record does not exist in the page")]
  NodeNotFound,
}



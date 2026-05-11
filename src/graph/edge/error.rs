use thiserror::Error;

use crate::storage::error::NexoraStorageError;

#[derive(Debug, Error)]
pub enum NexoraGraphEdgeError {
  #[error("Storage error: {0}")]
  Storage(#[from] NexoraStorageError),

  #[error("No free slot to insert the edge")]
  NoFreeSlot,

  #[error("Edge record does not exist")]
  EdgeNotFound,

  #[error("Node referenced by edge does not exist")]
  NodeNotFound,
}



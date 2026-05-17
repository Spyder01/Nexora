use thiserror::Error;

use crate::storage::error::NexoraStorageError;
use crate::graph::node::error::NexoraGraphNodeError;

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

impl From<NexoraGraphNodeError> for NexoraGraphEdgeError {
    fn from(e: NexoraGraphNodeError) -> Self {
        match e {
            NexoraGraphNodeError::NodeNotFound | NexoraGraphNodeError::NoFreeSlot => NexoraGraphEdgeError::NodeNotFound,
            NexoraGraphNodeError::Storage(s) => NexoraGraphEdgeError::Storage(s),
        }
    }
}



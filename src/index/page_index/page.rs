use zerocopy::{FromBytes, IntoBytes, Immutable, KnownLayout};
use zerocopy::byteorder::little_endian::{U64, U16};

use crate::index::page_index::constants::{PAGE_INDEX_HEADER_SIZE, NODE_PAGE_INDEX_RECORD_SIZE, MAX_NODE_PAGE_INDEX_RECORD_COUNT};

use crate::storage::models::{NexoraPageHeader};
use crate::storage::constants::{PAGE_SIZE, PAGE_HEADER_SIZE};


pub const PAGE_INDEX_HEADER_PADDING: usize = PAGE_INDEX_HEADER_SIZE - 2;

#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct NodePageIndexHeader {
    pub page_dir_count: U16,

    _pad: [u8; PAGE_INDEX_HEADER_PADDING],
}

const _: () = assert!(
    std::mem::size_of::<NodePageIndexHeader>() == PAGE_INDEX_HEADER_SIZE,
    "PageIndexHeader must be size of PAGE_INDEX_HEADERE_SIZE"
);


pub const NODE_PAGE_INDEX_RECORD_PADDING: usize = NODE_PAGE_INDEX_RECORD_SIZE - 8;

#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct NodePageIndexRecord {
   pub page_id: U64,

   _pad: [u8; NODE_PAGE_INDEX_RECORD_PADDING],
}

const _: () = assert!(
    std::mem::size_of::<NodePageIndexRecord>() == NODE_PAGE_INDEX_RECORD_SIZE,
    "NodePageIndexRecord must be size of NODE_PAGE_INDEX_RECORD_SIZE"
);

pub const NODE_PAGE_INDEX_PAGE_PADDING: usize = PAGE_SIZE - PAGE_HEADER_SIZE - PAGE_INDEX_HEADER_SIZE - MAX_NODE_PAGE_INDEX_RECORD_COUNT*NODE_PAGE_INDEX_RECORD_SIZE;

#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct NodePageIndexPage {
    pub page_header:            NexoraPageHeader,
    pub node_page_index_header: NodePageIndexHeader,
    pub records:                [NodePageIndexRecord; MAX_NODE_PAGE_INDEX_RECORD_COUNT],

    _pad: [u8; NODE_PAGE_INDEX_PAGE_PADDING],
}


const _: () = assert!(
    std::mem::size_of::<NodePageIndexPage>() == PAGE_SIZE,
    "NodePageIndexPage must be size of PAGE_SIZE"
);


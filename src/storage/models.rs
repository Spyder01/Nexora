use core::result::Result::Ok;

use zerocopy::{FromBytes, IntoBytes, Immutable, KnownLayout};
use zerocopy::byteorder::little_endian::{U32, U64};

use crate::storage::constants::{PAGE_HEADER_SIZE, PAGE_SIZE, PAGE_SIZE_SHIFT, SENTINEL_PAGE_ID};


#[derive(Debug, Copy, Clone, PartialEq, Hash, Eq)]
pub struct PageId(pub u64);

impl PageId {
    pub fn as_u64(self) -> u64 {
        self.0
    }

    pub const fn byte_offset(self) -> u64 {
        return self.0 << PAGE_SIZE_SHIFT;
    }
}

#[derive(Debug)]
#[repr(u8)]
pub enum PageType {
  Header   = 0,
  Footer   = 1,
  Node     = 2,
  Edge     = 3,
  Overflow = 4,
  PageIndex = 5,
  Label    = 6,
  Free     = 7,
  String   = 8,
  Property = 9,
  LabelString = 10,
}

impl TryFrom<u8> for PageType {
  type Error = u8;

  fn try_from(value: u8) -> Result<Self, Self::Error> {
      match value {
          0 => Ok(PageType::Header),
          1 => Ok(PageType::Footer),
          2 => Ok(PageType::Node),
          3 => Ok(PageType::Edge),
          4 => Ok(PageType::Overflow),
          5 => Ok(PageType::PageIndex),
          6 => Ok(PageType::Label),
          7 => Ok(PageType::Free),
          8 => Ok(PageType::String),
          9 => Ok(PageType::Property),
          10 => Ok(PageType::LabelString),
          unknown => Err(unknown),
      }
  }
}


#[derive(Debug, FromBytes, IntoBytes, Immutable, KnownLayout, Clone, Copy)]
#[repr(C)]
pub struct NexoraPageHeader {
    pub page_id: U64,

    // For Header next page_id is always footer.
    pub next_page_id: U64,
    pub prev_page_id: U64,

    pub checksum: U32,

    pub page_type: u8,

    pub _pad: [u8; PAGE_HEADER_SIZE - 8 - 8 - 8 - 4 - 1],
}

const _: () = assert!(
    std::mem::size_of::<NexoraPageHeader>() == PAGE_HEADER_SIZE,
    "NexoraHeader must be exactly one page."
);

impl NexoraPageHeader {
    pub const fn init(page_id: u64) -> Self {
        Self {
            page_id:      U64::new(page_id),
            next_page_id: U64::new(SENTINEL_PAGE_ID),
            prev_page_id: U64::new(SENTINEL_PAGE_ID),
            checksum:     U32::new(0),
            page_type:    PageType::Header as u8,
            _pad:         [0u8; PAGE_HEADER_SIZE - 8 - 8 - 8 - 4 - 1],
        }
    } 
}


#[derive(Debug, FromBytes, IntoBytes, Immutable, KnownLayout, Clone, Copy)]
#[repr(C)]
pub struct NexoraHeader {
    pub page_header:    NexoraPageHeader,
    pub version:        U32,
    pub page_size:      U32,
    pub footer_page_id: U64,
    pub magic:          [u8; 4],
    pub _pad:           [u8; PAGE_SIZE - PAGE_HEADER_SIZE - 4 - 4 - 8 - 4],
}

const _: () = assert!(
    std::mem::size_of::<NexoraHeader>() == PAGE_SIZE,
    "NexoraHeader must be exactly one page."
);

pub const INITIAL_HEADER: NexoraHeader = NexoraHeader {
    page_header:    NexoraPageHeader::init(0),
    version:        U32::new(1),
    page_size:      U32::new(PAGE_SIZE as u32),
    footer_page_id: U64::new(1),
    magic:          *b"NXRA",
    _pad:           [0u8; PAGE_SIZE - PAGE_HEADER_SIZE - 4 - 4 - 8 - 4],
};

const NEXORA_FOOTER_PAGE_PADDING: usize = PAGE_SIZE - PAGE_HEADER_SIZE - 8 - 8 - 8 - 8 - 8 - 8 - 8 - 8 - 8 - 8 - 8 - 8 - 8 - 8 - 8 - 8 - 8 - 8 - 8 - 8 - 8;


#[derive(Debug, FromBytes, IntoBytes, Immutable, KnownLayout, Copy, Clone)]
#[repr(C)]
pub struct NexoraFooter {
    pub page_header:                 NexoraPageHeader,

    pub node_count:                  U64,
    pub first_node_page:             U64,
    pub last_node_page:              U64,
    pub next_node_id:                U64,

    pub edge_count:                  U64,
    pub first_edge_page:             U64,
    pub last_edge_page:              U64,
    pub next_edge_id:                U64,

    pub overflow_elements_count:     U64,
    pub first_overflow_element_page: U64,

    pub page_indices_count:          U64,
    pub first_page_index_page:       U64,

    pub free_pages_count:            U64,
    pub first_free_page:             U64,

    pub first_string_page:           U64,
    pub last_string_page:            U64,

    pub label_pages_count:           U64,
    pub first_label_page:            U64,
    pub first_label_string_page:     U64,

    pub first_property_page:         U64,

    pub page_count:                  U64,

    pub _pad: [u8; NEXORA_FOOTER_PAGE_PADDING],
}

const _: () = assert!(
    std::mem::size_of::<NexoraFooter>() == PAGE_SIZE,
    "NexoraFooter must be exactly one page."
);

pub const INITIAL_FOOTER: NexoraFooter = NexoraFooter {
    page_header: NexoraPageHeader {
        page_id:      U64::new(1),
        next_page_id: U64::new(SENTINEL_PAGE_ID),
        prev_page_id: U64::new(SENTINEL_PAGE_ID),
        checksum:     U32::new(0),
        page_type:    PageType::Footer as u8,
        _pad:         [0u8; PAGE_HEADER_SIZE - 8 - 8 - 8 - 4 - 1],
    },
    node_count:                  U64::new(0),
    first_node_page:             U64::new(SENTINEL_PAGE_ID),
    last_node_page:              U64::new(SENTINEL_PAGE_ID),
    next_node_id:                U64::new(0),
    edge_count:                  U64::new(0),
    first_edge_page:             U64::new(SENTINEL_PAGE_ID),
    last_edge_page:              U64::new(SENTINEL_PAGE_ID),
    next_edge_id:                U64::new(0),
    overflow_elements_count:     U64::new(0),
    first_overflow_element_page: U64::new(SENTINEL_PAGE_ID),
    page_indices_count:          U64::new(0),
    first_page_index_page:       U64::new(SENTINEL_PAGE_ID),
    free_pages_count:            U64::new(0),
    first_free_page:             U64::new(SENTINEL_PAGE_ID),
    first_string_page:           U64::new(SENTINEL_PAGE_ID),
    last_string_page:            U64::new(SENTINEL_PAGE_ID),
    label_pages_count:           U64::new(0),
    first_label_page:            U64::new(SENTINEL_PAGE_ID),
    first_label_string_page:     U64::new(SENTINEL_PAGE_ID),
    first_property_page:         U64::new(SENTINEL_PAGE_ID),
    page_count:                  U64::new(2), // pages 0 (header) and 1 (footer) already used
                                              
    _pad:                        [0u8; NEXORA_FOOTER_PAGE_PADDING],
};



# Notes

## Architecture
- Write-back footer strategy: footer holds file-state (node/edge counts, index roots, free list), header holds file-identity (magic bytes, version, page size, pointer to footer page).
- File is divided into fixed-size pages (4096 bytes, aligned to OS page size). Every read/write operates on whole pages — never partial.
- Footer uses Floating page: header stores the footer's page ID. When footer outgrows one page, allocate a new page, write footer there, update header pointer.
- File open requires exactly two page reads: Page 0 (header) → footer page. All critical graph state must fit in one footer page.
- Footer is marked dirty in memory on every mutation; flushed to disk on commit or close (not on every write).
- Page addressing is O(1): byte_offset = page_id * PAGE_SIZE. Fixed page size makes seeking to any page instant.
- Free list tracked in footer: deleted pages are not erased, they go onto a free list and are reclaimed before appending new pages.
- Binary (main.rs) stays thin; all engine logic lives in the library crate (lib.rs) so Nexora is embeddable like SQLite.
- Start with flat module structure (one file per major concern). Convert to nested directory modules only when a file grows too large or its internals need isolation.
- Pages are typed: NodePage, EdgePage, IndexPage, OverflowPage, FreePage — each kind forms a doubly linked list (next/prev page IDs in every page header). Footer holds the head of each list. Free list is singly linked (pop from front only).
- PageType is stored as a raw u8 on disk (zerocopy struct); converted to a #[repr(u8)] enum in logic via TryFrom. Enum cannot derive FromBytes because not all 256 u8 values are valid variants.
- PageStore is a trait with two methods only: read_page (caller-owned buffer) and write_page. Both return Result<(), NexoraError>.
- StorageManager<S: PageStore> is generic (zero overhead, compile-time dispatch). It owns footer state, dirty flag, allocate_page, free_page, and flush — none of these are on the trait.
- Two implementations: RegularPageStore (read_at/write_at syscalls) and MmapPageStore (memory-mapped). MmapPageStore auto-remaps internally when the file grows — caller never sees this.
- Construction (create/open) is outside the trait — each impl has its own constructor. Trait is purely a page I/O primitive.
- Custom NexoraError enum wraps std::io::Error and adds engine-specific variants: InvalidMagicBytes, UnsupportedVersion, CorruptPage, InvalidPageType.

## Graph Architecture

### Layer Stack
```
Application
    ↓
GraphStore          — interprets page contents (nodes, edges, slots)
    ↓
StorageManager      — manages page lifecycle (alloc, free, footer, checksums)
    ↓
PageStore trait     — raw I/O (read/write bytes)
    ↓
RegularPageStore    — disk implementation
```
- GraphStore owns interpretation of node/edge/property pages. StorageManager never looks inside them.
- GraphStore calls `storage_manager.allocate_page()` for new pages — never raw file I/O.
- Both layers still think in pages and bytes; the boundary is about who interprets what.

### Module Structure (src/graph/)
```
src/graph/
  mod.rs
  models.rs         — NodeId, EdgeId, Node, Edge, Property
  node_store.rs     — read/write node pages
  edge_store.rs     — read/write edge pages
  graph_store.rs    — public API: create_node, create_edge, get_node, neighbors
```

### Node Page Layout
- No slot array — node records are fixed-size, so offset is pure arithmetic: `record[i] = page_data_start + (i * RECORD_SIZE)`
- Structure: `[ NexoraPageHeader 32B | NodePageHeader 32B | record[0] | record[1] | ... ]`
- `NodePageHeader` fields: `record_count`, `max_records`, `min_node_id`, `max_node_id` (zone map for page skipping)
- `min_node_id`/`max_node_id` act as a zone map — skip pages whose range doesn't contain the target ID

### Node Record Layout (40 bytes, fixed-size)
```
node_id:          u64   — 8 bytes   (offset 0)
first_out_edge:   u64   — 8 bytes   (offset 8)   page_id of first outgoing edge
first_in_edge:    u64   — 8 bytes   (offset 16)  page_id of first incoming edge
property_page_id: u64   — 8 bytes   (offset 24)
label_id:         u32   — 4 bytes   (offset 32)  points to label string (e.g. "Person")
property_slot:    u16   — 2 bytes   (offset 36)
flags:            u8    — 1 byte    (offset 38)  bitfield: deleted, has_overflow, etc.
_pad:             u8    — 1 byte    (offset 39)
```
- Fields ordered largest-to-smallest to avoid internal padding gaps
- Tail padded to 40 bytes (multiple of 8 for alignment)
- Records per page: (4096 - 32 - 32) / 40 = 100 nodes per page

### Property Storage
- Properties are stored in a dedicated PropertyPage, not inline in the node record
- Node record holds `property_page_id` + `property_slot` as a pointer into the property chain
- Benefit: node records stay fixed-size; property chains can grow without touching node pages
- Cost: every property read is a second page read (mitigated by buffer pool cache in Phase 3)

### Edge Storage
- Each node holds `first_out_edge` and `first_in_edge` pointers (page_id of first edge in chain)
- Edges stored as adjacency chains — each edge record has `next_outgoing`/`next_incoming` pointers
- Traversal cost: O(degree(N)), not O(E) — never scans the whole file

### Fixed-Size Record Performance
- Predictable offsets enable O(1) arithmetic access — no slot indirection
- 40-byte records fit near one 64-byte CPU cache line — cache friendly on traversal
- Sequential scans allow CPU prefetcher to predict next record location
- In-place updates: updating a node is a single write to a known offset
- Deletion: tombstone via `flags` field (mark deleted, reclaim on compaction) — no shifting
- Mirrors Neo4j's original design (15-byte fixed node records) which achieved ~1B traversals/sec

## Above discussed trick

- STRING Page todo: String page slots double as a free list for the data area. When a slot is deleted, its `offset` and `chunk_length` fields describe a free region of the data area. On insert, scan free slots (via `~occupied & bounds_mask`) for one with `chunk_length >= new_chunk_size`. Reuse it — write new string bytes at `offset`. If new string is smaller, create a new free slot pointing to the leftover region (`offset + new_chunk_size`, `chunk_length = leftover`). If no suitable free slot, fall back to appending normally. Limitation: external fragmentation (small free slots accumulate, adjacent free regions can't easily be coalesced). Implement this only after basic insert/read is working correctly.

## Optimizations

- SIMD bitset scans: `Bitset256` is `[U64; 4]` = 256 bits, fitting exactly in an AVX2 YMM register. `first_zero()`/`first_set()` currently loop over 4 × 64-bit words sequentially. With AVX2, all 256 bits load in one `VMOVDQU`, compare in one `VPCMPEQQ`, collapse to a mask in one `VPMOVMSKB` — finding the first non-full lane in ~3 instructions instead of a loop. The 128-bit `U128` in node page headers maps to SSE2 XMM registers (baseline x86-64, no feature gate needed). The natural atom is 64 bits — `TZCNT` on a `u64` is already one instruction that processes all 64 bits in hardware. Nothing below 64 bits benefits. Gate AVX2 paths behind `#[cfg(target_feature = "avx2")]` with scalar fallback.
- SIMD for `find_frame` in BufferStore: scanning 256 `page_ids` ([u64; 256] = 2048 bytes) to find a matching PageId is the right place for SIMD — 256 comparisons with no I/O between them. AVX2 can compare 4 × u64 per instruction. `flush_all` is NOT a SIMD candidate — the bottleneck there is the disk write syscall inside each `flush_frame`, not the dirty bit scan (4 × u64 words, nanoseconds vs microseconds for I/O).

## Future Optimization
- Inline string optimization for GraphPropertyRecord only (not labels). Replace key_ptr/val_ptr PackedPtr (8 bytes each) with 24-byte inline string headers: [len: 4 | buffer: 12 | ptr: 8]. Short strings (≤12 bytes) stored inline with zero page lookups. Long strings store 4-byte prefix inline + PackedPtr to page chain. Labels stay as label_id (u32) — already deduplicated, no benefit from inlining. Benefit: eliminates page lookups for short property keys/values (name, age, weight etc). Cost: PropertyRecord grows from 24 to 56 bytes, records per page drops from ~170 to ~73.

## Heap Allocations
- `BufferStore::frames` and `page_ids` are `Box<[Frame]>` and `Box<[PageId]>` — heap-allocated via `vec![...].into_boxed_slice()` in `BufferStore::new()`. Necessary because the frame data (8MB at current pool size of `1 << 23`) cannot live in the struct body without causing stack overflows wherever `BufferStore` is held by value inside `StorageManager`. Future: if pool size becomes runtime-configurable, this also enables dynamic sizing at construction time without changing the type signature.

## Future Cleanup
- `close()` on `PageStore` trait is a design smell — lifecycle is not an I/O primitive concern. Options: (1) second trait bound `S: PageStore + Closeable`; (2) implement `Drop` on `BufferStore` to flush dirty frames — guarantees flush even if caller forgets `close()`, but `Drop` cannot return `Result` so I/O errors are silently swallowed. Refactor when design hardens.

## TODO
- Implement MmapPageStore using the `mmap2` Rust crate. Pre-allocate a large virtual address space upfront to avoid remapping on every page allocation. Use MAP_SHARED so writes go back to the file. Call msync() on flush/close for crash safety.
- Optimize `insert_node` page traversal: read only `NexoraPageHeader + GraphNodePageHeader` (80 bytes) first to check free slots via zone map and bitset, then read the full 4KB page only if a free slot exists. Avoids 4KB reads for full pages during chain traversal. Use a new `read_node_page_header` method alongside the existing `read_page_header_unchecked`. Only meaningful when the chain contains many full pages.

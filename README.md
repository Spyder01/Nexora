# Nexora

A native graph database engine in a single file — embeddable, zero external dependencies, analogous to SQLite but built around a graph data model rather than relational tables.

> **Status:** Early development. Core storage, graph primitives, traversal APIs, and Lua scripting are functional. WAL and a query language are planned.

**[→ Full user documentation](DOC.md)**  
**[→ Blog series: Building Nexora](https://devtales.suhan.art/tags/nexora/)**

---

## What it is

- **Single-file storage** — all graph data lives in one `.nxr` file. No server process, no install.
- **Graph data model** — nodes and directed edges, each with arbitrary key-value properties and a label. No schemas, no tables.
- **Embeddable library** — use as a Rust crate. The CLI binary is a thin wrapper.
- **No heap allocation in the storage layer** — fixed-size pages, stack buffers, `zerocopy` for safe layout. Heap allocations are a deliberate exception at the public API boundary only.

---

## Quick start

```rust
use nexora::graph::graphstore::graphstore::GraphStore;

let mut db = GraphStore::create(Path::new("my_graph.nxr"))?;

// Nodes
let alice = db.insert_node("Person")?;
let bob   = db.insert_node("Person")?;

// Edges
db.insert_edge(alice, bob, "KNOWS", 1.0)?;

// Properties
db.set_node_property(alice, "name", "Alice")?;

// Traversal
let mut cursor = db.outgoing_cursor(alice)?;
while let Some(edge) = db.next_outgoing(&mut cursor)? {
    println!("{} --[{}]--> {}", edge.src, edge.label, edge.dst);
}

db.close()?;
```

---

## CLI

```bash
# Interactive REPL
nexora mydb.nxr

# Run a Lua script file non-interactively
nexora exec mydb.nxr seed.lua

# Evaluate an inline Lua expression
nexora eval mydb.nxr "print(db:get_node(0).label)"

# Force-create a new database (fails if file exists)
nexora exec --new mydb.nxr seed.lua
```

`exec` and `eval` run inside the same sandbox as the REPL — `os`, `io`, and `require` are not available. Output only comes from explicit `print()` calls.

---

## Building

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Build with Lua REPL / CLI
cargo build --features lua

# Run tests
cargo test

# Run a single test
cargo test test_insert_and_get_label

# Benchmarks
cargo bench --bench graph_bench

# Lint / format
cargo clippy
cargo fmt
```

---

## Project layout

```
src/
├── lib.rs                        # Public crate root
├── main.rs                       # CLI entry point (requires --features lua)
│
├── graph/
│   ├── graphstore/graphstore.rs  # GraphStore — the main public API
│   ├── node/                     # Node page layout, insert/get/delete/scan
│   ├── edge/                     # Edge page layout, insert/get/delete, cursor traversal
│   ├── label/                    # Label deduplication, LabelStore
│   ├── property/                 # Property chain (key-value linked list per node/edge)
│   ├── string/                   # Variable-length string storage with overflow chaining
│   │   └── label_string_store.rs # Dedicated string store for label strings (separate chain)
│   ├── record/types.rs           # PackedPtr, RecordCursor, Bitset256
│   └── models.rs                 # Node, Edge, Property public structs
│
├── storage/
│   ├── storage_manager.rs        # Footer/header management, page allocation
│   ├── page_store.rs             # PageStore trait (read/write/header)
│   ├── page_store_disk.rs        # Disk-backed implementation (cross-platform)
│   └── models.rs                 # NexoraHeader, NexoraFooter, PageType, PageId
│
├── buffer_pool/                  # Fixed-capacity in-memory page cache
├── index/page_index/             # Node page index for O(1) node→page lookup
├── api/traversal/                # High-level traversal API (in progress)
└── lua/                          # Lua scripting REPL (--features lua)
```

---

## Public API

All interaction goes through `GraphStore<S: PageStore>`. The disk-backed convenience constructors are on `GraphStore<RegularPageStore>`:

```rust
GraphStore::create(path) -> Result<GraphStore<RegularPageStore>, _>
GraphStore::open(path)   -> Result<GraphStore<RegularPageStore>, _>
GraphStore::close(&mut self) -> Result<(), _>
```

### Nodes

| Method | Description |
|---|---|
| `insert_node(label)` | Insert a node, returns `node_id: u64` |
| `get_node(node_id)` | Returns `Node { id, label }` |
| `update_node(&node)` | Update label |
| `delete_node(node_id)` | Delete node and all its edges + properties |

### Edges

| Method | Description |
|---|---|
| `insert_edge(src, dst, label, weight)` | Insert directed edge, returns `edge_id: u64` |
| `get_edge(edge_id)` | Returns `Edge { id, src, dst, label, weight }` |
| `update_edge(&edge)` | Update label and weight |
| `delete_edge(edge_id)` | Delete edge and its properties |

### Traversal

```rust
// Outgoing edges from a node
let mut cursor = db.outgoing_cursor(node_id)?;
while let Some(edge) = db.next_outgoing(&mut cursor)? { ... }

// Incoming edges to a node
let mut cursor = db.incoming_cursor(node_id)?;
while let Some(edge) = db.next_incoming(&mut cursor)? { ... }

// Full scan
let mut cursor = db.all_nodes_cursor()?;
while let Some(node) = db.next_node(&mut cursor)? { ... }

let mut cursor = db.all_edges_cursor()?;
while let Some(edge) = db.next_edge(&mut cursor)? { ... }

// All nodes with a given label (scan-based, callback returns false to stop early)
db.for_each_with_label("Person", |node| {
    println!("{}", node.id);
    true // return false to stop
})?;
```

Higher-level algorithms (BFS, DFS, shortest path, has_path) are available through the `TraverseApi` trait:

```rust
use nexora::api::traversal::{Traversal, TraverseApi, Visit};

Traversal::new(&mut db).bfs(start, max_depth, |node, depth| {
    println!("{}: {}", depth, node.label);
    Visit::Continue
})?;

Traversal::new(&mut db).for_each_with_label("Person", |node| {
    println!("{}", node.id);
    Visit::Continue
})?;
```

### Properties

```rust
db.set_node_property(node_id, "age", "30")?;
let mut buf = [0u8; 64];
db.get_node_property(node_id, "age", &mut buf)?;
db.delete_node_property(node_id, "age")?;

// Same methods for edges: set_edge_property / get_edge_property / delete_edge_property

// Iterate all properties
let mut cursor = db.node_properties_cursor(node_id)?;
while let Some(prop) = db.next_property(&mut cursor)? {
    println!("{} = {}", prop.key, prop.value);
}
```

---

## File format

The `.nxr` file is a sequence of 4 KB pages. Every page begins with a `NexoraPageHeader` (type + next-page pointer + CRC32 checksum). Page 0 is the file header; page 1 is the footer (contains root pointers for each chain). All multi-byte integers use little-endian layout via `zerocopy`.

```
Page 0:  FileHeader  (magic, version)
Page 1:  Footer      (first_node_page, first_edge_page, first_label_page, ...)
Page 2+: Data pages  (Node | Edge | Label | String | LabelString | Property | ...)
```

Labels are deduplicated — inserting the same label string twice returns the same `label_id`. Label strings live on a separate page chain from property strings.

---

## Benchmarks

Run with:

```bash
cargo bench --bench graph_bench
# Or with a higher iteration cap
NEXORA_BENCH_MAX=100000 cargo bench --bench graph_bench
```

Criterion HTML reports are written to `target/criterion/` and uploaded as CI artifacts on every push to `main`.

---

## CI

Two workflows run on push/PR to `main`:

| Workflow | File | What it does |
|---|---|---|
| CI | `.github/workflows/ci.yml` | Tests + release builds on Linux and Windows; uploads binaries as artifacts |
| Benchmarks | `.github/workflows/bench.yml` | Criterion benchmarks on Linux; uploads HTML report |

---

## Design constraints

- **No heap allocation in storage/graph layers** — `Vec`, `String`, `Box` etc. require explicit justification and approval before introduction. Known approved exceptions are in `graphstore.rs` (label/string resolution at the public API boundary, marked `TODO(phase3)`).
- **No external server** — the crate is a library; the only binary is the optional Lua REPL.
- **Cross-platform** — storage I/O uses `std::io::{Seek, Read, Write}` (no Unix-specific syscalls).

---

## License

MIT — see [LICENSE](LICENSE).

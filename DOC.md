# Nexora Documentation

> This document covers the current stable surface. It will be updated as new features land.

---

## Table of contents

1. [Getting started](#1-getting-started)
2. [The REPL](#2-the-repl)
3. [Nodes](#3-nodes)
4. [Edges](#4-edges)
5. [Properties](#5-properties)
6. [Traversal](#6-traversal)
7. [Scanning the full graph](#7-scanning-the-full-graph)
8. [Multi-hop traversal patterns](#8-multi-hop-traversal-patterns)
9. [Persisting data](#9-persisting-data)
10. [File format](#10-file-format)

---

## 1. Getting started

### Build

```bash
git clone <repo>
cd nexora
cargo build --release --features lua
```

The compiled binary is at `target/release/nexora`.

### Start the REPL

```bash
# Open or create nexora.nxr in the current directory
nexora

# Open or create a named file
nexora mydb.nxr

# Force-create a new file (fails if it already exists)
nexora --new mydb.nxr

# Help
nexora --help
```

The REPL prints a banner and drops you into a Lua 5.4 session:

```
  Nexora 0.1.0  —  embedded graph database
  ─────────────────────────────────────────
  db      mydb.nxr  [created]
  engine  Lua 5.4
  hint    type help() for API reference, Ctrl-D to quit

nexora>
```

Type `help()` at any time to print the full API reference. Press **Ctrl-D** to quit.

---

## 2. The REPL

### Variables

Variables persist across lines — you do not need to re-declare them:

```lua
local a = db:insert_node("Person")   -- 'local' is stripped automatically
local b = db:insert_node("City")
db:insert_edge(a, b, "LIVES_IN", 1.0)  -- a and b are still in scope
```

### Expressions are printed automatically

Bare expressions print their return value without needing `print()`:

```lua
nexora> db:get_node(0)
{ id: 0, label: "Person" }

nexora> 2 + 2
4
```

### Multi-line input

The REPL detects incomplete blocks and keeps collecting lines:

```lua
nexora> for i = 1, 3 do
  ...  >   print(i)
  ...  > end
1
2
3
```

### Ctrl-C vs Ctrl-D

- **Ctrl-C** — cancels the current (possibly incomplete) line, returns to a fresh prompt
- **Ctrl-D** — closes the session and flushes the database to disk

---

## 3. Nodes

A node has an **id** (assigned automatically) and a **label** (a string you choose).

### Insert

```lua
local alice = db:insert_node("Person")
local london = db:insert_node("City")
print(alice, london)   -- 0   1
```

### Get

```lua
local node = db:get_node(alice)
print(node.id, node.label)   -- 0   Person
```

### Update label

```lua
db:update_node(alice, "Developer")
db:get_node(alice).label   -- "Developer"
```

### Delete

Deleting a node also deletes all its incident edges and their properties.

```lua
db:delete_node(alice)
db:get_node(alice)   -- error: node not found
```

---

## 4. Edges

An edge is **directed** — it goes from a source node to a destination node. It has an **id**, a **label**, and a floating-point **weight**.

### Insert

```lua
local alice  = db:insert_node("Person")
local bob    = db:insert_node("Person")
local london = db:insert_node("City")

local e1 = db:insert_edge(alice, bob,    "KNOWS",    1.0)
local e2 = db:insert_edge(alice, london, "LIVES_IN", 0.0)
```

### Get

```lua
local e = db:get_edge(e1)
print(e.id, e.src, e.dst, e.label, e.weight)
-- 0   0   1   KNOWS   1.0
```

### Update label and weight

```lua
db:update_edge(e1, "FRIENDS_WITH", 5.0)
```

### Delete

```lua
db:delete_edge(e1)
```

---

## 5. Properties

Any node or edge can have any number of key-value properties. Both keys and values are strings.

### Node properties

```lua
local id = db:insert_node("Person")

db:set_node_property(id, "name", "Alice")
db:set_node_property(id, "age",  "30")

db:get_node_property(id, "name")   -- "Alice"
db:get_node_property(id, "missing")  -- nil

db:delete_node_property(id, "age")   -- true (found and deleted)
db:delete_node_property(id, "age")   -- false (already gone)
```

Calling `set_node_property` with a key that already exists **overwrites** the value:

```lua
db:set_node_property(id, "name", "Bob")
db:get_node_property(id, "name")   -- "Bob"
```

### Edge properties

Same API, prefixed with `edge`:

```lua
local eid = db:insert_edge(a, b, "KNOWS", 1.0)

db:set_edge_property(eid, "since", "2020")
db:get_edge_property(eid, "since")   -- "2020"
db:delete_edge_property(eid, "since")
```

### Iterate all properties on a node or edge

```lua
local cursor = db:node_properties_cursor(id)
while true do
    local prop = db:next_property(cursor)
    if not prop then break end
    print(prop.key, "=", prop.value)
end
```

---

## 6. Traversal

Traversal uses a **cursor** — an opaque handle that steps through a linked list of edges one at a time.

### Outgoing edges (edges leaving a node)

```lua
local cursor = db:outgoing_cursor(alice)
while true do
    local edge = db:next_outgoing(cursor)
    if not edge then break end
    print(edge.src, "──"..edge.label.."──▶", edge.dst, "w="..edge.weight)
end
```

### Incoming edges (edges arriving at a node)

```lua
local cursor = db:incoming_cursor(london)
while true do
    local edge = db:next_incoming(cursor)
    if not edge then break end
    print(edge.src, "──▶", edge.dst)
end
```

### Cursor fields

Each edge returned by `next_outgoing` / `next_incoming` has:

| Field | Type | Description |
|---|---|---|
| `id` | integer | Edge id |
| `src` | integer | Source node id |
| `dst` | integer | Destination node id |
| `label` | string | Edge label |
| `weight` | number | Edge weight |

---

## 7. Scanning the full graph

### All nodes

```lua
local cursor = db:all_nodes_cursor()
while true do
    local node = db:next_node(cursor)
    if not node then break end
    print(node.id, node.label)
end
```

### All edges

```lua
local cursor = db:all_edges_cursor()
while true do
    local edge = db:next_edge(cursor)
    if not edge then break end
    print(edge.src, "──"..edge.label.."──▶", edge.dst)
end
```

Deleted nodes and edges are skipped automatically.

---

## 8. Multi-hop traversal patterns

Nexora provides built-in traversal methods on `db` for common graph algorithms. All callback-based methods receive node/edge data as a table; return `false` to stop early, or return nothing to continue.

### for_each_outgoing / for_each_incoming

Visit every edge leaving (or arriving at) a node, one hop at a time.

```lua
db:for_each_outgoing(alice, function(edge)
    print(edge.src, "──"..edge.label.."──▶", edge.dst)
end)

db:for_each_incoming(london, function(edge)
    print(edge.src, "──▶", edge.dst)
    if edge.label == "LIVES_IN" then return false end  -- stop early
end)
```

Each `edge` table has: `id`, `src`, `dst`, `label`, `weight`.

### bfs

Breadth-first search from `start`, visiting nodes level by level up to `max_depth` hops. Each node is visited at most once.

```lua
db:bfs(alice, 3, function(node, depth)
    print(string.rep("  ", depth) .. "#"..node.id.." "..node.label)
end)
```

The callback receives a `node` table (`id`, `label`) and the integer `depth` (0 = start node).

### dfs

Depth-first search from `start`, up to `max_depth` hops.

```lua
db:dfs(alice, 3, function(node, depth)
    print(string.rep("  ", depth) .. node.label)
end)
```

### has_path

Returns `true` if a directed path exists from `src` to `dst`. A node is always reachable from itself.

```lua
if db:has_path(alice, bob) then
    print("connected")
end
```

### shortest_path

Returns the shortest directed path (fewest hops) as an ordered array of node IDs, or `nil` if no path exists.

```lua
local path = db:shortest_path(alice, bob)
if path then
    for i, id in ipairs(path) do
        print(i, db:get_node(id).label)
    end
else
    print("no path")
end
```

### Follow a specific edge label (manual pattern)

When you need to filter by label during traversal, use `for_each_outgoing` with a queue yourself:

```lua
function follow(start, label, max_depth)
    local visited = {}
    local queue   = {{id=start, depth=0}}
    visited[start] = true

    while #queue > 0 do
        local item = table.remove(queue, 1)
        local node = db:get_node(item.id)
        print(string.rep("  ", item.depth) .. node.label.." #"..item.id)

        if item.depth < max_depth then
            db:for_each_outgoing(item.id, function(edge)
                if edge.label == label and not visited[edge.dst] then
                    visited[edge.dst] = true
                    table.insert(queue, {id=edge.dst, depth=item.depth+1})
                end
            end)
        end
    end
end

follow(0, "KNOWS", 2)
```

> **Note:** The built-in `bfs`/`dfs` traverse all edge labels. Use the manual pattern above when you need to filter by label.

---

## 9. Persisting data

Data is flushed to disk when you **quit the REPL** (Ctrl-D) or when your Rust code calls `db.close()`. Opening the same file again restores everything exactly:

```bash
nexora mydb.nxr      # session 1 — insert some nodes
# Ctrl-D

nexora mydb.nxr      # session 2 — data is still there
nexora> db:get_node(0)
{ id: 0, label: "Person" }
```

> **Warning:** Killing the process with Ctrl-C or a crash before Ctrl-D may leave the file in a partially written state. A WAL (write-ahead log) is planned for a future release.

---

## 10. File format

A `.nxr` file is a sequence of fixed-size **4 KB pages**. Each page begins with a header containing the page type, a next-page pointer, and a CRC32 checksum.

```
Page 0   FileHeader   magic bytes, format version
Page 1   Footer       root pointers for every page chain
Page 2+  Data         Node | Edge | Label | String | Property | …
```

The footer holds the head pointer of each chain (first node page, first edge page, first label page, etc.). All integers are little-endian. Checksums are verified on every read.

Labels are **deduplicated** — inserting `"Person"` ten times stores the string once and returns the same `label_id` every time.

---

*More sections will be added as WAL, label indexes, and the query language land.*

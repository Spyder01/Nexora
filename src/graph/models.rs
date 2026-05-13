// TODO(phase3): label fields are heap-allocated Strings. Once the buffer pool is in place,
// pages are pinned in memory and label can become a &str borrowed directly from the cached
// page — zero allocation, zero copy. Until then, String is the correct ergonomic choice.

#[derive(Debug, Clone)]
pub struct Node {
    pub id:    u64,
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct Property {
    pub key:   String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct Edge {
    pub id:     u64,
    pub src:    u64,
    pub dst:    u64,
    pub label:  String,
    pub weight: f64,
}

use crate::graph::models::{Edge, Node};

pub enum Visit {
    Continue,
    Stop,
}

pub trait TraverseApi {
    type Error;

    fn for_each_outgoing<F>(&mut self, node_id: u64, f: F) -> Result<(), Self::Error>
    where
        F: FnMut(Edge) -> Visit;

    fn for_each_incoming<F>(&mut self, node_id: u64, f: F) -> Result<(), Self::Error>
    where
        F: FnMut(Edge) -> Visit;

    fn bfs<F>(&mut self, start: u64, max_depth: usize, f: F) -> Result<(), Self::Error>
    where
        F: FnMut(Node, usize) -> Visit;

    fn dfs<F>(&mut self, start: u64, max_depth: usize, f: F) -> Result<(), Self::Error>
    where
        F: FnMut(Node, usize) -> Visit;

    fn has_path(&mut self, src: u64, dst: u64) -> Result<bool, Self::Error>;

    fn shortest_path(&mut self, src: u64, dst: u64) -> Result<Option<Vec<u64>>, Self::Error>;

    fn for_each_with_label<F>(&mut self, label: &str, f: F) -> Result<(), Self::Error>
    where
        F: FnMut(Node) -> Visit;
}

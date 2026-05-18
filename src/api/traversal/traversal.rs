use std::collections::{HashMap, HashSet, VecDeque};

use crate::api::traversal::api::{TraverseApi, Visit};
use crate::graph::graphstore::error::NexoraGraphStoreError;
use crate::graph::graphstore::graphstore::GraphStore;
use crate::graph::models::{Edge, Node};
use crate::storage::page_store::PageStore;

pub struct Traversal<'a, S: PageStore> {
    store: &'a mut GraphStore<S>,
}

impl<'a, S: PageStore> Traversal<'a, S> {
    pub fn new(store: &'a mut GraphStore<S>) -> Self {
        Traversal { store }
    }
}

impl<'a, S: PageStore> TraverseApi for Traversal<'a, S> {
    type Error = NexoraGraphStoreError;

    fn for_each_outgoing<F>(&mut self, node_id: u64, mut f: F) -> Result<(), Self::Error>
    where
        F: FnMut(Edge) -> Visit,
    {
        let mut cursor = self.store.outgoing_cursor(node_id)?;
        loop {
            match self.store.next_outgoing(&mut cursor)? {
                None => break,
                Some(edge) => {
                    if let Visit::Stop = f(edge) {
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    fn for_each_incoming<F>(&mut self, node_id: u64, mut f: F) -> Result<(), Self::Error>
    where
        F: FnMut(Edge) -> Visit,
    {
        let mut cursor = self.store.incoming_cursor(node_id)?;
        loop {
            match self.store.next_incoming(&mut cursor)? {
                None => break,
                Some(edge) => {
                    if let Visit::Stop = f(edge) {
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    fn bfs<F>(&mut self, start: u64, max_depth: usize, mut f: F) -> Result<(), Self::Error>
    where
        F: FnMut(Node, usize) -> Visit,
    {
        let mut visited: HashSet<u64> = HashSet::new();
        let mut queue: VecDeque<(u64, usize)> = VecDeque::new();
        visited.insert(start);
        queue.push_back((start, 0));

        while let Some((node_id, depth)) = queue.pop_front() {
            let node = self.store.get_node(node_id)?;
            if let Visit::Stop = f(node, depth) {
                break;
            }
            if depth < max_depth {
                let mut cursor = self.store.outgoing_cursor(node_id)?;
                loop {
                    match self.store.next_outgoing(&mut cursor)? {
                        None => break,
                        Some(edge) => {
                            if visited.insert(edge.dst) {
                                queue.push_back((edge.dst, depth + 1));
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn dfs<F>(&mut self, start: u64, max_depth: usize, mut f: F) -> Result<(), Self::Error>
    where
        F: FnMut(Node, usize) -> Visit,
    {
        let mut visited: HashSet<u64> = HashSet::new();
        let mut stack: Vec<(u64, usize)> = vec![(start, 0)];

        while let Some((node_id, depth)) = stack.pop() {
            if !visited.insert(node_id) {
                continue;
            }
            let node = self.store.get_node(node_id)?;
            if let Visit::Stop = f(node, depth) {
                break;
            }
            if depth < max_depth {
                let mut cursor = self.store.outgoing_cursor(node_id)?;
                let mut neighbors: Vec<u64> = Vec::new();
                loop {
                    match self.store.next_outgoing(&mut cursor)? {
                        None => break,
                        Some(edge) => {
                            if !visited.contains(&edge.dst) {
                                neighbors.push(edge.dst);
                            }
                        }
                    }
                }
                // Reverse so the first neighbor in storage order ends up on top of the stack.
                for neighbor in neighbors.into_iter().rev() {
                    stack.push((neighbor, depth + 1));
                }
            }
        }
        Ok(())
    }

    fn has_path(&mut self, src: u64, dst: u64) -> Result<bool, Self::Error> {
        if src == dst {
            return Ok(true);
        }
        let mut visited: HashSet<u64> = HashSet::new();
        let mut queue: VecDeque<u64> = VecDeque::new();
        visited.insert(src);
        queue.push_back(src);

        while let Some(node_id) = queue.pop_front() {
            let mut cursor = self.store.outgoing_cursor(node_id)?;
            loop {
                match self.store.next_outgoing(&mut cursor)? {
                    None => break,
                    Some(edge) => {
                        if edge.dst == dst {
                            return Ok(true);
                        }
                        if visited.insert(edge.dst) {
                            queue.push_back(edge.dst);
                        }
                    }
                }
            }
        }
        Ok(false)
    }

    fn for_each_with_label<F>(&mut self, label: &str, mut f: F) -> Result<(), Self::Error>
    where
        F: FnMut(Node) -> Visit,
    {
        self.store.for_each_with_label(label, |node| matches!(f(node), Visit::Continue))
    }

    fn shortest_path(&mut self, src: u64, dst: u64) -> Result<Option<Vec<u64>>, Self::Error> {
        if src == dst {
            return Ok(Some(vec![src]));
        }
        let mut visited: HashSet<u64> = HashSet::new();
        let mut pred: HashMap<u64, u64> = HashMap::new();
        let mut queue: VecDeque<u64> = VecDeque::new();
        visited.insert(src);
        queue.push_back(src);

        'bfs: while let Some(node_id) = queue.pop_front() {
            let mut cursor = self.store.outgoing_cursor(node_id)?;
            loop {
                match self.store.next_outgoing(&mut cursor)? {
                    None => break,
                    Some(edge) => {
                        if visited.insert(edge.dst) {
                            pred.insert(edge.dst, node_id);
                            if edge.dst == dst {
                                break 'bfs;
                            }
                            queue.push_back(edge.dst);
                        }
                    }
                }
            }
        }

        if !pred.contains_key(&dst) {
            return Ok(None);
        }

        let mut path = vec![dst];
        let mut current = dst;
        while current != src {
            match pred.get(&current) {
                None => return Ok(None),
                Some(&parent) => {
                    path.push(parent);
                    current = parent;
                }
            }
        }
        path.reverse();
        Ok(Some(path))
    }
}

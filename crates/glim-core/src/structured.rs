//! Prepared structured-data nodes and their admitted storage.
use crate::resources::{ResourcePool, WorkingPressure, WorkingReservation};
use std::ops::Deref;

#[derive(Debug, PartialEq, Eq)]
pub struct Node {
    pub key: String,
    pub full_key: String,
    pub full_value: Option<String>,
    pub value: String,
    pub kind: &'static str,
    pub children: Vec<usize>,
}

#[derive(Default)]
pub struct Tree {
    nodes: Vec<Node>,
    memory: Option<WorkingReservation>,
}
impl Deref for Tree {
    type Target = [Node];
    fn deref(&self) -> &Self::Target {
        &self.nodes
    }
}

pub struct TreeBuilder {
    tree: Tree,
    used: u64,
}
impl TreeBuilder {
    pub fn new(pool: &ResourcePool) -> Self {
        Self {
            tree: Tree {
                nodes: Vec::new(),
                memory: Some(pool.reserve_working(0, 0).expect("zero-byte reservation")),
            },
            used: 0,
        }
    }

    /// Reserve string copies and both sides of a vector reallocation before
    /// constructing a node. A denied insertion leaves the tree unchanged.
    pub fn push(
        &mut self,
        key: &str,
        full_key: &str,
        value: &str,
        full_value: Option<&str>,
        kind: &'static str,
        parent: Option<usize>,
    ) -> Result<usize, WorkingPressure> {
        let old_nodes = self.tree.nodes.capacity();
        let old_children = parent.map_or(0, |p| self.tree.nodes[p].children.capacity());
        let grow_nodes = self.tree.nodes.len() == old_nodes;
        let grow_children =
            parent.is_some_and(|p| self.tree.nodes[p].children.len() == old_children);
        let mut new_nodes = if grow_nodes {
            (old_nodes * 2).max(4)
        } else {
            old_nodes
        };
        let mut new_children = if grow_children {
            (old_children * 2).max(4)
        } else {
            old_children
        };
        let strings =
            (key.len() + full_key.len() + value.len() + full_value.map_or(0, str::len)) as u64;
        let memory = self.tree.memory.as_mut().expect("builder admission");
        let old_bytes = self.used;
        let peak = |nodes: usize, children: usize| {
            old_bytes
                + strings
                + if grow_nodes {
                    (nodes * size_of::<Node>()) as u64
                } else {
                    0
                }
                + if grow_children {
                    (children * size_of::<usize>()) as u64
                } else {
                    0
                }
        };
        // Reuse an admitted construction window across nodes instead of taking
        // the shared pool lock for every string. No allocation exceeds it.
        let mut admit = |required: u64| {
            if required <= memory.bytes() {
                return Ok(());
            }
            memory
                .try_grow_to(required.saturating_mul(2))
                .or_else(|_| memory.try_grow_to(required))
        };
        if let Err(error) = admit(peak(new_nodes, new_children)) {
            if !grow_nodes && !grow_children {
                return Err(error);
            }
            // Spare capacity must not make a fitting next node fail.
            if grow_nodes {
                new_nodes = self.tree.nodes.len() + 1;
            }
            if grow_children {
                new_children = old_children + 1;
            }
            admit(peak(new_nodes, new_children))?;
        }
        if grow_nodes {
            let mut nodes = Vec::with_capacity(new_nodes);
            nodes.append(&mut self.tree.nodes);
            self.tree.nodes = nodes;
        }
        let index = self.tree.nodes.len();
        if let Some(parent) = parent {
            if grow_children {
                let children = &mut self.tree.nodes[parent].children;
                let mut replacement = Vec::with_capacity(new_children);
                replacement.append(children);
                *children = replacement;
            }
            self.tree.nodes[parent].children.push(index);
        }
        self.tree.nodes.push(Node {
            key: key.to_owned(),
            full_key: full_key.to_owned(),
            value: value.to_owned(),
            full_value: full_value.map(str::to_owned),
            kind,
            children: Vec::new(),
        });
        self.used = old_bytes
            + strings
            + ((self.tree.nodes.capacity() - old_nodes) * size_of::<Node>()) as u64
            + parent.map_or(0, |p| {
                ((self.tree.nodes[p].children.capacity() - old_children) * size_of::<usize>())
                    as u64
            });
        Ok(index)
    }

    pub fn len(&self) -> usize {
        self.tree.nodes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.tree.nodes.is_empty()
    }
    pub fn finish(mut self) -> Tree {
        self.tree
            .memory
            .as_mut()
            .expect("builder admission")
            .shrink_to(self.used);
        self.tree
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denied_child_insertion_preserves_parent_and_transfers_admission() {
        let pool = ResourcePool::with_budgets(0, 2048);
        let mut builder = TreeBuilder::new(&pool);
        builder
            .push("$", "$", "object", None, "object", None)
            .unwrap();
        let charged = pool.snapshot().working_bytes;
        let blocker = pool
            .reserve_working(2048 - charged, 2048 - charged)
            .unwrap();
        let label = "x".repeat(charged as usize);
        assert!(
            builder
                .push("a", "a", &label, None, "object", Some(0))
                .is_err()
        );
        drop(blocker);
        assert_eq!(pool.snapshot().working_bytes, charged);
        builder
            .push("a", "a", "1", Some("1"), "number", Some(0))
            .unwrap();
        let tree = builder.finish();
        assert_eq!(tree[0].children, [1]);
        assert_eq!(tree[1].full_value.as_deref(), Some("1"));
        assert!(pool.snapshot().working_bytes > 0);
        assert!(pool.snapshot().working_bytes <= charged);
        drop(tree);
        assert_eq!(pool.snapshot().working_bytes, 0);
    }
}

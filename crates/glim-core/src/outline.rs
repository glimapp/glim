//! Prepared outline values shared by parsers and independent presentations.
use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Outline {
    pub title: String,
    pub description: String,
    pub feed: Option<String>,
    pub website: Option<String>,
    pub children: Vec<usize>,
}
#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Document {
    pub path: PathBuf,
    pub fingerprint: String,
    pub title: String,
    pub nodes: Vec<Outline>,
    pub roots: Vec<usize>,
}

impl Document {
    pub fn heap_bytes(&self) -> u64 {
        (self.title.capacity() + self.fingerprint.capacity() + self.path.as_os_str().len()) as u64
            + (self.nodes.capacity() * std::mem::size_of::<Outline>()) as u64
            + (self.roots.capacity() * std::mem::size_of::<usize>()) as u64
            + self
                .nodes
                .iter()
                .map(|node| {
                    (node.title.capacity() + node.description.capacity()) as u64
                        + node.feed.as_ref().map_or(0, |v| v.capacity() as u64)
                        + node.website.as_ref().map_or(0, |v| v.capacity() as u64)
                        + (node.children.capacity() * std::mem::size_of::<usize>()) as u64
                })
                .sum::<u64>()
    }
}

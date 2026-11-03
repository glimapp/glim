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

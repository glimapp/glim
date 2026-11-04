//! Book catalog values and reading state, independent of storage and presentation.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Epub,
    FictionBook,
    Comic,
}
impl Format {
    pub fn label(self) -> &'static str {
        match self {
            Self::Epub => "EPUB",
            Self::FictionBook => "FB2",
            Self::Comic => "CBZ",
        }
    }
}
#[derive(Clone)]
pub struct Chapter {
    pub href: String,
    pub title: String,
    pub html: Option<String>,
}
#[derive(Clone)]
pub struct TocEntry {
    pub title: String,
    pub href: String,
    pub depth: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ReadingState {
    pub chapter: usize,
    pub fraction: f32,
    pub font_size: f32,
    pub line_height: f32,
    pub page_width: f32,
    pub serif: bool,
    pub paper: bool,
    pub bookmarks: Vec<Bookmark>,
}
impl Default for ReadingState {
    fn default() -> Self {
        Self {
            chapter: 0,
            fraction: 0.,
            font_size: 19.,
            line_height: 1.7,
            page_width: 720.,
            serif: true,
            paper: true,
            bookmarks: Vec::new(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bookmark {
    pub chapter: usize,
    pub fraction: f32,
    pub label: String,
}

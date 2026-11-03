//! Persistent workspace descriptions shared by normal startup and update recovery.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionTab {
    File { path: PathBuf, preview: bool },
    Playlist { path: PathBuf, shuffle: bool },
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSession {
    pub roots: Vec<PathBuf>,
    pub tabs: Vec<SessionTab>,
    pub active: Option<usize>,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub window_group: Option<String>,
    #[serde(default)]
    pub window_active: bool,
    #[serde(default)]
    pub show_home: bool,
}

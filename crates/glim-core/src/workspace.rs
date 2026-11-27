use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectoryEntry {
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_symlink: bool,
}

pub type ExplorerExpansion = BTreeMap<PathBuf, Vec<PathBuf>>;

/// A workspace's ordered folder roots, including repeated paths.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RecentWorkspace {
    pub id: String,
    pub folders: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl RecentWorkspace {
    pub fn title(&self) -> String {
        self.name.clone().unwrap_or_else(|| {
            self.folders
                .iter()
                .map(|path| {
                    path.file_name()
                        .unwrap_or(path.as_os_str())
                        .to_string_lossy()
                        .into_owned()
                })
                .collect::<Vec<_>>()
                .join(" · ")
        })
    }
}

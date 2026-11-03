use anyhow::Result;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

/// Bounded event coalescing. Unknown events and overflow retain a full-rescan marker.
#[derive(Clone, Default)]
pub struct Changes {
    paths: BTreeSet<PathBuf>,
    rescan: bool,
}
impl Changes {
    pub fn all() -> Self {
        Self {
            paths: BTreeSet::new(),
            rescan: true,
        }
    }
    pub fn is_empty(&self) -> bool {
        !self.rescan && self.paths.is_empty()
    }
    fn insert(&mut self, path: PathBuf) {
        if self.rescan {
            return;
        }
        self.paths.insert(path);
        if self.paths.len() > crate::limits::WATCH_PENDING_PATHS {
            *self = Self::all();
        }
    }
    pub fn merge(&mut self, other: Self) {
        if other.rescan {
            *self = Self::all();
        } else {
            for path in other.paths {
                self.insert(path);
            }
        }
    }
    pub fn affects(&self, path: &Path) -> bool {
        self.rescan
            || self
                .paths
                .iter()
                .any(|changed| path.starts_with(changed) || changed.starts_with(path))
    }
    /// Background-only: follow aliases without canonicalizing on the render thread.
    pub fn affects_file(&self, path: &Path) -> bool {
        if self.affects(path) {
            return true;
        }
        fn canonicalize_file(path: &Path) -> std::io::Result<PathBuf> {
            path.canonicalize().or_else(|_| {
                let parent = path
                    .parent()
                    .ok_or_else(|| std::io::Error::other("Missing parent"))?;
                Ok::<_, std::io::Error>(
                    parent
                        .canonicalize()?
                        .join(path.file_name().unwrap_or_default()),
                )
            })
        }
        let canonical = canonicalize_file(path);
        let Ok(canonical) = canonical else {
            return true;
        };
        if self.affects(&canonical) {
            return true;
        }
        self.paths.iter().any(|changed| {
            canonicalize_file(changed).is_ok_and(|changed| {
                canonical.starts_with(&changed) || changed.starts_with(&canonical)
            })
        })
    }
}

pub struct WorkspaceWatch {
    _watcher: RecommendedWatcher,
    pending: Arc<Mutex<Changes>>,
}
impl WorkspaceWatch {
    pub fn new(root: &Path) -> Result<Self> {
        let pending = Arc::new(Mutex::new(Changes::default()));
        let events = pending.clone();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let mut pending = events.lock().unwrap_or_else(|e| e.into_inner());
                match event {
                    Ok(event) if event.need_rescan() => *pending = Changes::all(),
                    Ok(event) if !matches!(event.kind, notify::EventKind::Access(_)) => {
                        if event.paths.is_empty() {
                            *pending = Changes::all();
                        } else {
                            for path in event.paths {
                                pending.insert(path);
                            }
                        }
                    }
                    Err(_) => *pending = Changes::all(),
                    _ => {}
                }
            })?;
        watcher.watch(root, RecursiveMode::Recursive)?;
        Ok(Self {
            _watcher: watcher,
            pending,
        })
    }
    pub fn add_path(&mut self, path: &Path) -> Result<()> {
        self._watcher.watch(path, RecursiveMode::Recursive)?;
        Ok(())
    }
    pub fn take_changes(&self) -> Changes {
        std::mem::take(&mut *self.pending.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coalescing_retains_directory_invalidation_and_bounds_event_storms() {
        let mut changes = Changes::default();
        changes.insert("/workspace/docs".into());
        changes.insert("/workspace/docs".into());
        assert_eq!(changes.paths.len(), 1);
        assert!(changes.affects(Path::new("/workspace/docs/readme.md")));
        assert!(!changes.affects(Path::new("/workspace/docs-other/readme.md")));
        for i in 0..=crate::limits::WATCH_PENDING_PATHS {
            changes.insert(format!("/workspace/generated/{i}").into());
        }
        assert!(changes.rescan);
        assert!(changes.paths.is_empty());
        changes.merge(Changes::default());
        assert!(changes.affects(Path::new("/elsewhere/file")));
    }

    #[test]
    fn alias_events_reach_the_open_document() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let real = dir.path().join("real");
        let alias = dir.path().join("alias");
        std::fs::create_dir(&real)?;
        std::os::unix::fs::symlink(&real, &alias)?;
        let file = real.join("doc.md");
        std::fs::write(&file, "contents")?;
        let mut changes = Changes::default();
        changes.insert(file.clone());
        assert!(changes.affects_file(&alias.join("doc.md")));
        assert!(!changes.affects_file(&real.join("other.md")));
        std::fs::remove_file(&file)?;
        assert!(changes.affects_file(&alias.join("doc.md")));
        Ok(())
    }
    #[test]
    fn detects_atomic_file_replacement() -> Result<()> {
        let temp = tempfile::tempdir()?;
        std::fs::write(temp.path().join("doc.md"), "before")?;
        let watch = WorkspaceWatch::new(temp.path())?;
        std::fs::write(temp.path().join("next.md"), "after")?;
        std::fs::rename(temp.path().join("next.md"), temp.path().join("doc.md"))?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            if !watch.take_changes().is_empty() {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        anyhow::bail!("Watcher did not report file replacement")
    }
}

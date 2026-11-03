//! Identity and saved state survive view replacement and window movement.
use glim_core::ContentSnapshot;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DocumentVersion {
    id: u64,
    revision: u64,
}

pub(crate) struct DocumentSession {
    pub path: PathBuf,
    pub edit: Option<super::edit_session::EditSession>,
    pub snapshot: ContentSnapshot,
    pub draft_directory: Option<PathBuf>,
    pub recovery_key: Option<String>,
    version: DocumentVersion,
}
impl DocumentSession {
    pub fn new(path: PathBuf, snapshot: ContentSnapshot) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self {
            path,
            edit: None,
            snapshot,
            draft_directory: None,
            recovery_key: None,
            version: DocumentVersion {
                id: NEXT.fetch_add(1, Ordering::Relaxed),
                revision: 0,
            },
        }
    }
    pub fn with_edit(mut self, edit: Option<super::edit_session::EditSession>) -> Self {
        self.edit = edit;
        self
    }
    pub fn version(&self) -> DocumentVersion {
        self.version
    }
    pub fn mark_saved(&mut self, snapshot: ContentSnapshot) {
        self.snapshot = snapshot;
        self.version.revision += 1;
    }
    pub fn inherit_identity(&mut self, previous: &Self) {
        self.version = previous.version;
        self.version.revision += 1;
        self.recovery_key.clone_from(&previous.recovery_key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reload_preserves_identity_but_reopen_does_not() {
        let mut original =
            DocumentSession::new("note.txt".into(), ContentSnapshot::Text("old".into()));
        original.recovery_key = Some("recovery".into());
        let requested = original.version();
        original.mark_saved(ContentSnapshot::Text("saved".into()));
        assert_ne!(requested, original.version());
        let mut replacement = DocumentSession::new(
            original.path.clone(),
            ContentSnapshot::Text("external".into()),
        );
        replacement.inherit_identity(&original);
        assert_eq!(replacement.version.id, requested.id);
        assert_ne!(replacement.version(), original.version());
        assert_eq!(replacement.recovery_key, original.recovery_key);
        let reopened = DocumentSession::new(original.path.clone(), ContentSnapshot::Untracked);
        assert_ne!(reopened.version.id, requested.id);
    }
}

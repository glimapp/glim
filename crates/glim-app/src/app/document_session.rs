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
    pub remote: Option<glim_services::remote::Session>,
    pub remote_version: Option<glim_services::remote::RemoteVersion>,
    pub path: PathBuf,
    pub edit: Option<super::edit_session::EditSession>,
    pub snapshot: ContentSnapshot,
    pub source_missing: bool,
    pub trashed: bool,
    pub retained_file: Option<std::sync::Arc<glim_services::files::RetainedFile>>,
    pub draft_directory: Option<PathBuf>,
    pub recovery_key: Option<String>,
    version: DocumentVersion,
}
impl DocumentSession {
    pub fn new(path: PathBuf, snapshot: ContentSnapshot) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self {
            remote: None,
            remote_version: None,
            path,
            edit: None,
            snapshot,
            source_missing: false,
            trashed: false,
            retained_file: None,
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
        self.source_missing = false;
        self.trashed = false;
        self.retained_file = None;
        self.version.revision += 1;
    }
    pub fn mark_trashed(
        &mut self,
        file: Option<std::sync::Arc<glim_services::files::RetainedFile>>,
    ) {
        self.trashed = true;
        self.source_missing = true;
        self.retained_file = file;
        self.recovery_key = None;
        self.version.revision += 1;
    }
    pub fn restore_on_launch(&self) -> bool {
        self.draft_directory.is_none() && !self.trashed
    }
    pub fn autosave_enabled(&self) -> bool {
        self.restore_on_launch()
    }
    pub fn inherit_identity(&mut self, previous: &Self) {
        self.remote = previous.remote.clone();
        self.version = previous.version;
        self.version.revision += 1;
        self.recovery_key.clone_from(&previous.recovery_key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trashed_buffers_are_session_only_until_explicitly_saved() {
        let mut document =
            DocumentSession::new("note.md".into(), ContentSnapshot::Text("# Kept".into()));
        let before = document.version();
        document.recovery_key = Some("old-checkpoint".into());
        document.mark_trashed(None);
        assert!(!document.autosave_enabled());
        assert!(!document.restore_on_launch());
        assert!(document.recovery_key.is_none());
        assert_eq!(document.snapshot.saved_text().as_deref(), Some("# Kept"));
        assert_ne!(document.version(), before);
        document.mark_saved(ContentSnapshot::Text("# Saved copy".into()));
        assert!(document.autosave_enabled());
        assert!(document.restore_on_launch());
        assert!(!document.trashed);
    }

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

use super::{Content, read_content_cancellable};
use crate::{paged::PageRefresh, sqlite, work::Cancellation};
use anyhow::Result;
use std::{path::PathBuf, time::SystemTime};

pub enum RefreshRequest {
    Reload(PathBuf),
    Missing(PathBuf),
    Page(PageRefresh),
    Sqlite {
        path: PathBuf,
        stamp: Option<sqlite::SourceStamp>,
    },
    NativeImage {
        path: PathBuf,
        stamp: (u64, Option<SystemTime>),
    },
}

impl RefreshRequest {
    pub fn read(
        self,
        changes: &crate::watch::Changes,
        cancel: &Cancellation,
    ) -> Result<Option<Content>> {
        cancel.check()?;
        let affected = match &self {
            // SQLite has a cheap family stamp covering main/WAL/journal files.
            Self::Sqlite { .. } => true,
            Self::Page(page) => changes.affects_file(&page.source.original),
            Self::Reload(path) | Self::Missing(path) | Self::NativeImage { path, .. } => {
                changes.affects_file(path)
            }
        };
        if !affected {
            return Ok(None);
        }
        cancel.check()?;
        let path = match self {
            Self::Page(request) => return request.read(cancel).map(|page| page.map(Content::Page)),
            Self::Sqlite { path, stamp } => {
                if stamp.is_some_and(|stamp| {
                    sqlite::source_stamp(&path).is_ok_and(|actual| actual == stamp)
                }) {
                    return Ok(None);
                }
                path
            }
            Self::NativeImage { path, stamp } => {
                if std::fs::metadata(&path).is_ok_and(|m| (m.len(), m.modified().ok()) == stamp) {
                    return Ok(None);
                }
                path
            }
            Self::Reload(path) => path,
            Self::Missing(path) => {
                if std::fs::metadata(&path)
                    .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
                {
                    return Ok(None);
                }
                path
            }
        };
        read_content_cancellable(&path, cancel).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_source_waits_for_recreation_and_explicit_reload_reports_failure() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("note.txt");
        let changes = crate::watch::Changes::all();
        let cancel = Cancellation::default();
        assert!(
            RefreshRequest::Missing(path.clone())
                .read(&changes, &cancel)?
                .is_none()
        );
        let failure = RefreshRequest::Reload(path.clone())
            .read(&changes, &cancel)
            .err()
            .unwrap();
        assert_eq!(
            super::super::ReadFailure::from(&failure).kind,
            super::super::ReadFailureKind::NotFound
        );
        std::fs::write(&path, "restored")?;
        assert!(
            RefreshRequest::Missing(path)
                .read(&changes, &cancel)?
                .is_some()
        );
        Ok(())
    }
}

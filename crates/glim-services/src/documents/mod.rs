//! Format selection and preparation. Invoke on a background executor.
use crate::limits as reader_limits;
use glim_core::Document;
use std::path::{Path, PathBuf};
pub mod presentation;
pub mod refresh;
mod registry;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadFailureKind {
    NotFound,
    PermissionDenied,
    Other,
}

#[derive(Clone, Debug)]
pub struct ReadFailure {
    pub kind: ReadFailureKind,
    pub detail: String,
}

impl From<&anyhow::Error> for ReadFailure {
    fn from(error: &anyhow::Error) -> Self {
        let kind = match error
            .downcast_ref::<std::io::Error>()
            .map(std::io::Error::kind)
        {
            Some(std::io::ErrorKind::NotFound) => ReadFailureKind::NotFound,
            Some(std::io::ErrorKind::PermissionDenied) => ReadFailureKind::PermissionDenied,
            _ => ReadFailureKind::Other,
        };
        Self {
            kind,
            detail: format!("{error:#}"),
        }
    }
}

impl From<String> for ReadFailure {
    fn from(detail: String) -> Self {
        Self {
            kind: ReadFailureKind::Other,
            detail,
        }
    }
}

pub enum Content {
    OpenApi(Box<Content>),
    Archive(crate::archive::Archive),
    File(Document),
    Page(crate::paged::TextPage),
    Image(crate::media::ImageDocument),
    XMind(crate::xmind::MindMap),
    Opml(crate::opml::Document),
    Book(crate::books::Book),
    Native(PathBuf, bool, bool),
    Sqlite(PathBuf),
    Unavailable(PathBuf, ReadFailure),
}
impl Content {
    pub fn path(&self) -> &Path {
        match self {
            Self::OpenApi(source) => source.path(),
            Self::Archive(d) => &d.path,
            Self::File(d) => &d.path,
            Self::Page(d) => &d.path,
            Self::Image(d) => &d.path,
            Self::XMind(d) => &d.path,
            Self::Opml(d) => &d.path,
            Self::Book(d) => &d.path,
            Self::Native(p, ..) | Self::Sqlite(p) | Self::Unavailable(p, _) => p,
        }
    }
    pub fn matches_snapshot(&self, snapshot: &glim_core::ContentSnapshot) -> bool {
        use glim_core::ContentSnapshot;
        match (self, snapshot) {
            (Self::OpenApi(source), snapshot) => source.matches_snapshot(snapshot),
            (Self::File(d), ContentSnapshot::Text(text)) => d.text == text.as_ref(),
            (Self::Image(d), ContentSnapshot::Fingerprint(value)) => &d.fingerprint == value,
            (Self::Opml(d), ContentSnapshot::Fingerprint(value)) => &d.fingerprint == value,
            (Self::XMind(d), ContentSnapshot::Fingerprint(value)) => &d.fingerprint == value,
            (Self::Book(d), ContentSnapshot::Fingerprint(value)) => &d.fingerprint == value,
            (Self::Page(d), ContentSnapshot::Fingerprint(value)) => &d.fingerprint() == value,
            _ => false,
        }
    }
}

pub fn read_session_tab(
    tab: &glim_core::session::SessionTab,
    cancel: &crate::work::Cancellation,
) -> anyhow::Result<Content> {
    cancel.check()?;
    match tab {
        glim_core::session::SessionTab::File { path, .. } => read_content_cancellable(path, cancel),
        glim_core::session::SessionTab::Playlist { path, shuffle } => {
            Ok(Content::Native(path.clone(), true, *shuffle))
        }
    }
}

pub fn read_content(path: &std::path::Path) -> anyhow::Result<Content> {
    read_content_cancellable(path, &crate::work::Cancellation::default())
}
pub fn read_content_cancellable(
    path: &std::path::Path,
    cancel: &crate::work::Cancellation,
) -> anyhow::Result<Content> {
    let _scope = crate::diagnostics::FileScope::enter(path, "read");
    log::info!("read begin file={}", path.display());
    let started = std::time::Instant::now();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        read_content_inner(path, cancel)
    }))
    .unwrap_or_else(|_| Err(anyhow::anyhow!("File reader panicked: {}", path.display())));
    cancel.check()?;
    match &result {
        Ok(_) => log::info!(
            "read complete file={} elapsed_ms={}",
            path.display(),
            started.elapsed().as_millis()
        ),
        Err(error) => log::error!(
            "read failed file={} elapsed_ms={} error={error:#}",
            path.display(),
            started.elapsed().as_millis()
        ),
    }
    result
}
fn read_content_inner(path: &Path, cancel: &crate::work::Cancellation) -> anyhow::Result<Content> {
    for reader in registry::READERS {
        cancel.check()?;
        if (reader.accepts)(path) {
            log::debug!("reader selected id={} file={}", reader.id, path.display());
            let content = (reader.open)(path, cancel)?;
            cancel.check()?;
            return Ok(content);
        }
    }
    let content = text_or_bytes(path, cancel)?;
    if crate::openapi::supports(path) {
        let detected = crate::openapi::is_spec(path, cancel).unwrap_or(false);
        cancel.check()?;
        if detected {
            return Ok(Content::OpenApi(Box::new(content)));
        }
    }
    Ok(content)
}

fn source_fallback(
    path: &Path,
    cancel: &crate::work::Cancellation,
    error: anyhow::Error,
) -> anyhow::Result<Content> {
    cancel.check()?;
    log::warn!(
        "Structured preview unavailable file={}: {error:#}",
        path.display()
    );
    crate::paged::read_page_cancellable(path, 0, false, cancel)
        .or_else(|_| {
            cancel.check()?;
            crate::paged::read_binary_page_cancellable(path, 0, cancel)
        })
        .map(Content::Page)
}

fn outline(path: &Path, cancel: &crate::work::Cancellation) -> anyhow::Result<Content> {
    match crate::opml::read(path, cancel) {
        Ok(document) => Ok(Content::Opml(document)),
        Err(error) => source_fallback(path, cancel, error),
    }
}

fn drawing(path: &Path, cancel: &crate::work::Cancellation) -> anyhow::Result<Content> {
    match crate::drawio::read(path, cancel) {
        Ok(document) => Ok(Content::Image(document)),
        Err(error) => source_fallback(path, cancel, error),
    }
}

fn mind_map(path: &Path, cancel: &crate::work::Cancellation) -> anyhow::Result<Content> {
    match crate::xmind::read_cancellable(path, cancel) {
        Ok(map) => Ok(Content::XMind(map)),
        Err(error) => {
            cancel.check()?;
            match crate::xmind::read_source(path, cancel) {
                Ok(page) => {
                    use crate::telemetry::{file_event, flag, text};
                    file_event(
                        "reader_fallback",
                        path,
                        [
                            ("reader_kind", text("xmind")),
                            ("reason", text(crate::xmind::failure_stage(&error))),
                            (
                                "preview_budget",
                                flag(error.is::<reader_limits::PreviewBudgetExceeded>()),
                            ),
                        ],
                    );
                    Ok(Content::Page(page))
                }
                Err(_) => {
                    cancel.check()?;
                    Err(error)
                }
            }
        }
    }
}

fn vector(path: &Path, cancel: &crate::work::Cancellation) -> anyhow::Result<Content> {
    match crate::vector_formats::read_preview(path) {
        Ok(image) => Ok(Content::Image(image)),
        Err(error) if error.is::<reader_limits::PreviewBudgetExceeded>() => {
            source_fallback(path, cancel, error)
        }
        Err(error) => Err(error),
    }
}

fn text_or_bytes(path: &Path, cancel: &crate::work::Cancellation) -> anyhow::Result<Content> {
    let result = (|| {
        let first = crate::paged::read_page_cancellable(path, 0, false, cancel)?;
        if first.total > crate::files::MAX_DOCUMENT_BYTES
            || first
                .text
                .lines()
                .any(|line| line.len() > reader_limits::TEXT_LINE_BYTES)
        {
            Ok(Content::Page(first))
        } else {
            crate::files::read_document_cancellable(path, cancel)
                .map(|d| {
                    if d.text
                        .lines()
                        .any(|line| line.len() > reader_limits::TEXT_LINE_BYTES)
                        || d.text
                            .bytes()
                            .filter(|b| *b == b'\n')
                            .take(reader_limits::TEXT_EDITABLE_NEWLINES + 1)
                            .count()
                            > reader_limits::TEXT_EDITABLE_NEWLINES
                    {
                        Content::Page(first)
                    } else {
                        Content::File(d)
                    }
                })
                .or_else(|error| {
                    if error.is::<reader_limits::PreviewBudgetExceeded>() {
                        crate::paged::read_page_cancellable(path, 0, false, cancel)
                            .map(Content::Page)
                    } else {
                        Err(error)
                    }
                })
        }
    })();
    // Unknown binary files use the same bounded, read-only byte pages as
    // hidden files. A filename prefix does not determine whether bytes are viewable.
    if result.is_err() {
        cancel.check()?;
        crate::paged::read_binary_page_cancellable(path, 0, cancel).map(Content::Page)
    } else {
        result
    }
}

#[cfg(test)]
mod format_tests {
    use super::{Content, read_content};

    #[test]
    fn saved_text_cannot_collide_with_preview_identity() {
        for text in ["native-preview", "sqlite-preview", "fingerprint"] {
            let content = Content::File(glim_core::Document {
                path: "note.txt".into(),
                kind: glim_core::DocumentKind::Text,
                text: text.into(),
            });
            assert!(content.matches_snapshot(&glim_core::ContentSnapshot::Text(text.into())));
            assert!(
                !content.matches_snapshot(&glim_core::ContentSnapshot::Fingerprint(text.into()))
            );
            assert!(!content.matches_snapshot(&glim_core::ContentSnapshot::Untracked));
        }
    }

    #[test]
    fn routing_probes_ambiguous_files_before_generic_fallback() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let key = dir.path().join("private.key");
        std::fs::write(&key, "-----BEGIN PRIVATE KEY-----\nexample\n")?;
        assert!(matches!(read_content(&key)?, Content::File(_)));
        let database = dir.path().join("database.data");
        let connection = rusqlite::Connection::open(&database)?;
        connection.execute_batch("CREATE TABLE example (value TEXT);")?;
        drop(connection);
        assert!(matches!(read_content(&database)?, Content::Sqlite(_)));
        let invalid = dir.path().join("invalid.opml");
        std::fs::write(&invalid, "incomplete outline")?;
        assert!(matches!(read_content(&invalid)?, Content::Page(_)));
        let (cancel, owner) = crate::work::Cancellation::task();
        drop(owner);
        let result = super::read_content_cancellable(&invalid, &cancel);
        assert!(result.is_err_and(|error| error.is::<crate::work::Cancelled>()));
        Ok(())
    }
    #[test]
    fn dotfiles_are_routed_by_content_not_missing_extension() {
        let dir = tempfile::tempdir().unwrap();
        for name in [".env", ".npmrc", ".gitignore", ".config.local"] {
            let path = dir.path().join(name);
            std::fs::write(&path, "EXAMPLE=value\n").unwrap();
            assert!(matches!(read_content(&path).unwrap(), Content::File(_)));
        }
        let binary = dir.path().join(".DS_Store");
        std::fs::write(&binary, [0, 1, 2, 255]).unwrap();
        assert!(matches!(read_content(&binary).unwrap(), Content::Page(_)));
    }
}

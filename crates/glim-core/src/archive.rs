//! Prepared archive entries and directory queries.
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

#[derive(serde::Deserialize, serde::Serialize)]
pub struct Entry {
    pub name: String,
    pub size: u64,
    pub directory: bool,
}
pub struct Archive {
    pub path: PathBuf,
    pub format: String,
    entries: Vec<Entry>,
    total_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectoryEntry {
    pub name: String,
    pub directory: bool,
    pub bytes: u64,
}

impl Archive {
    pub fn new(path: &Path, format: String, mut entries: Vec<Entry>) -> Self {
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        let total_bytes = entries
            .iter()
            .fold(0u64, |total, entry| total.saturating_add(entry.size));
        Self {
            path: path.to_path_buf(),
            format,
            entries,
            total_bytes,
        }
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    /// List immediate children of an archive path prefix, including sizes of
    /// directory descendants. The root is empty; other prefixes end in `/`.
    pub fn directory_entries<E>(
        &self,
        prefix: &str,
        mut checkpoint: impl FnMut() -> Result<(), E>,
    ) -> Result<Vec<DirectoryEntry>, E> {
        checkpoint()?;
        let start = self
            .entries
            .partition_point(|entry| entry.name.as_str() < prefix);
        let mut children = HashMap::new();
        for entry in &self.entries[start..] {
            checkpoint()?;
            let Some(tail) = entry.name.strip_prefix(prefix) else {
                break;
            };
            if tail.is_empty() || entry.name.starts_with("__MACOSX/") {
                continue;
            }
            let (name, directory) = tail
                .split_once('/')
                .map_or((tail, entry.directory), |(name, _)| (name, true));
            let row = children.entry(name).or_insert((directory, 0u64));
            row.1 = row.1.saturating_add(entry.size);
        }
        let mut rows = Vec::with_capacity(children.len());
        for (name, (directory, bytes)) in children {
            checkpoint()?;
            rows.push(DirectoryEntry {
                name: name.to_owned(),
                directory,
                bytes,
            });
        }
        rows.sort_unstable_by(|a, b| b.bytes.cmp(&a.bytes).then(a.name.cmp(&b.name)));
        checkpoint()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_queries_stop_without_publishing_partial_rows() {
        let entries = ["first/a", "first/b"]
            .into_iter()
            .map(|name| Entry {
                name: name.into(),
                size: 1,
                directory: false,
            })
            .collect();
        let archive = Archive::new(Path::new("test.zip"), "ZIP".into(), entries);
        let mut checks = 0;
        let result = archive.directory_entries("first/", || {
            checks += 1;
            if checks == 3 {
                Err("cancelled")
            } else {
                Ok(())
            }
        });
        assert_eq!(result, Err("cancelled"));
        assert_eq!(checks, 3);
    }

    #[test]
    fn directory_listing_preserves_sizes_visibility_and_order() {
        let entries = [
            ("docs/nested/a", false, 7),
            ("docs-old/first", false, 12),
            ("docs/", true, 0),
            ("docs/β", false, 7),
            ("__MACOSX/docs/ignored", false, 900),
            ("docs/nested/", true, 0),
            ("docs/a", false, 7),
            ("docs/nested/b", false, 6),
            ("docs0", false, 28),
            ("notes", false, 2),
            ("notes-old", false, 5),
            ("notes/child", false, 3),
        ]
        .into_iter()
        .map(|(name, directory, size)| Entry {
            name: name.to_owned(),
            size,
            directory,
        })
        .collect();
        let archive = Archive::new(Path::new("example.zip"), "ZIP".into(), entries);
        assert_eq!(archive.total_bytes(), 977);
        let rows = |prefix| {
            archive
                .directory_entries(prefix, || Ok::<_, std::convert::Infallible>(()))
                .unwrap()
                .into_iter()
                .map(|row| (row.name, row.directory, row.bytes))
                .collect::<Vec<_>>()
        };
        let expected = |entries: &[(&str, bool, u64)]| {
            entries
                .iter()
                .map(|&(name, dir, size)| (name.to_owned(), dir, size))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            rows(""),
            expected(&[
                ("docs0", false, 28),
                ("docs", true, 27),
                ("docs-old", true, 12),
                ("notes", false, 5),
                ("notes-old", false, 5),
            ])
        );
        assert_eq!(
            rows("docs/"),
            expected(&[("nested", true, 13), ("a", false, 7), ("β", false, 7),])
        );
        assert_eq!(
            rows("docs/nested/"),
            expected(&[("a", false, 7), ("b", false, 6)])
        );
        assert!(rows("__MACOSX/").is_empty());
        assert!(rows("missing/").is_empty());
    }
}

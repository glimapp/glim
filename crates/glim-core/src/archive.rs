//! Prepared archive entries and directory queries.
use crate::resources::{ResourcePool, WorkingPressure, WorkingReservation};
use std::{
    collections::{HashMap, hash_map::Entry as MapEntry},
    ops::Deref,
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

/// Directory rows retain their output allocation through worker/view handoff.
pub struct DirectoryRows {
    rows: Vec<DirectoryEntry>,
    _memory: WorkingReservation,
}

impl Deref for DirectoryRows {
    type Target = [DirectoryEntry];
    fn deref(&self) -> &Self::Target {
        &self.rows
    }
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
    pub fn directory_entries<E: From<WorkingPressure>>(
        &self,
        prefix: &str,
        pool: &ResourcePool,
        mut checkpoint: impl FnMut() -> Result<(), E>,
    ) -> Result<DirectoryRows, E> {
        checkpoint()?;
        let start = self
            .entries
            .partition_point(|entry| entry.name.as_str() < prefix);
        let mut children = HashMap::<&str, (bool, u64)>::new();
        let mut name_bytes = 0u64;
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
            match children.entry(name) {
                MapEntry::Occupied(mut row) => {
                    let bytes = &mut row.get_mut().1;
                    *bytes = bytes.saturating_add(entry.size);
                }
                MapEntry::Vacant(row) => {
                    name_bytes += name.len() as u64;
                    row.insert((directory, entry.size));
                }
            }
        }
        let bytes = name_bytes + (children.len() * size_of::<DirectoryEntry>()) as u64;
        let memory = pool.reserve_working(bytes, bytes)?;
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
        Ok(DirectoryRows {
            rows,
            _memory: memory,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    enum QueryError {
        Cancelled,
        Pressure,
    }
    impl From<WorkingPressure> for QueryError {
        fn from(_: WorkingPressure) -> Self {
            Self::Pressure
        }
    }

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
        let result = archive.directory_entries("first/", &ResourcePool::new(0), || {
            checks += 1;
            if checks == 3 {
                Err(QueryError::Cancelled)
            } else {
                Ok(())
            }
        });
        assert!(matches!(result, Err(QueryError::Cancelled)));
        assert_eq!(checks, 3);
    }

    #[test]
    fn directory_rows_hold_exact_output_admission_through_handoff() {
        let entries = ["folder/中文", "folder/second", "one"]
            .into_iter()
            .map(|name| Entry {
                name: name.into(),
                size: 1,
                directory: false,
            })
            .collect();
        let archive = Archive::new(Path::new("test.zip"), "ZIP".into(), entries);
        let bytes = (2 * size_of::<DirectoryEntry>() + "folderone".len()) as u64;
        let pool = ResourcePool::with_budgets(0, bytes - 1);
        let query = archive.directory_entries("", &pool, || Ok::<_, WorkingPressure>(()));
        assert!(query.is_err());
        assert_eq!(pool.snapshot().working_bytes, 0);

        let pool = ResourcePool::with_budgets(0, bytes);
        let rows = archive
            .directory_entries("", &pool, || Ok::<_, WorkingPressure>(()))
            .unwrap();
        assert_eq!((rows[0].name.as_str(), rows[0].bytes), ("folder", 2));
        assert_eq!(pool.snapshot().working_bytes, bytes);
        drop(rows);
        assert_eq!(pool.snapshot().working_bytes, 0);
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
        let pool = ResourcePool::new(0);
        let rows = |prefix| {
            archive
                .directory_entries(prefix, &pool, || Ok::<_, WorkingPressure>(()))
                .unwrap()
                .iter()
                .map(|row| (row.name.clone(), row.directory, row.bytes))
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

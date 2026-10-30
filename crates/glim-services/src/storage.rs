//! Application-owned SQLite state. All entry points are blocking services.
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};
use std::{
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

mod migration;
pub(crate) mod sessions;

pub fn initialize() -> Result<()> {
    let directory = crate::preferences::directory()?;
    let mut connection = open_at(&directory)?;
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    migration::archive_sources(&tx, &directory)?;
    tx.commit()?;
    Ok(())
}
pub(crate) fn open() -> Result<Connection> {
    open_at(&crate::preferences::directory()?)
}
pub(crate) fn open_at(directory: &Path) -> Result<Connection> {
    std::fs::create_dir_all(directory)?;
    let path = directory.join("glim.sqlite3");
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(&path)?;
    let mut db = Connection::open(&path).context("Cannot open Glim application database")?;
    db.busy_timeout(std::time::Duration::from_secs(10))?;
    db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;")?;
    let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    ensure!(
        version <= 1,
        "Glim database was created by a newer application version"
    );
    if version == 0 {
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let version: i64 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 0 {
            tx.execute_batch(include_str!("storage/schema.sql"))?;
            migration::import(&tx, directory)
                .context("Cannot migrate existing Glim data; original files have been preserved")?;
            tx.execute_batch("PRAGMA user_version=1")?;
        }
        tx.commit()?;
    }
    let mode: String = db.query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
    if !mode.eq_ignore_ascii_case("wal") {
        db.pragma_update(None, "journal_mode", "WAL")?;
    }
    Ok(db)
}
pub(crate) fn setting(db: &Connection, key: &str) -> Result<Option<String>> {
    Ok(db
        .query_row("SELECT value FROM settings WHERE key=?1", [key], |r| {
            r.get(0)
        })
        .optional()?)
}
pub(crate) fn set_setting(db: &Connection, key: &str, value: &str) -> Result<()> {
    db.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", (key,value))?;
    Ok(())
}
pub(crate) fn record(db: &Connection, kind: &str, id: &str) -> Result<Option<String>> {
    Ok(db
        .query_row(
            "SELECT payload FROM records WHERE kind=?1 AND id=?2",
            (kind, id),
            |r| r.get(0),
        )
        .optional()?)
}
pub(crate) fn set_record(db: &Connection, kind: &str, id: &str, payload: &str) -> Result<()> {
    db.execute("INSERT INTO records(kind,id,payload) VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO UPDATE SET payload=excluded.payload", (kind,id,payload))?;
    Ok(())
}
pub(crate) fn path_text(path: &Path) -> Result<&str> {
    path.to_str()
        .context("Application state contains a non-UTF-8 path")
}
pub(crate) fn paths(db: &Connection, scope: &str, owner: &str) -> Result<Vec<PathBuf>> {
    Ok(db
        .prepare("SELECT path FROM paths WHERE scope=?1 AND owner=?2 ORDER BY position")?
        .query_map((scope, owner), |r| {
            Ok(PathBuf::from(r.get::<_, String>(0)?))
        })?
        .collect::<rusqlite::Result<_>>()?)
}
pub(crate) fn save_paths(
    db: &Connection,
    scope: &str,
    owner: &str,
    paths: &[PathBuf],
) -> Result<()> {
    db.execute(
        "DELETE FROM paths WHERE scope=?1 AND owner=?2",
        (scope, owner),
    )?;
    let mut insert =
        db.prepare("INSERT INTO paths(scope,owner,position,path) VALUES(?1,?2,?3,?4)")?;
    for (index, path) in paths.iter().enumerate() {
        insert.execute((scope, owner, index as i64, path_text(path)?))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use glim_core::update::{SessionTab, WorkspaceSession};
    use std::collections::BTreeMap;

    #[test]
    fn legacy_migration_preserves_window_order_duplicate_roots_and_is_not_reimported() -> Result<()>
    {
        let directory = tempfile::tempdir()?;
        let first = WorkspaceSession {
            roots: vec!["/项目".into(), "/项目".into(), "/项目/子目录".into()],
            tabs: vec![
                SessionTab::File {
                    path: "/项目/a.md".into(),
                    preview: true,
                },
                SessionTab::Playlist {
                    path: "/音乐".into(),
                    shuffle: true,
                },
            ],
            active: Some(1),
            workspace_id: Some("one".into()),
            name: Some("研究".into()),
            window_group: Some("window".into()),
        };
        let windows = BTreeMap::from([
            (
                "window".to_owned(),
                vec![first.clone(), WorkspaceSession::default()],
            ),
            ("empty".to_owned(), vec![]),
        ]);
        std::fs::write(
            directory.path().join("window-sessions.json"),
            serde_json::to_vec(&windows)?,
        )?;
        std::fs::write(directory.path().join("word-wrap"), "true\n")?;
        let db = open_at(directory.path())?;
        assert_eq!(sessions::load(&db, "window")?, windows);
        assert_eq!(setting(&db, "word-wrap")?.as_deref(), Some("true"));
        set_setting(&db, "word-wrap", "false")?;
        drop(db);
        let db = open_at(directory.path())?;
        assert_eq!(setting(&db, "word-wrap")?.as_deref(), Some("false"));
        assert!(directory.path().join("window-sessions.json").is_file());
        migration::archive_sources(&db, directory.path())?;
        assert!(
            directory
                .path()
                .join("LegacyMigration/window-sessions.json")
                .is_file()
        );
        assert!(db.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))? == "ok");
        Ok(())
    }
    #[test]
    fn malformed_legacy_state_rolls_back_and_can_be_retried_without_losing_sources() -> Result<()> {
        let directory = tempfile::tempdir()?;
        std::fs::write(directory.path().join("word-wrap"), "true")?;
        let source = directory.path().join("workspace-sessions.json");
        std::fs::write(&source, b"broken json")?;
        assert!(open_at(directory.path()).is_err());
        let db = Connection::open(directory.path().join("glim.sqlite3"))?;
        assert_eq!(
            db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
            0
        );
        assert_eq!(std::fs::read(&source)?, b"broken json");
        drop(db);
        std::fs::write(source, b"{}")?;
        assert_eq!(
            setting(&open_at(directory.path())?, "word-wrap")?.as_deref(),
            Some("true")
        );
        Ok(())
    }
    #[test]
    fn invalid_session_replacement_keeps_the_previous_committed_snapshot() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut db = open_at(directory.path())?;
        let original = BTreeMap::from([("window".into(), vec![WorkspaceSession::default()])]);
        {
            let tx = db.transaction()?;
            sessions::save(&tx, "window", &original)?;
            tx.commit()?;
        }
        {
            let tx = db.transaction()?;
            let invalid = BTreeMap::from([(
                "window".into(),
                vec![WorkspaceSession {
                    active: Some(0),
                    ..Default::default()
                }],
            )]);
            assert!(sessions::save(&tx, "window", &invalid).is_err());
        }
        assert_eq!(sessions::load(&db, "window")?, original);
        Ok(())
    }
}

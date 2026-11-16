//! Durable derived results, separated from user state to isolate cache writes.
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::{os::unix::fs::OpenOptionsExt, path::Path};

pub(crate) struct CacheStore {
    db: Connection,
}

impl CacheStore {
    pub(crate) fn open() -> Result<Self> {
        Self::open_at(&crate::preferences::directory()?)
    }

    pub(crate) fn open_at(directory: &Path) -> Result<Self> {
        std::fs::create_dir_all(directory)?;
        let path = directory.join("glim-cache.sqlite3");
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&path)?;
        let mut db = Connection::open(path).context("Cannot open Glim cache database")?;
        db.busy_timeout(std::time::Duration::from_millis(
            crate::limits::SQLITE_BUSY_MILLISECONDS as u64,
        ))?;
        db.execute_batch("PRAGMA synchronous=FULL;")?;
        let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        ensure!(
            version <= 1,
            "Glim cache database requires a newer application"
        );
        if version == 0 {
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let version: i64 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
            ensure!(
                version <= 1,
                "Glim cache database requires a newer application"
            );
            if version == 0 {
                tx.execute_batch(
                    "CREATE TABLE IF NOT EXISTS cache_entries(
                    kind TEXT NOT NULL, key BLOB NOT NULL, payload BLOB NOT NULL,
                    checksum BLOB NOT NULL, PRIMARY KEY(kind,key)
                 ) WITHOUT ROWID;
                 PRAGMA user_version=1;",
                )?;
            }
            tx.commit()?;
        }
        let mode: String = db.query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
        if !mode.eq_ignore_ascii_case("wal") {
            db.pragma_update(None, "journal_mode", "WAL")?;
        }
        Ok(Self { db })
    }

    pub(crate) fn load(&self, kind: &str, key: &[u8], maximum: usize) -> Result<Option<Vec<u8>>> {
        let entry = self
            .db
            .query_row(
                "SELECT payload,checksum FROM cache_entries
             WHERE kind=?1 AND key=?2 AND length(payload)<=?3 AND length(checksum)=32",
                (kind, key, i64::try_from(maximum)?),
                |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?)),
            )
            .optional()?;
        Ok(entry.and_then(|(payload, checksum)| {
            (Sha256::digest(&payload).as_slice() == checksum).then_some(payload)
        }))
    }

    pub(crate) fn remember(&self, kind: &str, key: &[u8], payload: &[u8]) -> Result<()> {
        let checksum = Sha256::digest(payload);
        self.db.execute(
            "INSERT INTO cache_entries(kind,key,payload,checksum) VALUES(?1,?2,?3,?4)
             ON CONFLICT(kind,key) DO UPDATE SET payload=excluded.payload,checksum=excluded.checksum",
            (kind, key, payload, checksum.as_slice()),
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reopening_preserves_all_entries_and_checks_each_payload() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let cache = CacheStore::open_at(directory.path())?;
        cache.remember("image", b"a", b"first")?;
        cache.remember("image", b"b", b"second")?;
        cache.remember("diagram", b"a", b"third")?;
        drop(cache);
        let cache = CacheStore::open_at(directory.path())?;
        assert_eq!(
            cache.load("image", b"a", 5)?.as_deref(),
            Some(&b"first"[..])
        );
        assert_eq!(
            cache.load("image", b"b", 6)?.as_deref(),
            Some(&b"second"[..])
        );
        assert_eq!(
            cache.load("diagram", b"a", 5)?.as_deref(),
            Some(&b"third"[..])
        );
        assert!(cache.load("image", b"a", 4)?.is_none());
        cache.db.execute(
            "UPDATE cache_entries SET payload=x'00' WHERE kind='image' AND key=x'61'",
            [],
        )?;
        assert!(cache.load("image", b"a", 5)?.is_none());
        cache.remember("image", b"a", b"fixed")?;
        assert_eq!(
            cache.load("image", b"a", 5)?.as_deref(),
            Some(&b"fixed"[..])
        );
        Ok(())
    }
}
